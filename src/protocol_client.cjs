const crypto=require('crypto');
const {requestText}=require('./proxy_http.cjs');

const percent=s=>encodeURIComponent(String(s)).replace(/\+/g,'%20').replace(/\*/g,'%2A').replace(/%7E/g,'~');
const nonce=()=>crypto.randomUUID();
const timestamp=()=>new Date().toISOString().replace(/\.\d{3}Z$/,'Z');
function signV2(params,secret){
  const p={...params}; delete p.Signature;
  const canonical=Object.keys(p).sort().map(k=>`${percent(k)}=${percent(p[k])}`).join('&');
  const stringToSign=`POST&%2F&${percent(canonical)}`;
  return {canonical,stringToSign,signature:crypto.createHmac('sha1',`${secret}&`).update(stringToSign).digest('base64')};
}
function form(params){return new URLSearchParams(Object.entries(params).map(([k,v])=>[k,String(v)])).toString()}

// Browser-like headers to avoid F025 AI-based detection
const BROWSER_HEADERS={
  'accept': '*/*',
  'accept-language': 'zh-CN',
  'content-type': 'application/x-www-form-urlencoded; charset=UTF-8',
  'origin': 'https://chat.qwen.ai',
  'referer': 'https://chat.qwen.ai/auth?mode=register',
  'user-agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36',
  'sec-ch-ua': '"Not;A=Brand";v="8", "Chromium";v="150", "Google Chrome";v="150"',
  'sec-ch-ua-mobile': '?0',
  'sec-ch-ua-platform': '"Windows"',
};

async function postForm(url,params,extraHeaders){
  const r=await requestText(url,{
    method:'POST',
    headers:{...BROWSER_HEADERS,...extraHeaders},
    body:form(params)
  });
  return {status:r.status,headers:r.headers,text:r.text};
}
async function initCaptcha(cfg,input){
  const p={AccessKeyId:cfg.keyId,SignatureMethod:'HMAC-SHA1',SignatureVersion:'1.0',Format:'JSON',Timestamp:timestamp(),Version:'2023-03-05',Action:'InitCaptchaV2',...input,SignatureNonce:nonce()};
  p.Signature=signV2(p,cfg.keySecret).signature;
  return postForm(cfg.initEndpoint,p);
}
async function verifyCaptcha(cfg,input){
  const p={AccessKeyId:cfg.keyId,SignatureMethod:'HMAC-SHA1',SignatureVersion:'1.0',Format:'JSON',Timestamp:timestamp(),Version:'2023-03-05',Action:'VerifyCaptchaV2',...input,SignatureNonce:nonce()};
  p.Signature=signV2(p,cfg.keySecret).signature;
  return postForm(cfg.verifyEndpoint,p);
}
function passwordHash(password){return crypto.createHash('sha256').update(password,'utf8').digest('hex')}
function decodeRequestInfo(b64){return Buffer.from(b64,'base64').toString('utf8')}

module.exports={percent,nonce,timestamp,signV2,form,postForm,initCaptcha,verifyCaptcha,passwordHash,decodeRequestInfo,BROWSER_HEADERS};
if(require.main===module){
  const sample={Action:'InitCaptchaV2',Format:'JSON',SignatureMethod:'HMAC-SHA1',SignatureNonce:'00000000-0000-4000-8000-000000000000',SignatureVersion:'1.0',Timestamp:'2026-07-16T00:00:00Z',Version:'2023-03-05'};
  const out=signV2(sample,process.env.ALIYUN_CAPTCHA_KEY_SECRET||'REDACTED');
  console.log(JSON.stringify({passwordExample:passwordHash('Trace!Qwen#2026'),canonical:out.canonical,stringToSign:out.stringToSign,signature:process.env.ALIYUN_CAPTCHA_KEY_SECRET?out.signature:'set ALIYUN_CAPTCHA_KEY_SECRET to test'},null,2));
}
