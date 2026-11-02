async function __run_probe() {
  let error = null;
  let errorStack = null;
  let decoded = null;
  const response = globalThis.__INIT_RESPONSE__ || null;
  try {
    if (response && globalThis.__PROTO_INTERNALS__) {
      const raw = response.DeviceConfig || (response.Result && response.Result.DeviceConfig) || (response.Success && response.DeviceConfig);
      decoded = globalThis.__PROTO_INTERNALS__.ge(raw);
      __calls.ge = __clean(decoded);
    }
    if (decoded && globalThis.FEILIN && globalThis.FEILIN.initFeiLin && globalThis.__PROTO_INTERNALS__) {
      const state = globalThis.__PROTO_INTERNALS__.state();
      const sdkConfig = {SceneId: globalThis.__PROBE_SCENE_ID__ || response.SceneId || '19x5u7lo', region: 'sgp', dualStack: true, verifyType: '2.0', mode: globalThis.__PROBE_MODE__ || 'popup', language: 'cn', prefix: '0a'};
      globalThis.__PROTO_INTERNALS__.Cn(sdkConfig);
      const verifyType = globalThis.__PROTO_INTERNALS__.Sr(sdkConfig) || '2.0';
      state.Ie._extend({DeviceConfig: undefined, deviceConfig: undefined, DeviceToken: undefined, verifyType});
      state.Ie._extend(sdkConfig);
      const runtime = globalThis.__PROTO_INTERNALS__.Rt;
      const config = {sceneId: sdkConfig.SceneId, appName: globalThis.__PROBE_APP_NAME__ || (runtime && runtime.appName && runtime.appName[verifyType]), appKey: runtime && runtime.appKey && runtime.appKey[verifyType] && runtime.appKey[verifyType][sdkConfig.region], endpoints: globalThis.__PROTO_INTERNALS__.br(sdkConfig.secEndpointType, verifyType, sdkConfig.region), dev: false};
      globalThis.__PROTO_INTERNALS__.tn(config.endpoints, config.appName);
      state.Ie._extend(config);
      state.Ie.logs = state.Ie.logs || [];
      state.Ie.initTime = state.Ie.initTime || Date.now();
      state.Ie._extend({deviceConfig: decoded, timestamp: decoded.timestamp});
      window.um = window.um || {};
      window.z_um = window.z_um || {};
      const callbacks = [];
      const pending = globalThis.FEILIN.initFeiLin(state.Ie, (...args) => { callbacks.push(__clean(args)); __calls.feilinCallbacks = callbacks; });
      try {
        const value = await Promise.race([Promise.resolve(pending), new Promise((_, reject) => setTimeout(() => reject(new Error('FeiLin timeout')), globalThis.__PROBE_FEILIN_WAIT_MS__ || 8000))]);
        __calls.feilin = {value: __clean(value), callbacks};
      } catch (caught) {
        __calls.feilinError = `${caught && caught.name || 'Error'}: ${caught && caught.message || String(caught)}`;
      }
    }
    const originalZGetToken = window.z_um && typeof window.z_um.getToken === 'function' ? window.z_um.getToken.bind(window.z_um) : null;
    if (globalThis.__DYN_INTERNALS__ && globalThis.__DYN_INTERNALS__.nf && globalThis.__DYN_INTERNALS__.nf.getResult) globalThis.__DYN_INTERNALS__.nf.getResult = () => Promise.resolve(null);
    if (response && typeof globalThis.AliyunCaptcha === 'function') {
      if (originalZGetToken && window.z_um) window.z_um.getToken = originalZGetToken;
      const verifyCallbacks = [];
      const config = Object.assign({}, response, {SceneId: globalThis.__PROBE_SCENE_ID__ || response.SceneId || '19x5u7lo', CaptchaType: response.CaptchaType || 'PUZZLE', mode: globalThis.__PROBE_CAPTCHA_MODE__ || 'popup', language: 'zh-cn', verifyType: '2.0', rem: 1,
        captchaVerifyCallback(...args) { verifyCallbacks.push(__clean(args)); return Promise.resolve({Success: false, Code: 'PROBE'}); }, onBizResultCallback() {}, success() {}, fail() {}, fallbackCb() {}, log() {}, _extend(value) { Object.assign(this, value); }});
      config.DeviceToken = __calls.umToken || __calls.z_umToken || '';
      const prototype = globalThis.AliyunCaptcha.prototype;
      prototype.config = config;
      prototype.deviceConfig = decoded || {};
      const instance = Object.create(prototype);
      globalThis.AliyunCaptcha.call(instance);
      __calls.captchaConstructed = true;
      const slider = __getElement('#aliyunCaptcha-sliding-slider');
      const makeEvent = (type, x, y) => ({type, isTrusted: true, clientX: x, clientY: y, pageX: x, pageY: y, screenX: x, screenY: y, offsetX: x, offsetY: y, button: 0, buttons: type === 'mouseup' ? 0 : 1, target: slider, currentTarget: slider, timeStamp: Date.now(), preventDefault() {}, stopPropagation() {}});
      const startX = 20;
      const sourceOffset = Number(globalThis.__PROBE_SOURCE_OFFSET__ || 141);
      const displayTarget = sourceOffset * 300 / 296;
      const dragDistance = (-65 + Math.sqrt(65 ** 2 + 12 * 845 * displayTarget)) / 6;
      const endX = Math.round(startX + dragDistance);
      __calls.dragGeometry = {sourceOffset, displayTarget, dragDistance, endX};
      for (const element of __elements.values()) if (typeof element.onload === 'function') element.onload({type: 'load', target: element, currentTarget: element, isTrusted: true});
      const down = __listeners.find(item => item.target === '#aliyunCaptcha-sliding-slider' && item.type === 'mousedown');
      if (down) {
        down.fn(makeEvent('mousedown', startX, 80));
        const assigned = type => [globalThis, document, document.body, slider, ...__elements.values()].flatMap(object => Object.entries(object).filter(([key, value]) => key.toLowerCase() === `on${type}` && typeof value === 'function').map(([, fn]) => fn));
        const fireMove = (x, y) => { for (const listener of [...__listeners].filter(item => item.type === 'mousemove')) listener.fn(makeEvent('mousemove', x, y)); for (const fn of assigned('mousemove')) fn(makeEvent('mousemove', x, y)); };
        const dragLength = endX - startX;
        const pointCount = 60 + Math.floor(Math.random() * 20);
        const baseY = 80;
        const overshoot = 4 + Math.floor(Math.random() * 8);
        const overshootTime = 0.60 + Math.random() * 0.15;
        const positions = [];
        for (let i = 0; i <= pointCount; i++) {
          const time = i / pointCount;
          let position;
          if (time <= overshootTime) { const local = time / overshootTime; position = startX + (dragLength + overshoot) * (1 - Math.pow(1 - local, 2.8)); }
          else { const local = (time - overshootTime) / (1 - overshootTime), peak = startX + dragLength + overshoot; position = peak + (endX - peak) * (1 - Math.pow(1 - local, 3.5)); }
          if (time > 0.75 && Math.random() < 0.06) position -= 1 + Math.random() * 3;
          positions.push(position);
        }
        positions[positions.length - 1] = endX;
        let walk = 0;
        const yPositions = [];
        for (let i = 0; i <= pointCount; i++) { walk += (Math.random() - 0.5) * 3; walk *= 0.92; walk = Math.max(-18, Math.min(18, walk)); yPositions.push(baseY + Math.round(walk)); }
        const points = [];
        for (let i = 0; i <= pointCount; i++) {
          const time = i / pointCount;
          let delay;
          if (i === 0) delay = 50 + Math.floor(Math.random() * 40);
          else if (time < 0.12) delay = 30 + Math.floor(Math.random() * 25);
          else if (time > 0.92) delay = 25 + Math.floor(Math.random() * 35);
          else if (time > overshootTime && time < overshootTime + 0.12) delay = 20 + Math.floor(Math.random() * 30);
          else delay = 8 + Math.floor(Math.random() * 18);
          if (Math.random() < 0.035 && time > 0.08 && time < 0.88) delay += 60 + Math.floor(Math.random() * 80);
          points.push({x: Math.round(positions[i]), y: yPositions[i], delay});
        }
        const jitterStart = points.length - 5 - Math.floor(Math.random() * 4);
        for (let i = jitterStart; i < points.length; i++) { points[i].x = Math.round(points[i].x + (Math.random() - 0.5) * 1.5); points[i].y = Math.round(points[i].y + (Math.random() - 0.5) * 1.5); }
        points[points.length - 1].x = endX;
        for (const point of points) { fireMove(point.x, point.y); await new Promise(resolve => setTimeout(resolve, point.delay)); }
        await new Promise(resolve => setTimeout(resolve, 40 + Math.floor(Math.random() * 60)));
        for (const listener of [...__listeners].filter(item => item.type === 'mouseup')) listener.fn(makeEvent('mouseup', endX, 80));
        for (const fn of assigned('mouseup')) fn(makeEvent('mouseup', endX, 80));
        await new Promise(resolve => setTimeout(resolve, 1000));
      }
      __calls.verifyCallbacks = verifyCallbacks;
      __calls.captchaInstance = __describe(instance);
      __calls.captchaInstanceValues = __clean(instance);
      if (instance.captcha) { __calls.puzzleInstance = __describe(instance.captcha); __calls.puzzleInstanceValues = __clean(instance.captcha); }
      __calls.listeners = __listeners.map((item, index) => ({index, target: item.target, type: item.type, name: item.fn.name || '', length: item.fn.length}));
      __calls.elements = Object.fromEntries([...__elements.entries()].map(([name, element]) => [name, {style: __clean(element.style), className: element.className, innerHTML: element.innerHTML, src: element.src, offsetWidth: element.offsetWidth, clientWidth: element.clientWidth}]));
      if (globalThis.__DYN_INTERNALS__) __calls.dynamicInternals = __clean(Object.assign({}, globalThis.__DYN_INTERNALS__, {state: globalThis.__DYN_INTERNALS__.state && globalThis.__DYN_INTERNALS__.state()}));
    }
    if (response) { const deadline = Date.now() + 12000; while (__calls.xhrCalls.some(item => item.status === undefined && !item.error) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 100)); }
    if (response && globalThis.__PROBE_REQUIRE_TELEMETRY__) {
      const businessCode = action => {
        const item = __calls.xhrCalls.find(entry => entry.action === action);
        try { return {item, code: JSON.parse(item && (item.actualResponse || item.response) || 'null').Code || null}; } catch (_) { return {item, code: null}; }
      };
      const log2 = businessCode('Log2'), log3 = businessCode('Log3');
      __calls.telemetry = {log2: {status: log2.item && log2.item.status || null, code: log2.code, bodyLength: log2.item && log2.item.bodyLength || 0}, log3: {status: log3.item && log3.item.status || null, code: log3.code, bodyLength: log3.item && log3.item.bodyLength || 0}, uploadLog: !!__calls.xhrCalls.find(item => item.action === 'UploadLog')};
      if (!log2.item || log2.code !== '200') throw new Error('Log2 did not complete successfully');
      if (!log3.item || log3.code !== '200') throw new Error('Log3 did not complete successfully');
    }
  } catch (caught) {
    error = `${caught && caught.name || 'Error'}: ${caught && caught.message || String(caught)}`;
    errorStack = String(caught && caught.stack || '').split('\n').slice(0, 16).join('\n');
  }
  if (globalThis.__FEILIN_CALLS__) __calls.feilinCalls = __clean(globalThis.__FEILIN_CALLS__);
  if (globalThis.__FEILIN_INTERNALS__) __calls.feilinInternals = __describe(globalThis.__FEILIN_INTERNALS__);
  if (globalThis.__N2_TK_PROBE__) __calls.n2TkProbe = __clean(globalThis.__N2_TK_PROBE__);
  __calls.pendingTimerCount = timerHandles.size;
  const inspected = {};
  for (const key of ['FEILIN', 'AliyunCaptcha', 'AliyunCaptcha_clientX', '_aliyun_device_ifr', '_aliyun_device_cvs', '__PROTO_INTERNALS__', '__DYN_INTERNALS__']) if (key in globalThis) inspected[key] = __describe(globalThis[key]);
  return nativeStringify({target: 'quickjs', error, errorStack, globals: Object.keys(globalThis).sort(), inspected, calls: __calls, trace});
}
