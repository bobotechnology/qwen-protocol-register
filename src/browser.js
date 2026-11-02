'use strict';
const window = globalThis;
const self = globalThis;
const calls = {jsonCalls: [], xhrCalls: [], btoaCalls: [], eventDispatches: [], promiseRejections: []};
const trace = [];
const listeners = [];
const elementCache = new Map();
let lastDeviceData = null;
let timerSeq = 1;
const timerHandles = new Map();
const nativeStringify = JSON.stringify;
JSON.stringify = function(...args) {
  if (args[0] && args[0].dghfh565) lastDeviceData = args[0];
  if (args[0] && args[0].deviceConfig && !args[0].deviceData && lastDeviceData) args[0].deviceData = lastDeviceData;
  const result = nativeStringify.apply(JSON, args);
  if (result && result.length > 20) calls.jsonCalls.push({source: 'native', result});
  return result;
};
function clean(value, depth = 0, seen = new WeakSet()) {
  if (value == null || ['string', 'number', 'boolean'].includes(typeof value)) return value;
  if (typeof value === 'function') return `[Function ${value.name || 'anonymous'}]`;
  if (depth > 4) return '[depth]';
  if (typeof value !== 'object') return String(value);
  if (seen.has(value)) return '[circular]';
  seen.add(value);
  if (Array.isArray(value)) return value.slice(0, 250).map(item => clean(item, depth + 1, seen));
  const output = {};
  for (const key of Object.keys(value).slice(0, 100)) {
    try { output[key] = clean(value[key], depth + 1, seen); } catch (_) {}
  }
  return output;
}
function describe(value) {
  if (value == null) return {type: String(value)};
  const descriptors = {};
  for (const [key, descriptor] of Object.entries(Object.getOwnPropertyDescriptors(value))) {
    descriptors[key] = {kind: descriptor.get || descriptor.set ? 'accessor' : typeof descriptor.value, enumerable: descriptor.enumerable, writable: descriptor.writable};
  }
  const prototype = typeof value === 'function' ? value.prototype : Object.getPrototypeOf(value);
  return {type: typeof value, name: value.name || (value.constructor && value.constructor.name) || null, keys: Object.keys(value), descriptors, prototypeKeys: prototype ? Object.getOwnPropertyNames(prototype) : []};
}
function record(op, path, value) {
  if (trace.length < 5000) trace.push({op, path, value});
}
function dummy(name = 'dummy') {
  const target = function() {};
  return new Proxy(target, {
    get(object, property) {
      if (property === Symbol.toPrimitive) return () => '';
      if (property === 'then') return undefined;
      if (property === 'prototype') return {};
      record('get', `${name}.${String(property)}`);
      return dummy(`${name}.${String(property)}`);
    },
    set(object, property, value) { object[property] = value; return true; },
    apply() { record('call', name); return dummy(`${name}()`); },
    construct() { return dummy(`new ${name}`); }
  });
}
function makeStyle(name) {
  return {cssText: '', display: '', visibility: '', setProperty(key, value) { this[key] = String(value); }, getPropertyValue(key) { return this[key] || ''; }};
}
function canvas2dContext() {
  return {
    canvas: null, fillStyle: '#000000', strokeStyle: '#000000', font: '10px sans-serif', textBaseline: 'alphabetic', globalCompositeOperation: 'source-over',
    beginPath() {}, closePath() {}, moveTo() {}, lineTo() {}, arc() {}, rect() {}, clip() {}, stroke() {}, fill() {}, clearRect() {}, fillRect() {}, strokeRect() {},
    fillText() {}, strokeText() {}, save() {}, restore() {}, translate() {}, rotate() {}, scale() {}, transform() {}, setTransform() {}, drawImage() {},
    measureText(text) { return {width: String(text).length * 7.21875, actualBoundingBoxAscent: 9, actualBoundingBoxDescent: 2}; },
    getImageData(x, y, w, h) { const width = Number(w || 1), height = Number(h || 1), data = new Uint8ClampedArray(Math.max(0, width * height * 4)); for (let i = 0; i < data.length; i += 4) { data[i] = 17; data[i + 1] = 34; data[i + 2] = 51; data[i + 3] = 255; } return {data, width, height}; },
    putImageData() {}, createImageData(w, h) { return {data: new Uint8ClampedArray(Number(w) * Number(h) * 4), width: Number(w), height: Number(h)}; },
    createLinearGradient() { return {addColorStop() {}}; }, createRadialGradient() { return {addColorStop() {}}; }, createPattern() { return {setTransform() {}}; },
    isPointInPath() { return false; }, isPointInStroke() { return false; }, setLineDash() {}, getLineDash() { return []; }
  };
}
function webglContext() {
  const extension = {UNMASKED_VENDOR_WEBGL: 37445, UNMASKED_RENDERER_WEBGL: 37446};
  return {
    VENDOR: 7936, RENDERER: 7937, VERSION: 7938, SHADING_LANGUAGE_VERSION: 35724, MAX_TEXTURE_SIZE: 3379, MAX_VIEWPORT_DIMS: 3386,
    getExtension(name) { return String(name) === 'WEBGL_debug_renderer_info' ? extension : null; },
    getSupportedExtensions() { return ['ANGLE_instanced_arrays', 'EXT_blend_minmax', 'EXT_color_buffer_half_float', 'EXT_disjoint_timer_query', 'EXT_float_blend', 'EXT_frag_depth', 'EXT_shader_texture_lod', 'EXT_texture_compression_bptc', 'EXT_texture_filter_anisotropic', 'OES_element_index_uint', 'OES_fbo_render_mipmap', 'OES_standard_derivatives', 'OES_texture_float', 'OES_texture_float_linear', 'OES_texture_half_float', 'OES_texture_half_float_linear', 'OES_vertex_array_object', 'WEBGL_color_buffer_float', 'WEBGL_compressed_texture_s3tc', 'WEBGL_debug_renderer_info', 'WEBGL_debug_shaders', 'WEBGL_depth_texture', 'WEBGL_draw_buffers', 'WEBGL_lose_context', 'WEBGL_multi_draw']; },
    getParameter(parameter) { if (parameter === 37445) return 'Google Inc. (Intel)'; if (parameter === 37446) return 'ANGLE (Intel, Intel(R) Graphics (0x00007D45) Direct3D11 vs_5_0 ps_5_0, D3D11)'; if (parameter === 7936) return 'WebKit'; if (parameter === 7937) return 'WebKit WebGL'; if (parameter === 7938) return 'WebGL 1.0 (OpenGL ES 2.0 Chromium)'; if (parameter === 35724) return 'WebGL GLSL ES 1.0 (OpenGL ES GLSL ES 1.0 Chromium)'; if (parameter === 3379) return 16384; if (parameter === 3386) return new Int32Array([32767, 32767]); return 0; },
    createBuffer() { return {}; }, bindBuffer() {}, bufferData() {}, createProgram() { return {}; }, createShader() { return {}; }, shaderSource() {}, compileShader() {}, attachShader() {}, linkProgram() {}, useProgram() {}, getAttribLocation() { return 0; }, getUniformLocation() { return {}; }, enableVertexAttribArray() {}, vertexAttribPointer() {}, uniform2f() {}, drawArrays() {}, viewport() {}, clearColor() {}, clear() {}, readPixels(x, y, w, h, format, type, output) { if (output && output.fill) output.fill(127); }
  };
}
function addListener(target, type, fn) { if (typeof fn === 'function') listeners.push({target, type: String(type), fn}); }
function removeListener(target, type, fn) { for (let i = listeners.length - 1; i >= 0; i--) if (listeners[i].target === target && listeners[i].type === String(type) && listeners[i].fn === fn) listeners.splice(i, 1); }
function dispatch(target, event) { calls.eventDispatches.push({target, type: String(event && event.type), detail: clean(event && event.detail)}); for (const listener of [...listeners]) if (listener.target === target && listener.type === String(event && event.type)) listener.fn(event); return true; }
function makeElement(name = 'element') {
  const text = String(name);
  return {
    tagName: 'DIV', id: text.replace(/^#/, '').replace(/[^\w-].*$/, ''), style: makeStyle(text), className: '', innerHTML: '', onclick: null, offsetLeft: 0, offsetTop: 0,
    offsetWidth: text.includes('slider') ? 40 : text.includes('puzzle') ? 52 : 300, clientWidth: text.includes('slider') ? 40 : text.includes('puzzle') ? 52 : 300, offsetHeight: 40, clientHeight: 40,
    naturalWidth: text.includes('puzzle') ? 52 : 296, naturalHeight: 180, complete: true,
    setAttribute(key, value) { this[key] = value; }, getAttribute(key) { return this[key] == null ? null : this[key]; }, appendChild() {}, removeChild() {}, remove() {}, insertAdjacentHTML() {},
    addEventListener(type, fn) { addListener(text, type, fn); }, removeEventListener(type, fn) { removeListener(text, type, fn); }, dispatchEvent(event) { return dispatch(text, event); },
    closest() { return null; }, querySelector(selector) { return getElement(`${text} ${selector}`); }, querySelectorAll() { return []; },
    getBoundingClientRect() { const width = this.offsetWidth, height = this.offsetHeight; return {left: 0, top: 0, width, height, right: width, bottom: height}; },
    toDataURL() { return 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYGD4DwABBAEAHnOcQAAAAABJRU5ErkJggg=='; },
    toBlob(callback) { if (callback) callback(new Blob([new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10])], {type: 'image/png'})); },
    getContext(type) { const context = /webgl/i.test(String(type)) ? webglContext() : canvas2dContext(); context.canvas = this; return context; }
  };
}
function getElement(name) { if (!elementCache.has(name)) elementCache.set(name, makeElement(name)); return elementCache.get(name); }
const document = {
  head: {appendChild() {}, removeChild() {}},
  body: {appendChild() {}, removeChild() {}, addEventListener(type, fn) { addListener('body', type, fn); }, removeEventListener(type, fn) { removeListener('body', type, fn); }, dispatchEvent(event) { return dispatch('body', event); }, insertAdjacentHTML() {}},
  documentElement: {style: {setProperty() {}}},
  createElement(tag) { const element = makeElement(tag); element.tagName = String(tag).toUpperCase(); return element; },
  createTextNode(text) { return {nodeType: 3, textContent: String(text), parentNode: null}; },
  fonts: {status: 'loaded', ready: Promise.resolve(), check() { return true; }, forEach() {}}, visibilityState: 'visible', hidden: false, hasFocus() { return true; },
  createEvent() { return {type: '', detail: undefined, bubbles: false, cancelable: false, initCustomEvent(type, bubbles, cancelable, detail) { this.type = String(type); this.bubbles = !!bubbles; this.cancelable = !!cancelable; this.detail = detail; }, initEvent(type, bubbles, cancelable) { this.type = String(type); this.bubbles = !!bubbles; this.cancelable = !!cancelable; }}; },
  querySelector(selector) { if (selector === 'head' || selector === 'html head') return this.head; if (selector === 'body') return this.body; if (selector === 'html') return this.documentElement; return getElement(selector); },
  querySelectorAll() { return []; }, getElementsByTagName(tag) { if (tag === 'head') return [this.head]; if (tag === 'body') return [this.body]; if (tag === 'html') return [this.documentElement]; return []; },
  getElementById(id) { return getElement(`#${id}`); }, addEventListener(type, fn) { addListener('document', type, fn); }, removeEventListener(type, fn) { removeListener('document', type, fn); }, dispatchEvent(event) { return dispatch('document', event); }, cookie: ''
};
class NodeList extends Array {}
class Node {}
class Event { constructor(type, options = {}) { this.type = type; Object.assign(this, options); } preventDefault() {} stopPropagation() {} }
class CustomEvent extends Event {}
class MouseEvent extends Event {}
class TouchEvent extends Event {}
class HTMLElement extends Node {}
class Element extends Node {}
class Range {}
class Window {}
class History {}
class Navigator {}
class Location {}
class Screen {}
class Option {}
class Attr extends Node {}
class CharacterData extends Node {}
class Text extends CharacterData {}
class Comment extends CharacterData {}
class DocumentType extends Node {}
class Document extends Node {}
class HTMLCanvasElement extends Node {}
class HTMLImageElement extends Node {}
class CSSRule {}
class CSSStyleRule extends CSSRule {}
class CSSImportRule extends CSSRule {}
class MediaList {}
class StyleSheet {}
class CSSStyleSheet extends StyleSheet {}
class Storage {
  constructor() { this.values = new Map(); }
  get length() { return this.values.size; }
  key(index) { return [...this.values.keys()][index] || null; }
  getItem(key) { const name = String(key); return this.values.has(name) ? this.values.get(name) : null; }
  setItem(key, value) { this.values.set(String(key), String(value)); }
  removeItem(key) { this.values.delete(String(key)); }
  clear() { this.values.clear(); }
}
class Blob { constructor(parts = [], options = {}) { this.parts = parts; this.type = options.type || ''; } }
class File extends Blob { constructor(parts, name, options = {}) { super(parts, options); this.name = name; } }
class TextEncoder { encode(text) { const output = []; for (const character of String(text)) { let point = character.codePointAt(0); if (point < 128) output.push(point); else if (point < 2048) output.push(192 | point >> 6, 128 | point & 63); else if (point < 65536) output.push(224 | point >> 12, 128 | point >> 6 & 63, 128 | point & 63); else output.push(240 | point >> 18, 128 | point >> 12 & 63, 128 | point >> 6 & 63, 128 | point & 63); } return new Uint8Array(output); } }
class TextDecoder { decode(input) { const bytes = input && input.buffer ? new Uint8Array(input.buffer, input.byteOffset || 0, input.byteLength == null ? input.buffer.byteLength : input.byteLength) : new Uint8Array(input || new ArrayBuffer(0)); let output = ''; for (let i = 0; i < bytes.length;) { const first = bytes[i++]; if (first < 128) output += String.fromCharCode(first); else if (first < 224) { output += String.fromCharCode((first & 31) << 6 | bytes[i++] & 63); } else if (first < 240) { output += String.fromCharCode((first & 15) << 12 | (bytes[i++] & 63) << 6 | bytes[i++] & 63); } else { let point = (first & 7) << 18 | (bytes[i++] & 63) << 12 | (bytes[i++] & 63) << 6 | bytes[i++] & 63; point -= 65536; output += String.fromCharCode(55296 + (point >> 10), 56320 + (point & 1023)); } } return output; } }
function formEncode(value) { return encodeURIComponent(String(value)).replace(/%20/g, '+').replace(/~/g, '%7E'); }
function formDecode(value) { try { return decodeURIComponent(String(value).replace(/\+/g, '%20')); } catch (_) { return String(value); } }
class URLSearchParams {
  constructor(init = '') { this.items = []; if (typeof init === 'string') { for (const pair of init.replace(/^\?/, '').split('&')) { if (!pair) continue; const index = pair.indexOf('='); this.append(formDecode(index < 0 ? pair : pair.slice(0, index)), formDecode(index < 0 ? '' : pair.slice(index + 1))); } } else if (init && typeof init === 'object') { for (const [key, value] of Object.entries(init)) this.append(key, value); } }
  append(key, value) { this.items.push([String(key), String(value)]); }
  set(key, value) { this.delete(key); this.append(key, value); }
  get(key) { const item = this.items.find(item => item[0] === String(key)); return item ? item[1] : null; }
  has(key) { return this.items.some(item => item[0] === String(key)); }
  delete(key) { this.items = this.items.filter(item => item[0] !== String(key)); }
  entries() { return this.items[Symbol.iterator](); }
  [Symbol.iterator]() { return this.entries(); }
  forEach(callback) { for (const [key, value] of this.items) callback(value, key, this); }
  toString() { return this.items.map(([key, value]) => `${formEncode(key)}=${formEncode(value)}`).join('&'); }
}
class URL {
  constructor(input, base) { const parsed = JSON.parse(__url_parse(String(input), base ? String(base) : '')); this.href = parsed.href; this.protocol = parsed.protocol; this.host = parsed.host; this.hostname = parsed.hostname; this.pathname = parsed.pathname; this.origin = parsed.origin; this.hash = parsed.hash; this.searchParams = new URLSearchParams(parsed.search); this.sync(); }
  sync() { const search = this.searchParams.toString(); this.search = search ? `?${search}` : ''; this.href = `${this.origin}${this.pathname}${this.search}${this.hash}`; }
  toString() { this.sync(); return this.href; }
}
class Headers { constructor(init = {}) { this.values = {}; for (const [key, value] of Object.entries(init)) this.values[key.toLowerCase()] = String(value); } get(key) { return this.values[String(key).toLowerCase()] || null; } set(key, value) { this.values[String(key).toLowerCase()] = String(value); } }
class Request { constructor(url, options = {}) { this.url = String(url); Object.assign(this, options); } }
class Response { constructor(body, options = {}) { this.body = body; this.status = options.status || 200; } async text() { return String(this.body || ''); } async json() { return JSON.parse(await this.text()); } }
function bytesToBinary(bytes) { let output = ''; for (let i = 0; i < bytes.length; i += 8192) output += String.fromCharCode(...bytes.subarray(i, Math.min(i + 8192, bytes.length))); return output; }
function atob(input) { const clean = String(input).replace(/\s/g, ''); const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'; const bytes = []; let accumulator = 0, bits = 0; for (const character of clean) { if (character === '=') break; const value = alphabet.indexOf(character); if (value < 0) throw new Error('Invalid base64'); accumulator = accumulator << 6 | value; bits += 6; if (bits >= 8) { bits -= 8; bytes.push(accumulator >> bits & 255); } } return bytesToBinary(new Uint8Array(bytes)); }
function btoa(input) { const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'; const bytes = new Uint8Array(Array.from(String(input), character => { const value = character.charCodeAt(0); if (value > 255) throw new Error('Invalid binary string'); return value; })); let output = ''; for (let i = 0; i < bytes.length; i += 3) { const a = bytes[i], b = i + 1 < bytes.length ? bytes[i + 1] : 0, c = i + 2 < bytes.length ? bytes[i + 2] : 0, value = a << 16 | b << 8 | c; output += alphabet[value >> 18 & 63] + alphabet[value >> 12 & 63] + (i + 1 < bytes.length ? alphabet[value >> 6 & 63] : '=') + (i + 2 < bytes.length ? alphabet[value & 63] : '='); } return output; }
const ua = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36';
const pdfMime = {type: 'application/pdf', suffixes: 'pdf', description: 'Portable Document Format'};
const textPdfMime = {type: 'text/pdf', suffixes: 'pdf', description: 'Portable Document Format'};
const plugins = ['PDF Viewer', 'Chrome PDF Viewer', 'Chromium PDF Viewer', 'Microsoft Edge PDF Viewer', 'WebKit built-in PDF'].map(name => Object.assign([pdfMime, textPdfMime], {name, filename: 'internal-pdf-viewer', description: 'Portable Document Format', item(index) { return this[index] || null; }, namedItem(name) { return this.find(item => item.type === name) || null; }}));
plugins.item = index => plugins[index] || null; plugins.namedItem = name => plugins.find(item => item.name === name) || null; plugins.refresh = () => {};
const mimeTypes = Object.assign([pdfMime, textPdfMime], {item(index) { return this[index] || null; }, namedItem(name) { return this.find(item => item.type === name) || null; }});
const navigator = {userAgent: ua, appVersion: ua.replace(/^Mozilla\//, ''), appCodeName: 'Mozilla', appName: 'Netscape', vendor: 'Google Inc.', product: 'Gecko', productSub: '20030107', language: 'zh-CN', languages: ['zh-CN'], platform: 'Win32', hardwareConcurrency: 8, deviceMemory: 8, maxTouchPoints: 0, cookieEnabled: true, doNotTrack: null, webdriver: false, pdfViewerEnabled: true, plugins, mimeTypes, connection: {effectiveType: '4g', rtt: 50, downlink: 10, saveData: false}, permissions: {query: async () => ({state: 'prompt', onchange: null})}, mediaDevices: {enumerateDevices: async () => []}, getBattery: async () => ({charging: true, chargingTime: 0, dischargingTime: Infinity, level: 1}), userAgentData: {brands: [{brand: 'Not;A=Brand', version: '99'}, {brand: 'Chromium', version: '150'}, {brand: 'Google Chrome', version: '150'}], mobile: false, platform: 'Windows', getHighEntropyValues: async keys => { const all = {architecture: 'x86', bitness: '64', brands: [{brand: 'Not;A=Brand', version: '99'}, {brand: 'Chromium', version: '150'}, {brand: 'Google Chrome', version: '150'}], fullVersionList: [{brand: 'Not;A=Brand', version: '99.0.0.0'}, {brand: 'Chromium', version: '150.0.7871.115'}, {brand: 'Google Chrome', version: '150.0.7871.115'}], mobile: false, model: '', platform: 'Windows', platformVersion: '10.0.0', uaFullVersion: '150.0.7871.115', wow64: false}; return Object.fromEntries(keys.map(key => [key, all[key] == null ? '' : all[key]])); }}};
const timeOrigin = Date.now() - 800;
const performance = {timeOrigin, now: () => Date.now() - timeOrigin, timing: {navigationStart: timeOrigin, fetchStart: timeOrigin + 2, domainLookupStart: timeOrigin + 3, domainLookupEnd: timeOrigin + 4, connectStart: timeOrigin + 4, connectEnd: timeOrigin + 12, requestStart: timeOrigin + 13, responseStart: timeOrigin + 45, responseEnd: timeOrigin + 80, domLoading: timeOrigin + 90, domInteractive: timeOrigin + 210, domContentLoadedEventStart: timeOrigin + 220, domContentLoadedEventEnd: timeOrigin + 230, domComplete: timeOrigin + 520, loadEventStart: timeOrigin + 530, loadEventEnd: timeOrigin + 540}, navigation: {type: 0, redirectCount: 0}, getEntriesByType(type) { if (type === 'resource') return [{name: 'https://g.alicdn.com/captcha-frontend/FeiLin/1.4.2/feilin106.a96470f298f1cede4bade82b02d0d380b41e66d4ede1859371bd4ae01f5258dd.js', entryType: 'resource', startTime: 124.6, duration: 561.536, transferSize: 561536, encodedBodySize: 561536, decodedBodySize: 561536, initiatorType: 'script'}]; if (type === 'navigation') return [{entryType: 'navigation', type: 'navigate', startTime: 0, duration: 540, domComplete: 520, loadEventEnd: 540}]; return []; }, getEntries() { return this.getEntriesByType('resource'); }, getEntriesByName(name) { return this.getEntries().filter(item => item.name === name); }, mark() {}, measure() {}, clearMarks() {}, clearMeasures() {}};
function setTimeoutNative(fn, ms, ...args) { const id = timerSeq++; const state = {cancelled: false}; timerHandles.set(id, state); __native_timeout(Math.max(0, Number(ms) || 0)).then(() => { if (!state.cancelled) { timerHandles.delete(id); fn(...args); } }); return id; }
function clearTimeoutNative(id) { const state = timerHandles.get(id); if (state) state.cancelled = true; timerHandles.delete(id); }
function setIntervalNative(fn, ms, ...args) { const id = timerSeq++; const state = {cancelled: false}; timerHandles.set(id, state); async function tick() { while (!state.cancelled) { await __native_timeout(Math.max(0, Number(ms) || 0)); if (!state.cancelled) fn(...args); } } tick(); return id; }
function requestAnimationFrame(fn) { return setTimeoutNative(() => fn(performance.now()), 16); }
function bodyToString(body) { if (body == null) return ''; if (typeof body === 'string') return body; if (body instanceof Uint8Array) return new TextDecoder().decode(body); if (body instanceof ArrayBuffer) return new TextDecoder().decode(body); return String(body); }
class XMLHttpRequest {
  constructor() { this.headers = {}; this.readyState = 0; this.status = 0; this.responseType = ''; this.responseText = ''; this.response = null; this.timeout = 0; this.withCredentials = false; this.upload = {addEventListener() {}, removeEventListener() {}}; }
  open(method, url, async = true) { this.method = String(method); this.url = String(url).replace('https://cloudauth-device.aliyuncs.com', 'https://cloudauth-device-dualstack.ap-southeast-1.aliyuncs.com'); this.async = async !== false; this.readyState = 1; if (this.onreadystatechange) this.onreadystatechange(); }
  setRequestHeader(name, value) { this.headers[String(name)] = String(value); }
  getAllResponseHeaders() { return this.responseHeaders || ''; }
  getResponseHeader(name) { return this.responseHeaderMap ? this.responseHeaderMap[String(name).toLowerCase()] || null : null; }
  addEventListener(type, fn) { this[`on${type}`] = fn; }
  removeEventListener(type, fn) { if (this[`on${type}`] === fn) this[`on${type}`] = null; }
  abort() { this.aborted = true; }
  send(body = null) {
    const text = bodyToString(body);
    const record = {method: this.method, url: this.url, bodyLength: text.length, action: new URLSearchParams(text).get('Action') || ''};
    if (record.action === 'Log2') __artifact_write('vm_log2_body.txt', text);
    if (record.action === 'Log3') __artifact_write('vm_log3_body.txt', text);
    calls.xhrCalls.push(record);
    __xhr_request(this.method || 'GET', this.url, nativeStringify(this.headers), /^(GET|HEAD)$/i.test(this.method) ? '' : text, this.timeout || 30000).then(raw => {
      const response = JSON.parse(raw); this.status = response.status; this.statusText = response.statusText; this.responseURL = response.url; this.responseHeaderMap = response.headers; this.responseHeaders = Object.entries(response.headers).map(([key, value]) => `${key}: ${value}`).join('\r\n'); this.responseText = response.text; this.response = this.responseType === 'json' ? JSON.parse(response.text) : response.text; this.readyState = 4; record.status = response.status; record.responseLength = response.text.length; record.response = response.text; if (this.onreadystatechange) this.onreadystatechange(); if (this.onload) this.onload({type: 'load', target: this, currentTarget: this});
    }, error => { this.readyState = 4; record.error = String(error); if (this.onreadystatechange) this.onreadystatechange(); if (this.onerror) this.onerror({type: 'error', target: this, currentTarget: this, error}); });
  }
}
const crypto = {getRandomValues(array) { const random = __random_bytes(array.length); for (let i = 0; i < array.length; i++) array[i] = random[i]; return array; }, randomUUID() { return __uuid(); }, subtle: {
  async digest(algorithm, data) { return Uint8Array.from(__digest(String(algorithm), Array.from(new Uint8Array(data.buffer || data)))).buffer; },
  async importKey(format, keyData, algorithm, extractable, usages) { return {format, keyData: new Uint8Array(keyData.buffer || keyData), algorithm, extractable, usages}; },
  async encrypt(algorithm, key, data) { return Uint8Array.from(__aes_encrypt(key.keyData ? Array.from(key.keyData) : [], Array.from(new Uint8Array(algorithm.iv.buffer || algorithm.iv)), Array.from(new Uint8Array(data.buffer || data)))).buffer; },
  async decrypt(algorithm, key, data) { return Uint8Array.from(__aes_decrypt(key.keyData ? Array.from(key.keyData) : [], Array.from(new Uint8Array(algorithm.iv.buffer || algorithm.iv)), Array.from(new Uint8Array(data.buffer || data)))).buffer; }
}};
globalThis.window = globalThis;
globalThis.self = globalThis;
Object.assign(globalThis, {document, navigator, performance, location: new URL('https://chat.qwen.ai/auth?mode=register'), screen: {width: 1440, height: 960, availWidth: 1440, availHeight: 960, colorDepth: 24, pixelDepth: 24, orientation: {type: 'landscape-primary', angle: 0}}, devicePixelRatio: 1, innerWidth: 1440, innerHeight: 960, outerWidth: 1455, outerHeight: 975, screenX: 0, screenY: 0,
  setTimeout: setTimeoutNative, clearTimeout: clearTimeoutNative, setInterval: setIntervalNative, clearInterval: clearTimeoutNative, requestAnimationFrame, cancelAnimationFrame: clearTimeoutNative,
  addEventListener(type, fn) { addListener('window', type, fn); }, removeEventListener(type, fn) { removeListener('window', type, fn); }, dispatchEvent(event) { return dispatch('window', event); },
  scroll() {}, scrollTo() {}, scrollBy() {}, moveTo() {}, moveBy() {}, resizeTo() {}, resizeBy() {}, focus() {}, blur() {}, close() {}, open() { return dummy('window.open'); },
  matchMedia(query) { return {matches: false, media: String(query), onchange: null, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return true; }}; }, print() {}, alert() {}, confirm() { return false; }, prompt() { return null; },
  URL, URLSearchParams, TextEncoder, TextDecoder, Blob, File, Headers, Request, Response, Range, Window, History, Navigator, Location, Screen, Option, Node, NodeList, Attr, CharacterData, Text, Comment, DocumentType, Document, HTMLCanvasElement, HTMLImageElement, CSSRule, CSSStyleRule, CSSImportRule, MediaList, StyleSheet, CSSStyleSheet, Storage,
  HTMLElement, Element, Event, CustomEvent, MouseEvent, TouchEvent, getComputedStyle: () => ({fontSize: '16px', lineHeight: 'normal'}), atob, btoa, Image: dummy('Image'), Audio: dummy('Audio'), Worker: dummy('Worker'), XMLHttpRequest, fetch: dummy('fetch'), crypto, MutationObserver: dummy('MutationObserver'),
  AliyunCaptchaConfig: dummy('AliyunCaptchaConfig'), localStorage: dummy('localStorage'), sessionStorage: dummy('sessionStorage'), history: dummy('history'), console: {log() {}, warn() {}, error() {}, info() {}, debug() {}}
});
const nativeThen = Promise.prototype.then;
Promise.prototype.then = function(onFulfilled, onRejected) {
  return nativeThen.call(this, onFulfilled, function(error) {
    if (calls.promiseRejections.length < 100) calls.promiseRejections.push({name: error && error.name || 'Error', message: error && error.message || String(error), stack: String(error && error.stack || '').split('\n').slice(0, 8).join('\n')});
    if (onRejected) return onRejected(error);
    throw error;
  });
};
const __listeners = listeners;
const __elements = elementCache;
const __calls = calls;
const __clean = clean;
const __describe = describe;
const __getElement = getElement;
