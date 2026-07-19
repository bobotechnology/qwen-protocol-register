const fs=require('fs');
const path=require('path');
const workDir=process.env.PIPELINE_OUT||path.join(__dirname,'output');
const input=path.join(workDir,'probe.json');
const output=path.join(workDir,'fresh_captcha_params.json');
const probe=JSON.parse(fs.readFileSync(input,'utf8'));
const candidates=(probe.calls?.jsonCalls||[]).flatMap(x=>{
  try{const v=JSON.parse(x.result);return v?.sceneId&&v?.certifyId&&v?.deviceToken&&v?.data?[v]:[]}catch{return []}
});
if(!candidates.length)throw new Error('generated CaptchaVerifyParam not found');
const value=candidates.at(-1);
fs.writeFileSync(output,JSON.stringify([value],null,2));
console.log(JSON.stringify({output,certifyId:value.certifyId,deviceTokenChars:value.deviceToken.length,dataChars:value.data.length}));