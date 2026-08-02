const fs=require('fs');
const [input,output]=process.argv.slice(2);
if(!input||!output)throw new Error('usage: node expose_dynamic_runtime.cjs input.js output.js');
const src=fs.readFileSync(input,'utf8');
const ctor=[...src.matchAll(/t\.AliyunCaptcha=([A-Za-z_$][\w$]*)/g)].at(-1)?.[1];
const pow=src.match(/PowVerifyString\)\{([A-Za-z_$][\w$]*)\.reset\(\)/)?.[1];
if(!ctor||!pow)throw new Error(`dynamic anchors not found: ctor=${ctor} pow=${pow}`);

// Exact match on the known export tail pattern. When Ali rotates whitespace or
// variable names we fall back to a structural regex so a single release doesn't
// break the entire pipeline.
let needle=`t=window,NodeList.prototype.forEach=Array.prototype.forEach,t.AliyunCaptcha=${ctor}`;
let matchIndex=src.indexOf(needle);
if(matchIndex===-1){
  // Structural fallback: [...]{...}=window, NodeList.prototype.forEach[...], ...AliyunCaptcha=ctor
  const tailRe=new RegExp(
    `([A-Za-z_$][\\w$]*)`+                     // local var (e.g. "t") = window
    `\\s*=\\s*window\\s*,\\s*`+
    `NodeList\\.prototype\\.forEach\\s*=\\s*Array\\.prototype\\.forEach\\s*,\\s*`+
    `\\1\\.AliyunCaptcha\\s*=\\s*`+            // same var .AliyunCaptcha = ctor
    `${ctor.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')}`,
    'g'
  );
  const tailMatch=tailRe.exec(src);
  if(!tailMatch)throw new Error(`export tail structural regex also failed (ctor=${ctor})`);
  needle=tailMatch[0];
  matchIndex=tailMatch.index;
}
const expose=`window.__DYN_INTERNALS__={ctor:${ctor},nf:${pow},state:function(){return {}}};${needle}`;
const result=src.slice(0,matchIndex)+expose+src.slice(matchIndex+needle.length);
fs.writeFileSync(output,result);
console.log(JSON.stringify({input,output,ctor,pow,bytes:src.length}));
