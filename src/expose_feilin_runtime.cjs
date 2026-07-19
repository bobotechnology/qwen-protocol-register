const fs = require('fs');
const crypto = require('crypto');
const parser = require('@babel/parser');
const traverse = require('@babel/traverse').default;
const gen = require('@babel/generator').default;
const t = require('@babel/types');

const input = process.argv[2];
const output = process.argv[3];
if (!input || !output) throw new Error('usage: node expose_feilin_runtime.cjs <raw.js> <derived.js>');

const source = fs.readFileSync(input, 'utf8');
const ast = parser.parse(source, { sourceType: 'script', allowReturnOutsideFunction: true });
const hash = crypto.createHash('sha256').update(source).digest('hex');
let exposed = 0;
let tkProbes = 0;
const matched = [];
const traceNames = new Set(['ic', 'is', 'il', 'ih', 'rL', 'rF', 'rJ', 'rk', 'rM']);

function probeStatement(kind, name) {
  return parser.parse(
    `if(typeof window!=='undefined'){window.__FEILIN_CALLS__=window.__FEILIN_CALLS__||[];window.__FEILIN_CALLS__.push({kind:${JSON.stringify(kind)},fn:${JSON.stringify(name)},args:Array.from(arguments),stack:String(new Error().stack).split('\\n').slice(1,7).join('\\n')})}`,
    { sourceType: 'script' }
  ).program.body[0];
}

function bodyText(node) {
  const start = node.start == null ? 0 : node.start;
  const end = node.end == null ? source.length : node.end;
  return source.slice(start, end);
}

function addEntryProbe(path, kind, name) {
  if (!path.node.body || !t.isBlockStatement(path.node.body)) return false;
  path.node.body.body.unshift(probeStatement(kind, name));
  matched.push({ kind, name });
  return true;
}

// All matches are optional. Function names and local layouts rotate frequently;
// structural observations must never prevent creation of the derived artifact.
traverse(ast, {
  FunctionDeclaration(path) {
    const name = path.node.id?.name || '<anonymous>';
    const text = bodyText(path.node.body);

    if (name === 'sw') {
      const names = ['se', 'sr', 'sw', 'sg', 'sy', 'rr', 'rL', 'rF', 'ic', 'is', 'il', 'ih', 'n2', 'rk', 'rM', 'rJ'];
      const props = names.map(x => t.objectProperty(t.stringLiteral(x), t.identifier(x)));
      path.insertAfter(t.ifStatement(
        t.binaryExpression('!==', t.unaryExpression('typeof', t.identifier('window')), t.stringLiteral('undefined')),
        t.expressionStatement(t.assignmentExpression('=', t.memberExpression(t.identifier('window'), t.identifier('__FEILIN_INTERNALS__')), t.objectExpression(props)))
      ));
      exposed++;
      matched.push({ kind: 'legacy-export', name });
    }

    if (name === 'n2') {
      path.traverse({ AssignmentExpression(q) {
        const n = q.node;
        if (!t.isIdentifier(n.left, { name: 'tk' }) || !t.isMemberExpression(n.right) ||
            !t.isIdentifier(n.right.object, { name: 'w' }) || !t.isIdentifier(n.right.property, { name: 'R' })) return;
        const original = t.cloneNode(n, true);
        q.replaceWith(t.sequenceExpression([
          t.assignmentExpression('=', t.memberExpression(t.identifier('window'), t.identifier('__N2_TK_PROBE__')),
            t.objectExpression([t.objectProperty(t.identifier('key'), t.identifier('R')), t.objectProperty(t.identifier('value'), t.memberExpression(t.identifier('w'), t.identifier('R'), true))])),
          original
        ]));
        q.skip();
        tkProbes++;
        matched.push({ kind: 'legacy-tk', name });
      }});
    }

    if (traceNames.has(name)) addEntryProbe(path, 'named-entry', name);

    const features = [
      ['device-collector', /dghfh565/],
      ['gather-cost', /GatherCost/],
      ['session-transform', /atob|split\(['"]#['"]\)|-h-/],
      ['captcha-input', /certifyId|VerifyCaptchaV2|sceneId/]
    ];
    for (const [kind, re] of features) {
      if (re.test(text)) { addEntryProbe(path, kind, name); break; }
    }
  },
  FunctionExpression(path) {
    const text = bodyText(path.node.body);
    const features = [['device-collector', /dghfh565/], ['gather-cost', /GatherCost/], ['session-transform', /atob|split\(['"]#['"]\)|-h-/]];
    for (const [kind, re] of features) {
      if (re.test(text)) { addEntryProbe(path, kind, '<expression>'); break; }
    }
  },
  AssignmentExpression(path) {
    const n = path.node;
    if (t.isIdentifier(n.left, { name: 'tk' }) && t.isMemberExpression(n.right) &&
        t.isIdentifier(n.right.object, { name: 'w' }) && t.isIdentifier(n.right.property, { name: 'R' })) {
      tkProbes++;
      matched.push({ kind: 'structural-tk', name: '<assignment>' });
    }
  }
});

const metadata = {
  version: 1,
  instrumented: true,
  mode: matched.length ? 'structural' : 'generic',
  sourceSha256: hash,
  matched,
  legacy: { exposed, tkProbes }
};
const marker = parser.parse(
  `if(typeof window!=='undefined'){window.__FEILIN_INSTRUMENTATION__=${JSON.stringify(metadata)};window.__FEILIN_INSTRUMENTATION__.loaded=true}`,
  { sourceType: 'script' }
).program.body;
ast.program.body.unshift(...marker);
fs.writeFileSync(output, gen(ast, { compact: false, comments: false }).code);
console.log(JSON.stringify({ output, instrumented: true, mode: metadata.mode, matched: matched.length, exposed, tkProbes }));
