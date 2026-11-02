use crate::artifact::ArtifactStore;
use crate::device::KeyStore;
use crate::sparse::extract_captcha_param;
use crate::transport::{HttpRequest, HttpTransport};
use aes::Aes128;
use anyhow::{Context, Result, bail};
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use rand::RngCore;
use rquickjs::{
    AsyncContext, AsyncRuntime, Ctx, Promise, Value,
    prelude::{Async, Func},
};
use serde_json::{Value as JsonValue, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const BROWSER_PRELUDE: &str = include_str!("browser.js");
const PROBE_DRIVER: &str = include_str!("probe_driver.js");

/// Build a small JS snippet that randomizes the QuickJS browser fingerprint
/// before BROWSER_PRELUDE runs. The goal is to produce a DIFFERENT Duane ID on
/// each attempt so Aliyun does not treat retries as duplicate submissions
/// (F008). Only low-risk fields are jittered — values stay inside the range a
/// real desktop Chrome would produce so F009 (virtual-device) is not triggered.
fn fingerprint_prelude() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    // Common desktop resolutions (width, height).
    const RESOLUTIONS: &[(u32, u32)] = &[
        (1920, 1080),
        (2560, 1440),
        (1366, 768),
        (1536, 864),
        (1440, 900),
        (1600, 900),
        (1280, 720),
    ];
    let (w, h) = RESOLUTIONS[rng.gen_range(0..RESOLUTIONS.len())];
    let hardware_concurrency = [4u32, 8, 12, 16][rng.gen_range(0..4)];
    let device_memory = [4u32, 8, 16][rng.gen_range(0..3)];
    let device_pixel_ratio = [1.0f64, 1.25, 1.5, 2.0][rng.gen_range(0..4)];
    // Chrome 149/150/151 with a realistic build/patch tail.
    let chrome_major = [149u32, 150, 151][rng.gen_range(0..3)];
    let chrome_build = rng.gen_range(7000..7900u32);
    let chrome_patch = rng.gen_range(50..200u32);
    let ua = format!(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{chrome_major}.0.{chrome_build}.{chrome_patch} Safari/537.36"
    );
    format!(
        r#"
(function() {{
  const ua = {ua:?};
  const w = {w}, h = {h};
  const dpr = {device_pixel_ratio};
  // Patch navigator.
  try {{
    Object.defineProperty(navigator, 'userAgent', {{get: () => ua, configurable: true}});
    Object.defineProperty(navigator, 'appVersion', {{get: () => ua.replace(/^Mozilla\//, ''), configurable: true}});
    Object.defineProperty(navigator, 'hardwareConcurrency', {{get: () => {hardware_concurrency}, configurable: true}});
    Object.defineProperty(navigator, 'deviceMemory', {{get: () => {device_memory}, configurable: true}});
    if (navigator.userAgentData) {{
      navigator.userAgentData.getHighEntropyValues = async keys => {{
        const brands = [{{brand:'Not;A=Brand',version:'99'}},{{brand:'Chromium',version:'{chrome_major}'}},{{brand:'Google Chrome',version:'{chrome_major}'}}];
        const full = [{{brand:'Not;A=Brand',version:'99.0.0.0'}},{{brand:'Chromium',version:'{chrome_major}.0.{chrome_build}.{chrome_patch}'}},{{brand:'Google Chrome',version:'{chrome_major}.0.{chrome_build}.{chrome_patch}'}}];
        const all = {{architecture:'x86',bitness:'64',brands,fullVersionList:full,mobile:false,model:'',platform:'Windows',platformVersion:'10.0.0',uaFullVersion:'{chrome_major}.0.{chrome_build}.{chrome_patch}',wow64:false}};
        return Object.fromEntries(keys.map(k => [k, all[k] == null ? '' : all[k]]));
      }};
    }}
  }} catch (e) {{}}
  // Patch screen + viewport.
  try {{
    const screen = {{width: w, height: h, availWidth: w, availHeight: h, colorDepth: 24, pixelDepth: 24, orientation: {{type: 'landscape-primary', angle: 0}}}};
    Object.defineProperty(globalThis, 'screen', {{get: () => screen, configurable: true}});
    Object.defineProperty(globalThis, 'devicePixelRatio', {{get: () => dpr, configurable: true}});
    Object.defineProperty(globalThis, 'innerWidth', {{get: () => w, configurable: true}});
    Object.defineProperty(globalThis, 'innerHeight', {{get: () => h, configurable: true}});
    Object.defineProperty(globalThis, 'outerWidth', {{get: () => w + 15, configurable: true}});
    Object.defineProperty(globalThis, 'outerHeight', {{get: () => h + 15, configurable: true}});
  }} catch (e) {{}}
}})();
"#
    )
}


#[derive(Clone)]
pub struct ProbeInput {
    pub target_source: String,
    pub feilin_source: String,
    pub dynamic_source: String,
    pub init_response: JsonValue,
    pub device_fixture: JsonValue,
    pub source_offset: u32,
    pub transport: Arc<dyn HttpTransport>,
    pub output_dir: PathBuf,
    pub scene_id: Option<String>,
    pub app_name: Option<String>,
    pub require_telemetry: bool,
}

#[derive(Debug, Clone)]
pub struct ProbeOutput {
    pub probe: JsonValue,
    pub param: JsonValue,
    pub keys: Option<KeyStore>,
}

pub async fn run_probe(input: ProbeInput) -> Result<ProbeOutput> {
    tokio::task::spawn_blocking(move || run_probe_sync(input))
        .await
        .context("QuickJS worker panicked")?
}

fn run_probe_sync(input: ProbeInput) -> Result<ProbeOutput> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    let output_dir = input.output_dir.clone();
    let artifact_dir = output_dir.clone();
    let probe = local.block_on(&runtime, async {
        let runtime = AsyncRuntime::new().map_err(|error| anyhow::anyhow!("{error:?}"))?;
        runtime.set_memory_limit(128 * 1024 * 1024).await;
        runtime.set_max_stack_size(2 * 1024 * 1024).await;
        let started = Instant::now();
        runtime
            .set_interrupt_handler(Some(Box::new(move || {
                started.elapsed() > Duration::from_secs(60)
            })))
            .await;
        let context = AsyncContext::full(&runtime)
            .await
            .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        let target = expose_crypto(&input.target_source);
        let feilin = input.feilin_source.clone();
        let dynamic = input.dynamic_source.clone();
        let transport = input.transport.clone();
        let init = input.init_response.clone();
        let fixture = input.device_fixture.clone();
        let scene_id = input.scene_id.clone();
        let app_name = input.app_name.clone();
        let source_offset = input.source_offset;
        let require_telemetry = input.require_telemetry;
        context
            .async_with(async move |ctx| {
                install_native(&ctx, transport, artifact_dir)
                    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                ctx.eval::<(), _>(BROWSER_PRELUDE)
                    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                // Randomize the browser fingerprint so each probe produces a
                // distinct Duane ID and Aliyun does not flag retries as F008.
                ctx.eval::<(), _>(fingerprint_prelude())
                    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                install_probe_config(
                    &ctx,
                    &init,
                    &fixture,
                    scene_id,
                    app_name,
                    source_offset,
                    require_telemetry,
                )
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                ctx.eval::<(), _>(target)
                    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                ctx.eval::<(), _>(feilin)
                    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                ctx.eval::<(), _>(dynamic)
                    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                let promise: Promise = ctx
                    .eval(PROBE_DRIVER.to_string() + "\n__run_probe()")
                    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
                promise
                    .into_future::<String>()
                    .await
                    .map_err(|error| anyhow::anyhow!("{error:?}"))
            })
            .await
            .map_err(|error| anyhow::anyhow!("QuickJS probe failed: {error:?}"))
    })?;
    let probe: JsonValue =
        serde_json::from_str(&probe).context("QuickJS returned invalid probe JSON")?;
    let store = ArtifactStore::new(output_dir)?;
    store.write_json("probe.json", &probe)?;
    if let Some(error) = probe.get("error").and_then(JsonValue::as_str)
        && !error.is_empty()
    {
        bail!("QuickJS runtime: {error}");
    }
    let param = extract_captcha_param(&probe)?;
    let keys = probe
        .get("calls")
        .and_then(|calls| calls.get("feilinCalls"))
        .and_then(extract_runtime_keys);
    store.write_json("fresh_captcha_params.json", &json!([param]))?;
    if let Some(keys) = &keys {
        let value =
            captured_key_value(&probe).unwrap_or_else(|| json!({"accessSec": keys.access_sec}));
        store.atomic_write_json("fresh_feilin_keys.json", &value)?;
    }
    Ok(ProbeOutput { probe, param, keys })
}

fn expose_crypto(source: &str) -> String {
    let anchor = "  e.n = function (t) {";
    if source.contains("e(9015)") && source.contains(anchor) {
        source.replacen(
            anchor,
            "  window.CryptoJS = e(9015); window.__ALIYUN_CRYPT = window.CryptoJS;\n  e.n = function (t) {",
            1,
        )
    } else {
        source.to_string()
    }
}

fn install_probe_config<'js>(
    ctx: &Ctx<'js>,
    init: &JsonValue,
    fixture: &JsonValue,
    scene_id: Option<String>,
    app_name: Option<String>,
    source_offset: u32,
    require_telemetry: bool,
) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    globals.set("__INIT_RESPONSE__", ctx.json_parse(init.to_string())?)?;
    let device = ctx.json_parse(fixture.to_string())?;
    globals.set("__DEVICE_FIXTURE__", device)?;
    globals.set("__PROBE_SOURCE_OFFSET__", source_offset)?;
    globals.set("__PROBE_REQUIRE_TELEMETRY__", require_telemetry)?;
    globals.set("__PROBE_MODE__", "embed")?;
    globals.set("__PROBE_CAPTCHA_MODE__", "popup")?;
    globals.set("__PROBE_GATHER_COST__", 725)?;
    globals.set(
        "__PROBE_FEILIN_WAIT_MS__",
        std::env::var("PROBE_FEILIN_WAIT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(8000),
    )?;
    if let Some(scene_id) = scene_id {
        globals.set("__PROBE_SCENE_ID__", scene_id)?;
    }
    if let Some(app_name) = app_name {
        globals.set("__PROBE_APP_NAME__", app_name)?;
    }
    ctx.eval::<(), _>(
        "if(globalThis.__DEVICE_FIXTURE__)Object.defineProperty(globalThis.__DEVICE_FIXTURE__,'GatherCost',{value:Number(globalThis.__PROBE_GATHER_COST__||725),writable:true,enumerable:false,configurable:true});",
    )?;
    Ok(())
}

fn install_native<'js>(
    ctx: &Ctx<'js>,
    transport: Arc<dyn HttpTransport>,
    output_dir: PathBuf,
) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    globals.set("__uuid", Func::from(|| uuid::Uuid::new_v4().to_string()))?;
    globals.set(
        "__random_bytes",
        Func::from(|length: usize| {
            let mut bytes = vec![0u8; length.min(1024 * 1024)];
            rand::thread_rng().fill_bytes(&mut bytes);
            bytes
        }),
    )?;
    globals.set(
        "__url_parse",
        Func::from(|input: String, base: String| -> rquickjs::Result<String> {
            let parsed = if base.is_empty() {
                url::Url::parse(&input)
            } else {
                url::Url::parse(&base).and_then(|base| base.join(&input))
            }
            .map_err(|_| rquickjs::Error::Exception)?;
            let origin = format!(
                "{}://{}",
                parsed.scheme(),
                parsed.host_str().unwrap_or_default()
            );
            let origin = parsed
                .port()
                .map_or(origin.clone(), |port| format!("{origin}:{port}"));
            Ok(json!({
                "href": parsed.to_string(),
                "protocol": format!("{}:", parsed.scheme()),
                "host": parsed.host_str().unwrap_or_default(),
                "hostname": parsed.host_str().unwrap_or_default(),
                "pathname": parsed.path(),
                "origin": origin,
                "search": parsed.query().unwrap_or_default(),
                "hash": parsed.fragment().map(|value| format!("#{value}")).unwrap_or_default(),
            })
            .to_string())
        }),
    )?;
    globals.set(
        "__digest",
        Func::from(
            |algorithm: String, bytes: Vec<u8>| -> rquickjs::Result<Vec<u8>> {
                if !algorithm.to_ascii_lowercase().contains("256") {
                    return Err(rquickjs::Error::Exception);
                }
                Ok(Sha256::digest(bytes).to_vec())
            },
        ),
    )?;
    globals.set(
        "__aes_encrypt",
        Func::from(
            |key: Vec<u8>, iv: Vec<u8>, plaintext: Vec<u8>| -> rquickjs::Result<Vec<u8>> {
                type Cipher = cbc::Encryptor<Aes128>;
                Ok(Cipher::new_from_slices(&key, &iv)
                    .map_err(|_| rquickjs::Error::Exception)?
                    .encrypt_padded_vec_mut::<Pkcs7>(&plaintext))
            },
        ),
    )?;
    globals.set(
        "__aes_decrypt",
        Func::from(
            |key: Vec<u8>, iv: Vec<u8>, ciphertext: Vec<u8>| -> rquickjs::Result<Vec<u8>> {
                type Cipher = cbc::Decryptor<Aes128>;
                Cipher::new_from_slices(&key, &iv)
                    .map_err(|_| rquickjs::Error::Exception)?
                    .decrypt_padded_vec_mut::<Pkcs7>(&ciphertext)
                    .map_err(|_| rquickjs::Error::Exception)
            },
        ),
    )?;
    globals.set(
        "__native_timeout",
        Func::from(Async(|milliseconds: f64| async move {
            tokio::time::sleep(Duration::from_secs_f64(
                (milliseconds / 1000.0).clamp(0.0, 300.0),
            ))
            .await;
            Ok::<(), rquickjs::Error>(())
        })),
    )?;
    globals.set(
        "__artifact_write",
        Func::from(move |name: String, body: String| -> rquickjs::Result<()> {
            if !matches!(name.as_str(), "vm_log2_body.txt" | "vm_log3_body.txt") {
                return Err(rquickjs::Error::Exception);
            }
            std::fs::create_dir_all(&output_dir)?;
            std::fs::write(output_dir.join(name), body)?;
            Ok(())
        }),
    )?;
    let xhr_transport = transport;
    globals.set(
        "__xhr_request",
        Func::from(Async(
            move |method: String, url: String, headers: String, body: String, timeout: u64| {
                let transport = xhr_transport.clone();
                async move {
                    let headers: BTreeMap<String, String> =
                        serde_json::from_str(&headers).map_err(|_| rquickjs::Error::Exception)?;
                    let request = HttpRequest {
                        method,
                        url,
                        headers,
                        body: (!body.is_empty()).then(|| body.into_bytes()),
                        timeout: Duration::from_millis(timeout.clamp(1, 120_000)),
                    };
                    let response = transport
                        .execute(request)
                        .await
                        .map_err(|_| rquickjs::Error::Exception)?;
                    Ok::<String, rquickjs::Error>(
                        json!({
                            "status": response.status,
                            "statusText": response.status_text,
                            "headers": response.headers,
                            "url": response.url,
                            "text": response.text(),
                        })
                        .to_string(),
                    )
                }
            },
        )),
    )?;
    Ok(())
}

fn extract_runtime_keys(calls: &JsonValue) -> Option<KeyStore> {
    let entries = calls.as_array()?;
    let rc_calls = entries
        .iter()
        .filter(|entry| entry.get("fn").and_then(JsonValue::as_str) == Some("rc"))
        .collect::<Vec<_>>();
    if rc_calls.is_empty() {
        return None;
    }
    let access_sec = rc_calls.iter().find_map(|entry| {
        let args = entry.get("args")?.as_array()?;
        let key = args.get(1)?.as_str()?;
        (key.chars().count() == 16).then(|| key.to_string())
    })?;
    let mut slots = Vec::new();
    for entry in rc_calls {
        let Some(args) = entry.get("args").and_then(JsonValue::as_array) else {
            continue;
        };
        let Some(key) = args.get(1).and_then(JsonValue::as_str) else {
            continue;
        };
        let Some(value) = args.get(2).and_then(JsonValue::as_str) else {
            continue;
        };
        if key == access_sec
            && value.len() >= 24
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "+/=".contains(character))
            && !slots.iter().any(|existing| existing == value)
        {
            slots.push(value.to_string());
            if slots.len() == 5 {
                break;
            }
        }
    }
    if slots.is_empty() {
        return None;
    }
    KeyStore::from_capture(&json!({"accessSec": access_sec, "webAesKeys": slots}))
}

fn captured_key_value(probe: &JsonValue) -> Option<JsonValue> {
    let entries = probe.get("calls")?.get("feilinCalls")?.as_array()?;
    let keys = extract_runtime_keys(&JsonValue::Array(entries.clone()))?;
    let mut slots = Vec::new();
    for entry in entries {
        if entry.get("fn").and_then(JsonValue::as_str) != Some("rc") {
            continue;
        }
        let args = entry.get("args")?.as_array()?;
        if args.get(1).and_then(JsonValue::as_str) == Some(keys.access_sec.as_str())
            && let Some(value) = args.get(2).and_then(JsonValue::as_str)
            && value.len() >= 24
            && !slots.iter().any(|existing| existing == value)
        {
            slots.push(value.to_string());
            if slots.len() == 5 {
                break;
            }
        }
    }
    Some(json!({
        "accessSec": keys.access_sec,
        "webAesKeys": slots,
        "source": "feilinCalls",
        "count": entries.len(),
    }))
}

pub fn validate_javascript(source: &str) -> Result<()> {
    let runtime = rquickjs::Runtime::new().map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let context =
        rquickjs::Context::full(&runtime).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    context.with(|ctx| {
        let function = format!("(function(){{\n{source}\n}})");
        ctx.eval::<Value, _>(function)
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("invalid JavaScript: {error:?}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::HttpResponse;
    use async_trait::async_trait;

    struct NoNetwork;
    #[async_trait]
    impl HttpTransport for NoNetwork {
        async fn execute(&self, _request: HttpRequest) -> Result<HttpResponse> {
            bail!("unexpected network request")
        }
    }

    #[test]
    fn browser_prelude_and_crypto_execute_in_quickjs() {
        let runtime = rquickjs::Runtime::new().unwrap();
        let context = rquickjs::Context::full(&runtime).unwrap();
        context.with(|ctx| {
            install_native(&ctx, Arc::new(NoNetwork), PathBuf::from(".")).unwrap();
            ctx.eval::<(), _>(BROWSER_PRELUDE).unwrap();
            let result: String = ctx.eval("btoa('hello') + ':' + atob('aGVsbG8=') + ':' + new URLSearchParams('a=x+y').get('a')").unwrap();
            assert_eq!(result, "aGVsbG8=:hello:x y");
            let digest: Vec<u8> = ctx.eval("Array.from(new Uint8Array(__digest('SHA-256', Array.from(new TextEncoder().encode('abc')))))").unwrap();
            assert_eq!(hex::encode(digest), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        });
    }

    #[test]
    fn checked_in_bundle_initializes_in_quickjs() {
        let source = std::fs::read_to_string("fixtures/AliyunCaptcha.exposed.js").unwrap();
        let runtime = rquickjs::Runtime::new().unwrap();
        let context = rquickjs::Context::full(&runtime).unwrap();
        context.with(|ctx| {
            install_native(&ctx, Arc::new(NoNetwork), PathBuf::from(".")).unwrap();
            ctx.eval::<(), _>(BROWSER_PRELUDE).unwrap();
            ctx.eval::<(), _>(expose_crypto(&source)).unwrap();
            assert!(
                ctx.eval::<bool, _>("!!globalThis.__PROTO_INTERNALS__")
                    .unwrap()
            );
            assert!(ctx.eval::<bool, _>("!!globalThis.CryptoJS.AES").unwrap());
        });
    }

    #[tokio::test]
    #[ignore = "opt-in replay of a previously captured live artifact directory"]
    async fn live_artifact_replay() {
        let Some(directory) = std::env::var_os("QWEN_REPLAY_DIR").map(PathBuf::from) else {
            return;
        };
        let manifest: JsonValue = serde_json::from_slice(
            &std::fs::read(directory.join("fresh_dynamic_manifest.json")).unwrap(),
        )
        .unwrap();
        let offset: JsonValue =
            serde_json::from_slice(&std::fs::read(directory.join("fresh_offset.json")).unwrap())
                .unwrap();
        let output = run_probe(ProbeInput {
            target_source: std::fs::read_to_string("fixtures/AliyunCaptcha.exposed.js").unwrap(),
            feilin_source: std::fs::read_to_string(
                manifest["feilin"]["exposedPath"].as_str().unwrap(),
            )
            .unwrap(),
            dynamic_source: std::fs::read_to_string(
                manifest["dynamicJS"]["exposedPath"].as_str().unwrap(),
            )
            .unwrap(),
            init_response: serde_json::from_slice(
                &std::fs::read(directory.join("fresh_init_response.json")).unwrap(),
            )
            .unwrap(),
            device_fixture: serde_json::from_slice(
                &std::fs::read(directory.join("adapted_device_object.json")).unwrap(),
            )
            .unwrap(),
            source_offset: offset["sourceOffset"].as_u64().unwrap() as u32,
            transport: Arc::new(crate::transport::ReqwestTransport::new(None).unwrap()),
            output_dir: directory,
            scene_id: None,
            app_name: Some("saf-captcha-waf".into()),
            require_telemetry: false,
        })
        .await
        .unwrap();
        assert!(
            output
                .param
                .get("data")
                .and_then(JsonValue::as_str)
                .is_some()
        );
    }

    #[test]
    fn validates_script_without_executing_it() {
        assert!(validate_javascript("throw new Error('must not execute')").is_ok());
        assert!(validate_javascript("function {").is_err());
    }

    #[test]
    fn extracts_runtime_keys_from_probe_shape() {
        let value = json!([
            {"fn":"rc","args":["x","0123456789abcdef","cipher-one-cipher-one"]},
            {"fn":"rc","args":["x","0123456789abcdef","cipher-two-cipher-two"]}
        ]);
        assert!(extract_runtime_keys(&value).is_none());
    }
}
