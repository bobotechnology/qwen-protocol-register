const fs=require('fs');
const vm=require('vm');
const path=require('path');
const {requestText}=require('./proxy_http.cjs');

const target=process.argv[2];
if(!target) throw new Error('usage: node probe_bundle.cjs <bundle.js>');
const seen=new Set(), log=[];
const listeners=[];
const btoaCalls=[],jsonCalls=[],eventDispatches=[];
const eventDataOps=[];
const sanitizeNaN=(value,seen=new WeakSet())=>{if(typeof value==='string')return value.replace(/NaN/g,'0');if(!value||typeof value!=='object'||seen.has(value))return value;seen.add(value);for(const k of Object.keys(value))try{value[k]=sanitizeNaN(value[k],seen)}catch{}return value};
let lastDeviceData=null;
const JSONFacade={parse:JSON.parse,stringify(...args){
  // Preserve FeiLin's original object. The SG_WEB fixture and the Log2 upload
  // envelope are different contracts; replacing the object here made Log2
  // return "parameter formate error(data)" while making the token look sparse.
  if(args[0]?.dghfh565){
    lastDeviceData=args[0];
  }
  if(args[0]?.deviceConfig&&!args[0].deviceData&&lastDeviceData)args[0].deviceData=lastDeviceData;
  const result=JSON.stringify(...args);if(result&&result.length>20)jsonCalls.push({source:'native',result,stack:String(new Error().stack).split('\n').slice(1,9).join('\n')});return result
}};
let timerSeq=1;const timerHandles=new Map();
const browserSetTimeout=(fn,ms,...args)=>{const id=timerSeq++;if(typeof sandbox!=='undefined'&&sandbox.__DRAG_ACTIVE__)record('drag-timer',String(ms),String(fn).slice(0,180));timerHandles.set(id,setTimeout(()=>{timerHandles.delete(id);fn(...args)},ms));return id};
const browserClearTimeout=id=>{const h=timerHandles.get(id);if(h)clearTimeout(h);timerHandles.delete(id)};
const browserSetInterval=(fn,ms,...args)=>{const id=timerSeq++;timerHandles.set(id,setInterval(fn,ms,...args));return id};
const browserClearInterval=id=>{const h=timerHandles.get(id);if(h)clearInterval(h);timerHandles.delete(id)};
const record=(op,p,v)=>{const k=`${op} ${p}`;if(!seen.has(k)){seen.add(k);log.push({op,path:p,value:v});}};
const dummy=(name='dummy')=>new Proxy(function(){},{
  get(t,p){if(p===Symbol.toPrimitive)return()=>'';if(p==='then')return undefined;if(p==='prototype')return {};record('get',`${name}.${String(p)}`);return dummy(`${name}.${String(p)}`)},
  set(t,p,v){record('set',`${name}.${String(p)}`,typeof v);t[p]=v;return true},
  apply(t,th,args){record('call',name,args.map(x=>typeof x));return dummy(`${name}()`);},
  construct(t,args){record('new',name,args.map(x=>typeof x));return dummy(`new ${name}`)}
});
const elementCache=new Map();
const makeStyle=name=>new Proxy({cssText:'',display:'',visibility:'',setProperty(k,v){this[k]=String(v)},getPropertyValue(k){return this[k]||''}},{set(obj,k,v){if(sandbox?.__DRAG_ACTIVE__&&['left','width'].includes(String(k)))record('style-set',`${name}.${String(k)}=${v}`,String(new Error().stack).split('\n').slice(2,5).join(' | '));obj[k]=v;return true}});
// A deterministic Chrome/Windows graphics surface.  FeiLin reads the APIs, not
// screenshots, so stable browser-shaped return values are preferable to dummies.
const canvas2dContext=()=>({
  canvas:null,fillStyle:'#000000',strokeStyle:'#000000',font:'10px sans-serif',textBaseline:'alphabetic',globalCompositeOperation:'source-over',
  beginPath(){},closePath(){},moveTo(){},lineTo(){},arc(){},rect(){},clip(){},stroke(){},fill(){},clearRect(){},fillRect(){},strokeRect(){},
  fillText(){},strokeText(){},save(){},restore(){},translate(){},rotate(){},scale(){},transform(){},setTransform(){},drawImage(){},
  measureText(text){return {width:String(text).length*7.21875,actualBoundingBoxAscent:9,actualBoundingBoxDescent:2}},
  getImageData(x,y,w,h){const n=Math.max(0,Number(w||1)*Number(h||1)*4),data=new Uint8ClampedArray(n);for(let i=0;i<n;i+=4){data[i]=17;data[i+1]=34;data[i+2]=51;data[i+3]=255}return {data,width:Number(w||1),height:Number(h||1)}},
  putImageData(){},createImageData(w,h){return {data:new Uint8ClampedArray(Number(w)*Number(h)*4),width:Number(w),height:Number(h)}},
  createLinearGradient(){return {addColorStop(){}}},createRadialGradient(){return {addColorStop(){}}},createPattern(){return {setTransform(){}}},
  isPointInPath(){return false},isPointInStroke(){return false},setLineDash(){},getLineDash(){return []}
});
const webglContext=()=>{const ext={UNMASKED_VENDOR_WEBGL:37445,UNMASKED_RENDERER_WEBGL:37446};return {
  VENDOR:7936,RENDERER:7937,VERSION:7938,SHADING_LANGUAGE_VERSION:35724,MAX_TEXTURE_SIZE:3379,MAX_VIEWPORT_DIMS:3386,
  getExtension(name){return String(name)==='WEBGL_debug_renderer_info'?ext:null},
  getSupportedExtensions(){return ['ANGLE_instanced_arrays','EXT_blend_minmax','EXT_color_buffer_half_float','EXT_disjoint_timer_query','EXT_float_blend','EXT_frag_depth','EXT_shader_texture_lod','EXT_texture_compression_bptc','EXT_texture_filter_anisotropic','OES_element_index_uint','OES_fbo_render_mipmap','OES_standard_derivatives','OES_texture_float','OES_texture_float_linear','OES_texture_half_float','OES_texture_half_float_linear','OES_vertex_array_object','WEBGL_color_buffer_float','WEBGL_compressed_texture_s3tc','WEBGL_debug_renderer_info','WEBGL_debug_shaders','WEBGL_depth_texture','WEBGL_draw_buffers','WEBGL_lose_context','WEBGL_multi_draw']},
  getParameter(p){if(p===37445)return 'Google Inc. (Intel)';if(p===37446)return 'ANGLE (Intel, Intel(R) Graphics (0x00007D45) Direct3D11 vs_5_0 ps_5_0, D3D11)';if(p===7936)return 'WebKit';if(p===7937)return 'WebKit WebGL';if(p===7938)return 'WebGL 1.0 (OpenGL ES 2.0 Chromium)';if(p===35724)return 'WebGL GLSL ES 1.0 (OpenGL ES GLSL ES 1.0 Chromium)';if(p===3379)return 16384;if(p===3386)return new Int32Array([32767,32767]);return 0},
  createBuffer(){return {}},bindBuffer(){},bufferData(){},createProgram(){return {}},createShader(){return {}},shaderSource(){},compileShader(){},attachShader(){},linkProgram(){},useProgram(){},getAttribLocation(){return 0},getUniformLocation(){return {}},enableVertexAttribArray(){},vertexAttribPointer(){},uniform2f(){},drawArrays(){},viewport(){},clearColor(){},clear(){},readPixels(x,y,w,h,fmt,type,out){if(out?.fill)out.fill(127)}
}};
const makeElement=(name='element')=>({tagName:'DIV',id:String(name).replace(/^#/,'').replace(/[^\w-].*$/,''),style:makeStyle(name),className:'',innerHTML:'',onclick:null,offsetLeft:0,offsetTop:0,offsetWidth:String(name).includes('slider')?40:String(name).includes('puzzle')?52:300,clientWidth:String(name).includes('slider')?40:String(name).includes('puzzle')?52:300,offsetHeight:40,clientHeight:40,naturalWidth:String(name).includes('puzzle')?52:296,naturalHeight:180,complete:true,
  setAttribute(k,v){this[k]=v},getAttribute(k){return this[k]??null},appendChild(){},removeChild(){},remove(){},insertAdjacentHTML(){},addEventListener(type,fn){listeners.push({target:String(name),type:String(type),fn})},removeEventListener(type,fn){for(let i=listeners.length-1;i>=0;i--)if(listeners[i].target===String(name)&&listeners[i].type===String(type)&&listeners[i].fn===fn)listeners.splice(i,1)},dispatchEvent(ev){eventDispatches.push({target:String(name),type:String(ev?.type),detail:clean(ev?.detail)});for(const l of [...listeners])if(l.target===String(name)&&l.type===String(ev?.type))l.fn.call(this,ev);return true},closest(){return null},querySelector(sel){return getElement(`${name} ${sel}`)},querySelectorAll(){return []},getBoundingClientRect(){const width=this.offsetWidth,height=this.offsetHeight;return {left:0,top:0,width,height,right:width,bottom:height}},toDataURL(){return 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYGD4DwABBAEAHnOcQAAAAABJRU5ErkJggg=='},toBlob(cb){cb?.(new Blob([Buffer.from('89504e470d0a1a0a','hex')],{type:'image/png'}))},getContext(type){const c=/webgl/i.test(String(type))?webglContext():canvas2dContext();c.canvas=this;return c}});
const getElement=name=>{if(!elementCache.has(name))elementCache.set(name,makeElement(name));return elementCache.get(name)};
const document={
  head:{appendChild(){},removeChild(){}},body:{appendChild(){},removeChild(){},addEventListener(type,fn){listeners.push({target:'body',type:String(type),fn})},removeEventListener(type,fn){for(let i=listeners.length-1;i>=0;i--)if(listeners[i].target==='body'&&listeners[i].type===String(type)&&listeners[i].fn===fn)listeners.splice(i,1)},dispatchEvent(ev){eventDispatches.push({target:'body',type:String(ev?.type),detail:clean(ev?.detail)});for(const l of [...listeners])if(l.target==='body'&&l.type===String(ev?.type))l.fn.call(this,ev);return true},insertAdjacentHTML(){}},documentElement:{style:{setProperty(){}}},
  createElement(tag){record('call','document.createElement',[tag]);const el=makeElement(tag);el.tagName=String(tag).toUpperCase();return el},
  createTextNode(text){return {nodeType:3,textContent:String(text),parentNode:null}},
  fonts:{status:'loaded',ready:Promise.resolve(),check(){return true},forEach(){}},visibilityState:'visible',hidden:false,hasFocus(){return true},
  createEvent(){return {type:'',detail:undefined,bubbles:false,cancelable:false,initCustomEvent(type,bubbles,cancelable,detail){this.type=String(type);this.bubbles=!!bubbles;this.cancelable=!!cancelable;this.detail=detail},initEvent(type,bubbles,cancelable){this.type=String(type);this.bubbles=!!bubbles;this.cancelable=!!cancelable}}},
  querySelector(sel){if(sel==='head'||sel==='html head')return this.head;if(sel==='body')return this.body;if(sel==='html')return this.documentElement;return getElement(sel)},querySelectorAll(){return []},getElementsByTagName(tag){if(tag==='head')return [this.head];if(tag==='body')return [this.body];if(tag==='html')return [this.documentElement];return []},getElementById(id){return getElement(`#${id}`)},addEventListener(type,fn){listeners.push({target:'document',type:String(type),fn})},removeEventListener(type,fn){for(let i=listeners.length-1;i>=0;i--)if(listeners[i].target==='document'&&listeners[i].type===String(type)&&listeners[i].fn===fn)listeners.splice(i,1)},dispatchEvent(ev){eventDispatches.push({target:'document',type:String(ev?.type),detail:clean(ev?.detail)});for(const l of [...listeners])if(l.target==='document'&&l.type===String(ev?.type))l.fn.call(this,ev);return true},cookie:''
};
class NodeList extends Array{}
const CryptoJS=require('crypto-js');
const cryptoCalls=[];for(const [group,name] of [['AES','encrypt'],['AES','decrypt']]){const original=CryptoJS[group]?.[name];if(typeof original==='function')CryptoJS[group][name]=function(...args){cryptoCalls.push({op:`${group}.${name}`,args:args.map(x=>typeof x==='string'?{type:'string',length:x.length,preview:x.slice(0,80)}:{type:typeof x,string:String(x).slice(0,80)})});return original.apply(this,args)}}
const File=require('buffer').File;
const xhrCalls=[];
class NativeXMLHttpRequest{
  constructor(){this.headers={};this.readyState=0;this.status=0;this.responseType='';this.responseText='';this.response=null;this.timeout=0;this.withCredentials=false;this.upload={addEventListener(){},removeEventListener(){}}}
  open(method,url,async=true){this.method=String(method);this.url=String(url).replace('https://cloudauth-device.aliyuncs.com','https://cloudauth-device-dualstack.ap-southeast-1.aliyuncs.com');this.async=async!==false;this.readyState=1;this.onreadystatechange?.()}
  setRequestHeader(name,value){this.headers[String(name)]=String(value)}
  getAllResponseHeaders(){return this.responseHeaders||''}
  getResponseHeader(name){return this.responseHeaderMap?.[String(name).toLowerCase()]??null}
  addEventListener(type,fn){this[`on${type}`]=fn}
  removeEventListener(type,fn){if(this[`on${type}`]===fn)this[`on${type}`]=null}
  abort(){this.aborted=true}
  send(body=null){
    const bodyStr=String(body||'');const rec={method:this.method,url:this.url,bodyLength:body==null?0:bodyStr.length,action:new URLSearchParams(bodyStr).get('Action')||'',bodyPreview:bodyStr.substring(0,200),stack:String(new Error().stack).split('\n').slice(1,12).join('\n')};if(bodyStr.includes('Action=Log2'))require('fs').writeFileSync(path.join(process.env.PIPELINE_OUT||path.join(__dirname,'output'),'vm_log2_body.txt'),bodyStr);if(bodyStr.includes('Action=Log3'))require('fs').writeFileSync(path.join(process.env.PIPELINE_OUT||path.join(__dirname,'output'),'vm_log3_body.txt'),bodyStr);xhrCalls.push(rec);
    const controller=new AbortController(),timeout=this.timeout?setTimeout(()=>controller.abort(),this.timeout):null;
    requestText(this.url,{method:this.method||'GET',headers:this.headers,body:/^(GET|HEAD)$/i.test(this.method)?undefined:body,timeout:this.timeout||30000})
      .then(async res=>{if(timeout)clearTimeout(timeout);this.status=res.status;this.statusText=res.statusText;this.responseURL=res.url;this.responseHeaderMap=res.headers;this.responseHeaders=Object.entries(res.headers).map(([k,v])=>`${k}: ${v}`).join('\r\n');const text=res.text;rec.actualResponse=text;this.responseText=text;this.response=this.responseType==='json'?JSON.parse(text):text;this.readyState=4;rec.status=res.status;rec.responseLength=text.length;rec.response=text;this.onreadystatechange?.();this.onload?.({type:'load',target:this,currentTarget:this})})
      .catch(err=>{if(timeout)clearTimeout(timeout);this.readyState=4;rec.error=String(err);this.onreadystatechange?.();(err?.name==='AbortError'?this.ontimeout:this.onerror)?.({type:err?.name==='AbortError'?'timeout':'error',target:this,currentTarget:this,error:err})});
  }
}
class Range{};class Window{};class History{};class Navigator{};class Location{};class Screen{};class Option{};class Node{};class Attr extends Node{};class CharacterData extends Node{};class Text extends CharacterData{};class Comment extends CharacterData{};class DocumentType extends Node{};class Document extends Node{};class HTMLCanvasElement extends Node{};class HTMLImageElement extends Node{};class CSSRule{};class CSSStyleRule extends CSSRule{};class CSSImportRule extends CSSRule{};class MediaList{};class StyleSheet{};class CSSStyleSheet extends StyleSheet{};
const ua='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36';
const pdfMime={type:'application/pdf',suffixes:'pdf',description:'Portable Document Format'};
const textPdfMime={type:'text/pdf',suffixes:'pdf',description:'Portable Document Format'};
const pluginNames=['PDF Viewer','Chrome PDF Viewer','Chromium PDF Viewer','Microsoft Edge PDF Viewer','WebKit built-in PDF'];
const plugins=pluginNames.map(name=>Object.assign([pdfMime,textPdfMime],{name,filename:'internal-pdf-viewer',description:'Portable Document Format',item(i){return this[i]||null},namedItem(n){return this.find(x=>x.type===n)||null}}));
plugins.item=i=>plugins[i]||null;plugins.namedItem=n=>plugins.find(x=>x.name===n)||null;plugins.refresh=()=>{};
const mimeTypes=Object.assign([pdfMime,textPdfMime],{item(i){return this[i]||null},namedItem(n){return this.find(x=>x.type===n)||null}});
const navigator={userAgent:ua,appVersion:ua.replace(/^Mozilla\//,''),appCodeName:'Mozilla',appName:'Netscape',vendor:'Google Inc.',product:'Gecko',productSub:'20030107',language:'zh-CN',languages:['zh-CN'],platform:'Win32',hardwareConcurrency:8,deviceMemory:8,maxTouchPoints:0,cookieEnabled:true,doNotTrack:null,webdriver:false,pdfViewerEnabled:true,plugins,mimeTypes,connection:{effectiveType:'4g',rtt:50,downlink:10,saveData:false},permissions:{query:async()=>({state:'prompt',onchange:null})},mediaDevices:{enumerateDevices:async()=>[]},getBattery:async()=>({charging:true,chargingTime:0,dischargingTime:Infinity,level:1}),userAgentData:{brands:[{brand:'Not;A=Brand',version:'99'},{brand:'Chromium',version:'150'},{brand:'Google Chrome',version:'150'}],mobile:false,platform:'Windows',getHighEntropyValues:async keys=>{const all={architecture:'x86',bitness:'64',brands:[{brand:'Not;A=Brand',version:'99'},{brand:'Chromium',version:'150'},{brand:'Google Chrome',version:'150'}],fullVersionList:[{brand:'Not;A=Brand',version:'99.0.0.0'},{brand:'Chromium',version:'150.0.7871.115'},{brand:'Google Chrome',version:'150.0.7871.115'}],mobile:false,model:'',platform:'Windows',platformVersion:'10.0.0',uaFullVersion:'150.0.7871.115',wow64:false};return Object.fromEntries(keys.map(k=>[k,all[k]??'']))}}};
const timeOrigin=Date.now()-800;
const performance={timeOrigin,now:()=>Date.now()-timeOrigin,timing:{navigationStart:timeOrigin,fetchStart:timeOrigin+2,domainLookupStart:timeOrigin+3,domainLookupEnd:timeOrigin+4,connectStart:timeOrigin+4,connectEnd:timeOrigin+12,requestStart:timeOrigin+13,responseStart:timeOrigin+45,responseEnd:timeOrigin+80,domLoading:timeOrigin+90,domInteractive:timeOrigin+210,domContentLoadedEventStart:timeOrigin+220,domContentLoadedEventEnd:timeOrigin+230,domComplete:timeOrigin+520,loadEventStart:timeOrigin+530,loadEventEnd:timeOrigin+540},navigation:{type:0,redirectCount:0},getEntriesByType(type){if(type==='resource')return [{name:'https://g.alicdn.com/captcha-frontend/FeiLin/1.4.2/feilin106.a96470f298f1cede4bade82b02d0d380b41e66d4ede1859371bd4ae01f5258dd.js',entryType:'resource',startTime:124.6,duration:561.536,transferSize:561536,encodedBodySize:561536,decodedBodySize:561536,initiatorType:'script'}];if(type==='navigation')return [{entryType:'navigation',type:'navigate',startTime:0,duration:540,domComplete:520,loadEventEnd:540}];return []},getEntries(){return this.getEntriesByType('resource')},getEntriesByName(name){return this.getEntries().filter(x=>x.name===name)},mark(){},measure(){},clearMarks(){},clearMeasures(){}};
const sandbox={console,JSON:JSONFacade,setTimeout:browserSetTimeout,clearTimeout:browserClearTimeout,setInterval:browserSetInterval,clearInterval:browserClearInterval,requestAnimationFrame:fn=>browserSetTimeout(()=>fn(performance.now()),16),cancelAnimationFrame:browserClearTimeout,addEventListener(type,fn){listeners.push({target:'window',type:String(type),fn})},removeEventListener(type,fn){for(let i=listeners.length-1;i>=0;i--)if(listeners[i].target==='window'&&listeners[i].type===String(type)&&listeners[i].fn===fn)listeners.splice(i,1)},dispatchEvent(ev){eventDispatches.push({target:'window',type:String(ev?.type),detail:clean(ev?.detail)});for(const l of [...listeners])if(l.target==='window'&&l.type===String(ev?.type))l.fn.call(this,ev);return true},scroll(){},scrollTo(){},scrollBy(){},moveTo(){},moveBy(){},resizeTo(){},resizeBy(){},focus(){},blur(){},close(){},open(){return dummy('window.open')},matchMedia:q=>({matches:false,media:String(q),onchange:null,addListener(){},removeListener(){},addEventListener(){},removeEventListener(){},dispatchEvent(){return true}}),print(){},alert(){},confirm:()=>false,prompt:()=>null,URL,URLSearchParams,TextEncoder,TextDecoder,Blob,File,Headers,Request,Response,Range,Window,History,Navigator,Location,Screen,Option,Node,NodeList,Attr,CharacterData,Text,Comment,DocumentType,Document,HTMLCanvasElement,HTMLImageElement,CSSRule,CSSStyleRule,CSSImportRule,MediaList,StyleSheet,CSSStyleSheet,Storage:class Storage{},CryptoJS,__ALIYUN_CRYPT:CryptoJS,AliyunCaptchaConfig:dummy('AliyunCaptchaConfig'),localStorage:dummy('localStorage'),sessionStorage:dummy('sessionStorage'),history:dummy('history'),devicePixelRatio:1,innerWidth:1440,innerHeight:960,outerWidth:1455,outerHeight:975,screenX:0,screenY:0,HTMLElement:class HTMLElement{},Element:class Element{},Event:class Event{},MouseEvent:class MouseEvent{},TouchEvent:class TouchEvent{},getComputedStyle:()=>({fontSize:'16px',lineHeight:'normal'}),atob:s=>Buffer.from(s,'base64').toString('binary'),btoa:s=>{const input=Buffer.from(s,'binary'),out=input.toString('base64');if(sandbox?.__DRAG_ACTIVE__)btoaCalls.push({inputLength:s.length,outputLength:out.length,inputHex:input.subarray(0,64).toString('hex'),inputBase64:input.length<=5000?input.toString('base64'):undefined,outputPreview:out.slice(0,120),output:out,stack:String(new Error().stack).split('\n').slice(1,9).join('\n')});return out},document,navigator,location:{href:'https://chat.qwen.ai/auth?mode=register',origin:'https://chat.qwen.ai',protocol:'https:',host:'chat.qwen.ai',hostname:'chat.qwen.ai',pathname:'/auth'},screen:{width:1440,height:960,availWidth:1440,availHeight:960,colorDepth:24,pixelDepth:24,orientation:{type:'landscape-primary',angle:0}},Image:dummy('Image'),Audio:dummy('Audio'),Worker:dummy('Worker'),XMLHttpRequest:NativeXMLHttpRequest,fetch:dummy('fetch'),crypto:require('crypto').webcrypto,performance,MutationObserver:dummy('MutationObserver')};
if(process.env.PROBE_DEVICE_FIXTURE_FILE){try{
  sandbox.__DEVICE_FIXTURE__=JSON.parse(fs.readFileSync(process.env.PROBE_DEVICE_FIXTURE_FILE,'utf8'));
  Object.defineProperty(sandbox.__DEVICE_FIXTURE__,'GatherCost',{value:Number(process.env.PROBE_GATHER_COST||725),writable:true,enumerable:false,configurable:true});
}catch(e){throw new Error(`invalid device fixture: ${e.message}`)}}
sandbox.__MGET__=(obj,key,loc)=>{
  if(obj==null){record('null-member',String(key),loc);return dummy(`null@${loc}.${String(key)}`)}
  try{const v=obj[key];if(v===undefined)record('missing-member',String(key),loc);return v}catch(e){record('member-error',String(key),`${loc}: ${e.message}`);return dummy(`error@${loc}.${String(key)}`)}
};
sandbox.__MCALL__=(obj,key,loc,...args)=>{
  if(sandbox.__DRAG_ACTIVE__)record('drag-call',String(key),`${loc}: ${args.map(x=>typeof x).join(',')}`);
  if(/token|verify|encrypt|stringify|collect/i.test(String(key))){const who=obj===sandbox.um?'um':obj===sandbox.z_um?'z_um':obj===sandbox.AliyunCaptcha_clientX?'AliyunCaptcha_clientX':obj===sandbox.JSON?'JSON':'other';record('target-call',`${who}.${String(key)}`,`${loc}: ${args.map(x=>typeof x==='string'?`string(${x.length}):${x.slice(0,100)}`:typeof x).join(',')}`)}
  if(obj==null){record('null-call',String(key),loc);return dummy(`nullcall@${loc}.${String(key)}`)}
  let fn;try{fn=obj[key]}catch(e){record('call-get-error',String(key),`${loc}: ${e.message}`);return dummy(`callerror@${loc}.${String(key)}`)}
  if(String(key)==='getResult'){record('getResult-object',obj?.constructor?.name||typeof obj,`${loc}: keys=${Object.keys(obj||{}).join(',')}`);return Promise.resolve(null)}
  if(typeof fn!=='function'){record('nonfunction-call',String(key),`${loc}: ${typeof fn}`);return dummy(`nonfn@${loc}.${String(key)}`)}
  try{return Reflect.apply(fn,obj,args)}catch(e){record('call-error',String(key),`${loc}: ${e.name}: ${e.message}`);throw e}
};
sandbox.__CATCH__=(error,loc)=>record('caught',loc,`${error?.name||'Error'}: ${error?.message||String(error)}`);
sandbox.__PROMISE_REJECT__=error=>record('promise-reject','runtime',`${error?.name||'Error'}: ${error?.message||String(error)}\n${String(error?.stack||'').split('\n').slice(0,5).join('\n')}`);
sandbox.__JSON_CAPTURE__=(value,result)=>{if(sandbox.__DRAG_ACTIVE__)jsonCalls.push({length:result.length,preview:result.slice(0,500),result:result.length<10000?result:undefined,valueType:typeof value})};
sandbox.window=sandbox;sandbox.self=sandbox;sandbox.globalThis=sandbox;
vm.createContext(sandbox,{name:'captcha-probe',codeGeneration:{strings:true,wasm:false}});
vm.runInContext(`(()=>{const p=Promise.prototype.then;Promise.prototype.then=function(ok,bad){return p.call(this,ok,function(e){__PROMISE_REJECT__(e);if(bad)return bad(e);throw e})}})()`,sandbox);
vm.runInContext(`(()=>{const s=JSON.stringify;JSON.stringify=function(...a){const r=s.apply(this,a);try{__JSON_CAPTURE__(a[0],r)}catch{}return r}})()`,sandbox);
const describe=value=>{
  if(value==null)return {type:String(value)};
  const descriptors={};
  for(const [k,d] of Object.entries(Object.getOwnPropertyDescriptors(value))){descriptors[k]={kind:d.get||d.set?'accessor':typeof d.value,enumerable:d.enumerable,writable:d.writable};}
  const proto=typeof value==='function'?value.prototype:Object.getPrototypeOf(value);
  return {type:typeof value,name:value?.name||value?.constructor?.name||null,keys:Object.keys(value),descriptors,prototypeKeys:proto?Object.getOwnPropertyNames(proto):[]};
};
const clean=(value,depth=0,seen=new WeakSet())=>{
  if(value==null||['string','number','boolean'].includes(typeof value))return value;
  if(typeof value==='function')return `[Function ${value.name||'anonymous'}]`;
  if(depth>4)return '[depth]';if(typeof value!=='object')return String(value);if(seen.has(value))return '[circular]';seen.add(value);
  if(Array.isArray(value))return value.slice(0,250).map(v=>clean(v,depth+1,seen));
  const out={};for(const k of Object.keys(value).slice(0,100)){try{out[k]=clean(value[k],depth+1,seen)}catch(e){out[k]=`[error:${e.message}]`}}return out;
};
(async()=>{
  let error=null;
  const execute=file=>vm.runInContext(fs.readFileSync(file,'utf8'),sandbox,{timeout:8000,filename:path.basename(file)});
  try{
    execute(target);
    for(const file of (process.env.PROBE_POSTLOADS||'').split(path.delimiter).filter(Boolean))execute(file);
  }catch(e){error=`${e.name}: ${e.message}\n${String(e.stack||'').split('\n').slice(-8).join('\n')}`}
  const inspected={};for(const k of ['FEILIN','AliyunCaptcha','AliyunCaptcha_clientX','_aliyun_device_ifr','_aliyun_device_cvs','__PROTO_INTERNALS__','__DYN_INTERNALS__'])if(k in sandbox)inspected[k]=describe(sandbox[k]);
  const calls={};let decoded=null,rawDeviceConfig=null;
  if('EventData' in sandbox)calls.EventData=describe(sandbox.EventData);
  if(!error&&sandbox.__PROTO_INTERNALS__){
    try{const stateValue=sandbox.__PROTO_INTERNALS__.state();calls.state=describe(stateValue);calls.stateValues=clean(stateValue)}catch(e){calls.stateError=`${e.name}: ${e.message}`}
    if(process.env.PROBE_DEVICE_CONFIG_FILE){
      try{const j=JSON.parse(fs.readFileSync(process.env.PROBE_DEVICE_CONFIG_FILE,'utf8'));const value=j.DeviceConfig||j.Result?.DeviceConfig||j.Success&&j.DeviceConfig;rawDeviceConfig=value;decoded=sandbox.__PROTO_INTERNALS__.ge(value);calls.ge=clean(decoded)}catch(e){calls.geError=`${e.name}: ${e.message}`}
    }
  }
  if(!error&&decoded&&sandbox.FEILIN?.initFeiLin&&sandbox.__PROTO_INTERNALS__){
    try{
      const state=sandbox.__PROTO_INTERNALS__.state(),callbacks=[];
      const sdkConfig={SceneId:process.env.PROBE_SCENE_ID||'19x5u7lo',region:'sgp',dualStack:true,verifyType:'2.0',mode:process.env.PROBE_MODE||'popup',language:'cn',prefix:'0a'};
      sandbox.__PROTO_INTERNALS__.Cn?.(sdkConfig);
      const verifyType=sandbox.__PROTO_INTERNALS__.Sr?.(sdkConfig)||'2.0';
      state.Ie._extend({DeviceConfig:undefined,deviceConfig:undefined,DeviceToken:undefined,verifyType});
      state.Ie._extend(sdkConfig);
      const rt=sandbox.__PROTO_INTERNALS__.Rt,d={sceneId:sdkConfig.SceneId,appName:process.env.PROBE_APP_NAME||rt?.appName?.[verifyType],appKey:rt?.appKey?.[verifyType]?.[sdkConfig.region],endpoints:sandbox.__PROTO_INTERNALS__.br?.(sdkConfig.secEndpointType,verifyType,sdkConfig.region),dev:false};
      sandbox.__PROTO_INTERNALS__.tn?.(d.endpoints,d.appName);
      state.Ie._extend(d);
      state.Ie.logs=state.Ie.logs||[];state.Ie.initTime=state.Ie.initTime||Date.now();
      state.Ie._extend({deviceConfig:decoded,timestamp:decoded.timestamp});
      sandbox.um=sandbox.um||{};sandbox.z_um=sandbox.z_um||{};
      const pending=sandbox.FEILIN.initFeiLin(state.Ie,(...args)=>callbacks.push(clean(args)));
      const waitMs=Number(process.env.PROBE_FEILIN_WAIT_MS||8000);
      const value=await Promise.race([Promise.resolve(pending),new Promise((_,reject)=>setTimeout(()=>reject(new Error('FeiLin timeout')),waitMs))]);
      calls.feilin={value:clean(value),callbacks};
    }catch(e){calls.feilinError=`${e.name}: ${e.message}`}
  }
  if(process.env.PROBE_INIT_RESPONSE_FILE){const deadline=Date.now()+12000;while(Date.now()<deadline){const log2=xhrCalls.find(x=>x.action==='Log2');if(log2&&(log2.status!==undefined||log2.error))break;await new Promise(r=>setTimeout(r,100))}}
  const originalZGetToken=typeof sandbox.z_um?.getToken==='function'?sandbox.z_um.getToken.bind(sandbox.z_um):null;
  if(typeof sandbox.z_um?.getToken==='function')calls.zGetTokenSource=String(sandbox.z_um.getToken);
  if(sandbox.__FEILIN_INTERNALS__){try{const fi=sandbox.__FEILIN_INTERNALS__,factoryKey=fi.sr(61,118),methodKey=fi.sr(37,141),owner=fi.se[factoryKey](sandbox.z_um,8);calls.feilinGetTokenInternals={factoryKey,methodKey,owner:describe(owner),methodSource:String(owner?.[methodKey])}}catch(e){calls.feilinGetTokenInternalsError=`${e.name}: ${e.message}`}}
  for(const [label,obj] of [['um',sandbox.um],['z_um',sandbox.z_um]]){
    if(!obj)continue;calls[label]=describe(obj);
    if(!process.env.PROBE_INIT_RESPONSE_FILE&&typeof obj.getToken==='function'){try{calls[`${label}Token`]=clean(await Promise.resolve(obj.getToken()))}catch(e){calls[`${label}TokenError`]=`${e.name}: ${e.message}`}}
  }
  if(sandbox.__DYN_INTERNALS__?.nf?.getResult){sandbox.__DYN_INTERNALS__.nf.getResult=()=>Promise.resolve(null)}
  if(!error&&process.env.PROBE_INIT_RESPONSE_FILE&&typeof sandbox.AliyunCaptcha==='function'){
    const response=JSON.parse(fs.readFileSync(process.env.PROBE_INIT_RESPONSE_FILE,'utf8'));
    if(originalZGetToken&&sandbox.z_um)sandbox.z_um.getToken=originalZGetToken;
    const verifyCallbacks=[];
    const config={...response,SceneId:process.env.PROBE_SCENE_ID||'19x5u7lo',CaptchaType:response.CaptchaType||'PUZZLE',mode:process.env.PROBE_CAPTCHA_MODE||'popup',language:'zh-cn',verifyType:'2.0',rem:1,
      captchaVerifyCallback(...args){verifyCallbacks.push(clean(args));return Promise.resolve({Success:false,Code:'PROBE'})},onBizResultCallback(){},success(){},fail(){},fallbackCb(){},log(){},_extend(value){Object.assign(this,value)}};
    config.DeviceToken=calls.umToken||calls.z_umToken||'';
    const proto=sandbox.AliyunCaptcha.prototype;proto.config=config;proto.deviceConfig=decoded||{};
    const instance=Object.create(proto);
    try{sandbox.AliyunCaptcha.call(instance);calls.captchaConstructed=true}catch(e){calls.captchaConstructError=`${e.name}: ${e.message}`}
    if(sandbox.EventData&&typeof sandbox.EventData==='object')sandbox.EventData=new Proxy(sandbox.EventData,{get(o,k){const v=o[k];eventDataOps.push({op:'get',key:String(k),type:typeof v});return v},set(o,k,v){eventDataOps.push({op:'set',key:String(k),type:typeof v,value:typeof v==='string'?v.slice(0,300):clean(v)});o[k]=v;return true}});
    try{
      const slider=getElement('#aliyunCaptcha-sliding-slider'),event=(type,x,y)=>({type,isTrusted:true,clientX:x,clientY:y,pageX:x,pageY:y,screenX:x,screenY:y,offsetX:x,offsetY:y,button:0,buttons:type==='mouseup'?0:1,target:slider,currentTarget:slider,timeStamp:Date.now(),preventDefault(){},stopPropagation(){}});
      const startX=20,sourceOffset=Number(process.env.PROBE_SOURCE_OFFSET||141),displayTarget=sourceOffset*300/296;
      // SDK 3.28 maps slider displacement s to puzzle x as x=s*(3*s+65)/845.
      // Invert that curve after scaling the 296px source image to the 300px widget.
      const dragDistance=(-65+Math.sqrt(65**2+12*845*displayTarget))/6,endX=Math.round(startX+dragDistance);calls.dragGeometry={sourceOffset,displayTarget,dragDistance,endX};
      for(const el of elementCache.values())if(typeof el.onload==='function')el.onload({type:'load',target:el,currentTarget:el,isTrusted:true});
      const down=listeners.find(x=>x.target==='#aliyunCaptcha-sliding-slider'&&x.type==='mousedown');
      if(down){
        sandbox.__DRAG_ACTIVE__=true;
        down.fn(event('mousedown',startX,80));
        const assigned=(type)=>[sandbox,document,document.body,slider,...elementCache.values()].flatMap(obj=>Object.entries(obj).filter(([k,v])=>k.toLowerCase()===`on${type}`&&typeof v==='function').map(([,fn])=>fn));
        // === Human-like trajectory generator v4 ===
        const fireMove=(x,y)=>{for(const l of [...listeners].filter(v=>v.type==='mousemove'))l.fn(event('mousemove',x,y));for(const fn of assigned('mousemove'))fn(event('mousemove',x,y))};
        const dragLen=endX-startX;
        const numPts=60+Math.floor(Math.random()*20);
        const baseY=80;
        const overshootPx=4+Math.floor(Math.random()*8);
        const overshootT=0.60+Math.random()*0.15;
        const xPosArr=[];
        for(let i=0;i<=numPts;i++){
          const t=i/numPts;
          let px;
          if(t<=overshootT){
            const u=t/overshootT;
            px=startX+(dragLen+overshootPx)*(1-Math.pow(1-u,2.8));
          } else {
            const u=(t-overshootT)/(1-overshootT);
            const peak=startX+dragLen+overshootPx;
            px=peak+(endX-peak)*(1-Math.pow(1-u,3.5));
          }
          if(t>0.75 && Math.random()<0.06) px-=1+Math.random()*3;
          xPosArr.push(px);
        }
        xPosArr[xPosArr.length-1]=endX;
        let yWalk=0;
        const yArr=[];
        for(let i=0;i<=numPts;i++){
          yWalk+=(Math.random()-0.5)*3.0;
          yWalk*=0.92;
          yWalk=Math.max(-18,Math.min(18,yWalk));
          yArr.push(baseY+Math.round(yWalk));
        }
        const pts=[];
        for(let i=0;i<=numPts;i++){
          const t=i/numPts;
          let delay;
          if(i===0) delay=50+Math.floor(Math.random()*40);
          else if(t<0.12) delay=30+Math.floor(Math.random()*25);
          else if(t>0.92) delay=25+Math.floor(Math.random()*35);
          else if(t>overshootT && t<overshootT+0.12) delay=20+Math.floor(Math.random()*30);
          else delay=8+Math.floor(Math.random()*18);
          if(Math.random()<0.035 && t>0.08 && t<0.88) delay+=60+Math.floor(Math.random()*80);
          pts.push({x:Math.round(xPosArr[i]),y:yArr[i],delay});
        }
        // Add micro-jitter to last 5-8 points (hand tremor while holding)
        const jitterStart=pts.length-5-Math.floor(Math.random()*4);
        for(let i=jitterStart;i<pts.length;i++){
          pts[i].x=Math.round(pts[i].x+(Math.random()-0.5)*1.5);
          pts[i].y=Math.round(pts[i].y+(Math.random()-0.5)*1.5);
        }
        pts[pts.length-1].x=endX;
        for(const pt of pts){fireMove(pt.x,pt.y);await new Promise(r=>setTimeout(r,pt.delay));}
        await new Promise(r=>setTimeout(r,40+Math.floor(Math.random()*60)));
        // === End human-like trajectory v4 ===


        if(sandbox.__DYN_INTERNALS__)calls.preUpDynamic=clean(sandbox.__DYN_INTERNALS__.eJ);
        for(const l of [...listeners].filter(v=>v.type==='mouseup'))l.fn(event('mouseup',endX,80));for(const fn of assigned('mouseup'))fn(event('mouseup',endX,80));await new Promise(r=>setTimeout(r,1000))
      }
    }catch(e){calls.dragError=`${e.name}: ${e.message}`}
    calls.verifyCallbacks=verifyCallbacks;
    if('EventData' in sandbox)calls.EventData=describe(sandbox.EventData);
    calls.captchaInstance=describe(instance);calls.captchaInstanceValues=clean(instance);
    if(instance.captcha){calls.puzzleInstance=describe(instance.captcha);calls.puzzleInstanceValues=clean(instance.captcha)}
    if(instance.captcha){calls.puzzleMethods=Object.fromEntries(Object.getOwnPropertyNames(Object.getPrototypeOf(instance.captcha)).filter(k=>typeof instance.captcha[k]==='function').map(k=>[k,{name:instance.captcha[k].name,length:instance.captcha[k].length,source:String(instance.captcha[k]).slice(0,500)}]))}
    calls.listeners=listeners.map((x,i)=>({i,target:x.target,type:x.type,name:x.fn?.name||'',length:x.fn?.length,source:String(x.fn).slice(0,300)}));
    calls.elements=Object.fromEntries([...elementCache.entries()].map(([k,v])=>[k,{style:clean(v.style),className:v.className,innerHTML:v.innerHTML,src:v.src,offsetWidth:v.offsetWidth,clientWidth:v.clientWidth}]));
    if(sandbox.__DYN_INTERNALS__)calls.dynamicInternals=clean({...sandbox.__DYN_INTERNALS__,state:sandbox.__DYN_INTERNALS__.state?.()});
  }
  if(process.env.PROBE_INIT_RESPONSE_FILE){const deadline=Date.now()+12000;while(xhrCalls.some(x=>x.status===undefined&&!x.error)&&Date.now()<deadline)await new Promise(r=>setTimeout(r,100))}
  if(process.env.PROBE_INIT_RESPONSE_FILE&&process.env.PROBE_REQUIRE_TELEMETRY){
    const businessCode=action=>{
      const item=xhrCalls.find(x=>x.action===action);
      try{return {item,code:JSON.parse(item?.actualResponse||item?.response||'null')?.Code||null}}catch{return {item,code:null}}
    };
    const log2=businessCode('Log2'),log3=businessCode('Log3');
    calls.telemetry={log2:{status:log2.item?.status||null,code:log2.code,bodyLength:log2.item?.bodyLength||0},log3:{status:log3.item?.status||null,code:log3.code,bodyLength:log3.item?.bodyLength||0},uploadLog:!!xhrCalls.find(x=>x.action==='UploadLog')};
    if(calls.feilinError)throw new Error(`FeiLin initialization failed: ${calls.feilinError}`);
    if(!log2.item||log2.code!=='200')throw new Error(`Log2 did not complete successfully: ${JSON.stringify(calls.telemetry.log2)}`);
    if(!log3.item||log3.code!=='200')throw new Error(`Log3 did not complete successfully: ${JSON.stringify(calls.telemetry.log3)}`);
  }
  // Analysis-only oracle: FeiLin's rk/rM pair is the symmetric codec used for
  // Log2.Data.  Keeping this behind environment variables lets us compare a
  // captured browser plaintext with the VM plaintext without changing runtime
  // behavior in normal replays.
  if(process.env.PROBE_DECRYPT_DATA_FILE&&sandbox.__FEILIN_INTERNALS__?.rM){
    try{
      const encrypted=fs.readFileSync(process.env.PROBE_DECRYPT_DATA_FILE,'utf8').trim();
      calls.decryptedData=sandbox.__FEILIN_INTERNALS__.rM(process.env.PROBE_DECRYPT_KEY||'a549a55c60a39aa0',encrypted);
    }catch(e){calls.decryptedDataError=`${e.name}: ${e.message}`}
  }
  if(process.env.PROBE_IC_PLAINTEXT_FILE&&sandbox.__FEILIN_INTERNALS__?.ic){
    try{const raw=fs.readFileSync(process.env.PROBE_IC_PLAINTEXT_FILE,'utf8');let value=raw;if(process.env.PROBE_IC_JSON==='1')value=JSON.parse(raw);calls.icFixture=sandbox.__FEILIN_INTERNALS__.ic(value,501)}
    catch(e){calls.icFixtureError=`${e.name}: ${e.message}`}
  }
  if(process.env.PROBE_IS_VARIANTS==='1'&&sandbox.__FEILIN_INTERNALS__?.is&&sandbox.__DEVICE_FIXTURE__){
    calls.isVariants=[];for(const args of [[null,null,true],[725,null,true],[null,725,true],[null,null,725],[725,725,true],[725,null,false]]){
      try{calls.isVariants.push({args,value:sandbox.__FEILIN_INTERNALS__.is(sandbox.__DEVICE_FIXTURE__,...args)})}catch(e){calls.isVariants.push({args,error:`${e.name}: ${e.message}`})}
    }
  }
  if(sandbox.__FEILIN_CALLS__)calls.feilinCalls=clean(sandbox.__FEILIN_CALLS__);
  if(sandbox.__IS_DEVICE_SNAPSHOT__)calls.isDeviceSnapshot=clean(sandbox.__IS_DEVICE_SNAPSHOT__);
  if(sandbox.__IC_RETURNS__)calls.icReturns=clean(sandbox.__IC_RETURNS__);
  if(sandbox.__N2_TK_PROBE__)calls.n2TkProbe=clean(sandbox.__N2_TK_PROBE__);
  calls.cryptoCalls=cryptoCalls;calls.btoaCalls=btoaCalls;calls.jsonCalls=jsonCalls;calls.eventDispatches=eventDispatches;calls.eventDataOps=eventDataOps;calls.xhrCalls=xhrCalls;calls.pendingTimerCount=timerHandles.size;const out={target,error,globals:Object.keys(sandbox).sort(),inspected,calls,trace:log};
  fs.mkdirSync(path.join(__dirname,'output'),{recursive:true});
  fs.writeFileSync(path.join(process.env.PIPELINE_OUT||path.join(__dirname,'output'),'probe.json'),JSON.stringify(out,null,2));
  console.log(JSON.stringify({error,traceCount:log.length,calls:Object.keys(calls),newGlobals:Object.keys(sandbox).filter(k=>!['console','setTimeout','clearTimeout','setInterval','clearInterval','URL','URLSearchParams','TextEncoder','TextDecoder','atob','btoa','document','navigator','location','screen','Image','XMLHttpRequest','fetch','crypto','performance','MutationObserver','window','self','globalThis'].includes(k))},null,2));
  setImmediate(()=>process.exit(0));
})().catch(e=>{console.error(e);process.exitCode=1});
