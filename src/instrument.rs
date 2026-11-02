use anyhow::{Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use swc_common::{FileName, SourceMap, Spanned, sync::Lrc};
use swc_ecma_ast::*;
use swc_ecma_parser::{Parser, StringInput, Syntax, lexer::Lexer};
use swc_ecma_visit::{VisitMut, VisitMutWith};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicInstrumentation {
    pub code: String,
    pub ctor: String,
    pub pow: String,
    pub bytes: usize,
}

pub fn expose_dynamic_runtime(source: &str) -> Result<DynamicInstrumentation> {
    static CTOR: OnceLock<Regex> = OnceLock::new();
    static POW: OnceLock<Regex> = OnceLock::new();
    let ctor_re = CTOR.get_or_init(|| {
        Regex::new(r"t\.AliyunCaptcha=([A-Za-z_$][\w$]*)").expect("valid constructor regex")
    });
    let pow_re = POW.get_or_init(|| {
        Regex::new(r"PowVerifyString[^\{]*\{\s*([A-Za-z_$][\w$]*)\.reset\(\)")
            .expect("valid PoW regex")
    });
    let ctor = ctor_re
        .captures_iter(source)
        .last()
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str());
    let pow = pow_re
        .captures(source)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str());
    let (Some(ctor), Some(pow)) = (ctor, pow) else {
        bail!("dynamic anchors not found");
    };
    let needle = format!(
        "t=window,NodeList.prototype.forEach=Array.prototype.forEach,t.AliyunCaptcha={ctor}"
    );
    let (index, matched) = if let Some(index) = source.find(&needle) {
        (index, needle)
    } else {
        let escaped = regex::escape(ctor);
        let fallback = Regex::new(&format!(
            r"([A-Za-z_$][\w$]*)\s*=\s*window\s*,\s*NodeList\.prototype\.forEach\s*=\s*Array\.prototype\.forEach\s*,\s*([A-Za-z_$][\w$]*)\.AliyunCaptcha\s*=\s*{escaped}"
        ))?;
        let captures = fallback
            .captures(source)
            .ok_or_else(|| anyhow::anyhow!("export tail structural regex also failed"))?;
        if captures.get(1).map(|value| value.as_str())
            != captures.get(2).map(|value| value.as_str())
        {
            bail!("export tail window alias mismatch");
        }
        let found = captures.get(0).expect("full regex match");
        (found.start(), found.as_str().to_string())
    };
    let expose = format!(
        "window.__DYN_INTERNALS__={{ctor:{ctor},nf:{pow},state:function(){{return {{}}}}}};{matched}"
    );
    let code = format!(
        "{}{}{}",
        &source[..index],
        expose,
        &source[index + matched.len()..]
    );
    parse_script(&code)?;
    Ok(DynamicInstrumentation {
        code,
        ctor: ctor.into(),
        pow: pow.into(),
        bytes: source.len(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchRecord {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentationMetadata {
    pub version: u32,
    pub instrumented: bool,
    pub mode: String,
    #[serde(rename = "sourceSha256")]
    pub source_sha256: String,
    pub matched: Vec<MatchRecord>,
    pub legacy: LegacyCounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LegacyCounts {
    pub exposed: usize,
    #[serde(rename = "tkProbes")]
    pub tk_probes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeilinInstrumentation {
    pub code: String,
    pub metadata: InstrumentationMetadata,
}

fn parse_script(source: &str) -> Result<Script> {
    let cm: Lrc<SourceMap> = Default::default();
    let file = cm.new_source_file(
        FileName::Custom("captcha.js".into()).into(),
        source.to_string(),
    );
    let lexer = Lexer::new(
        Syntax::Es(Default::default()),
        EsVersion::latest(),
        StringInput::from(&*file),
        None,
    );
    let mut parser = Parser::new_from(lexer);
    let mut recovered = Vec::new();
    let script = parser
        .parse_script()
        .map_err(|error| anyhow::anyhow!("JavaScript parse failed: {error:?}"))?;
    recovered.extend(parser.take_errors());
    if let Some(error) = recovered.first() {
        bail!("JavaScript parse failed: {error:?}");
    }
    Ok(script)
}

fn source_of<'a, T: Spanned>(source: &'a str, node: &T) -> &'a str {
    let span = node.span();
    let start = span.lo.0.saturating_sub(1) as usize;
    let end = span.hi.0.saturating_sub(1) as usize;
    source.get(start..end).unwrap_or_default()
}

fn features(source: &str, body: &FunctionBody) -> Vec<&'static str> {
    let text = source_of(source, body);
    let mut result = Vec::new();
    if text.contains("dghfh565") {
        result.push("device-collector");
    }
    if text.contains("GatherCost") {
        result.push("gather-cost");
    }
    if text.contains("atob")
        || text.contains("-h-")
        || text.contains("split('#')")
        || text.contains("split(\"#\")")
    {
        result.push("session-transform");
    }
    if text.contains("certifyId") || text.contains("VerifyCaptchaV2") || text.contains("sceneId") {
        result.push("captcha-input");
    }
    result
}

fn is_tk_assignment(assign: &AssignExpr) -> bool {
    let Some(left) = assign.left.as_ident() else {
        return false;
    };
    let Expr::Member(member) = &*assign.right else {
        return false;
    };
    let Expr::Ident(object) = &*member.obj else {
        return false;
    };
    left.sym == *"tk"
        && object.sym == *"w"
        && (matches!(&member.prop, MemberProp::Ident(property) if property.sym == *"R")
            || matches!(&member.prop, MemberProp::Computed(computed)
                if matches!(&*computed.expr, Expr::Ident(property) if property.sym == *"R")))
}

#[derive(Default)]
struct FeilinTransformer {
    source: String,
    matched: Vec<MatchRecord>,
    exposed: usize,
    tk_probes: usize,
    function_depth: usize,
    n2_pending: bool,
}

impl FeilinTransformer {
    fn record(&mut self, kind: &str, name: &str) {
        self.matched.push(MatchRecord {
            kind: kind.into(),
            name: name.into(),
        });
    }
}

impl VisitMut for FeilinTransformer {
    fn visit_mut_fn_decl(&mut self, node: &mut FnDecl) {
        let name = node.ident.sym.to_string();
        let feature = node
            .function
            .body
            .as_ref()
            .map(|body| features(&self.source, body))
            .unwrap_or_default();
        let trace =
            ["ic", "is", "il", "ih", "rL", "rF", "rJ", "rk", "rM", "rc"].contains(&name.as_str());
        let probe_kind = if trace {
            Some("named-entry")
        } else {
            feature.first().copied()
        };
        if let Some(body) = &mut node.function.body
            && let Some(kind) = probe_kind
        {
            let statement = format!(
                "if(typeof window!=='undefined'){{window.__FEILIN_CALLS__=window.__FEILIN_CALLS__||[];window.__FEILIN_CALLS__.push({{kind:{},fn:{},args:Array.from(arguments),stack:String(new Error().stack).split('\\n').slice(1,7).join('\\n')}})}}",
                serde_json::to_string(kind).unwrap(),
                serde_json::to_string(&name).unwrap()
            );
            if let Ok(probe) = parse_script(&statement)
                && let Some(probe_statement) = probe.body.into_iter().next()
            {
                body.stmts.insert(0, probe_statement);
            }
        }
        if trace {
            self.record("named-entry", &name);
        }
        if let Some(kind) = feature.first() {
            self.record(kind, &name);
        }
        if name == "sw" {
            self.exposed += 1;
            self.record("legacy-export", &name);
        }
        if name == "n2" {
            self.n2_pending = true;
        }
        self.function_depth += 1;
        node.visit_mut_children_with(self);
        self.function_depth -= 1;
    }

    fn visit_mut_fn_expr(&mut self, node: &mut FnExpr) {
        let feature = node
            .function
            .body
            .as_ref()
            .map(|body| features(&self.source, body))
            .unwrap_or_default();
        if let Some(kind) = feature.first().copied()
            && !matches!(kind, "captcha-input")
        {
            if let Some(body) = &mut node.function.body {
                let statement = format!(
                    "if(typeof window!=='undefined'){{window.__FEILIN_CALLS__=window.__FEILIN_CALLS__||[];window.__FEILIN_CALLS__.push({{kind:{},fn:'<expression>',args:Array.from(arguments),stack:String(new Error().stack).split('\\n').slice(1,7).join('\\n')}})}}",
                    serde_json::to_string(kind).unwrap()
                );
                if let Ok(probe) = parse_script(&statement)
                    && let Some(probe_statement) = probe.body.into_iter().next()
                {
                    body.stmts.insert(0, probe_statement);
                }
            }
            self.record(kind, "<expression>");
        }
        self.function_depth += 1;
        node.visit_mut_children_with(self);
        self.function_depth -= 1;
    }

    fn visit_mut_stmts(&mut self, statements: &mut Vec<Stmt>) {
        statements.visit_mut_children_with(self);
        let mut output = Vec::with_capacity(statements.len());
        for statement in statements.drain(..) {
            let is_sw =
                matches!(&statement, Stmt::Decl(Decl::Fn(function)) if function.ident.sym == *"sw");
            if is_sw {
                let exposure = "if(typeof window!=='undefined'){window.__FEILIN_INTERNALS__={se:se,sr:sr,sw:sw,sg:sg,sy:sy,rr:rr,rL:rL,rF:rF,ic:ic,is:is,il:il,ih:ih,n2:n2,rk:rk,rM:rM,rJ:rJ};window.__FEILIN_KEYS__=window.__FEILIN_KEYS__||[];var _origRk=window.__FEILIN_INTERNALS__.rk;var _origRm=window.__FEILIN_INTERNALS__.rM;window.__FEILIN_INTERNALS__.rk=function(key,text){window.__FEILIN_KEYS__.push({fn:'rk',key:String(key),len:key.length});return _origRk.call(this,key,text)};window.__FEILIN_INTERNALS__.rM=function(key,text){window.__FEILIN_KEYS__.push({fn:'rM',key:String(key),len:key.length});return _origRm.call(this,key,text)};}";
                if let Ok(script) = parse_script(exposure) {
                    output.extend(script.body);
                }
            }
            output.push(statement);
        }
        *statements = output;
    }

    fn visit_mut_expr(&mut self, expression: &mut Expr) {
        expression.visit_mut_children_with(self);
        if let Expr::Assign(assign) = expression
            && is_tk_assignment(assign)
        {
            let original = assign.clone();
            let probe = format!(
                "(window.__N2_TK_PROBE__={{key:R,value:w[R]}}, {})",
                swc_ecma_codegen::to_code(&Expr::Assign(original))
            );
            if let Ok(parsed) = parse_script(&probe)
                && let Some(Stmt::Expr(statement)) = parsed.body.into_iter().next()
            {
                *expression = *statement.expr;
            }
            self.tk_probes += 1;
            self.record("structural-tk", "<assignment>");
        }
    }
}

pub fn expose_feilin_runtime(source: &str) -> Result<FeilinInstrumentation> {
    let mut script = parse_script(source)?;
    let mut transformer = FeilinTransformer {
        source: source.into(),
        ..FeilinTransformer::default()
    };
    script.visit_mut_with(&mut transformer);
    let metadata = InstrumentationMetadata {
        version: 1,
        instrumented: true,
        mode: if transformer.matched.is_empty() {
            "generic".into()
        } else {
            "structural".into()
        },
        source_sha256: hex::encode(Sha256::digest(source.as_bytes())),
        matched: transformer.matched.clone(),
        legacy: LegacyCounts {
            exposed: transformer.exposed,
            tk_probes: transformer.tk_probes,
        },
    };
    let marker = format!(
        "if(typeof window!=='undefined'){{window.__FEILIN_INSTRUMENTATION__={};window.__FEILIN_INSTRUMENTATION__.loaded=true}}",
        serde_json::to_string(&metadata)?
    );
    let mut body = parse_script(&marker)?.body;
    body.extend(script.body);
    script.body = body;
    let code = swc_ecma_codegen::to_code(&script);
    parse_script(&code)?;
    Ok(FeilinInstrumentation { code, metadata })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_inserts_internals_and_keeps_export() {
        let source = "function C(){};function P(){};P.prototype.PowVerifyString=function(){P.reset()};t.AliyunCaptcha=C;t=window,NodeList.prototype.forEach=Array.prototype.forEach,t.AliyunCaptcha=C";
        let output = expose_dynamic_runtime(source).unwrap();
        assert_eq!(output.ctor, "C");
        assert_eq!(output.pow, "P");
        assert!(output.code.contains("window.__DYN_INTERNALS__"));
        assert!(output.code.ends_with("t.AliyunCaptcha=C"));
    }

    #[test]
    fn dynamic_accepts_structural_tail_whitespace() {
        let source = "function C(){};P.prototype.PowVerifyString=function(){P.reset()};t.AliyunCaptcha=C;x = window , NodeList.prototype.forEach = Array.prototype.forEach , x.AliyunCaptcha = C";
        let output = expose_dynamic_runtime(source).unwrap();
        assert!(output.code.contains("ctor:C"));
    }

    #[test]
    fn feilin_adds_marker_entry_probe_and_exports() {
        let source = "function sw(){} function rM(k,v){return v} function n2(){tk=w[R]}";
        let output = expose_feilin_runtime(source).unwrap();
        assert!(output.code.contains("__FEILIN_INSTRUMENTATION__"));
        assert!(output.code.contains("__FEILIN_INTERNALS__"));
        assert!(output.code.contains("__FEILIN_CALLS__"));
        assert!(output.code.contains("__N2_TK_PROBE__"));
        assert_eq!(output.metadata.legacy.exposed, 1);
        assert!(
            output
                .metadata
                .matched
                .iter()
                .any(|entry| entry.name == "rM")
        );
    }

    #[test]
    fn feilin_invalid_javascript_is_rejected() {
        assert!(expose_feilin_runtime("function {").is_err());
    }
}
