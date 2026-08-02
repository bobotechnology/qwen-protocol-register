// protocol_lab/decrypt_device_config.cjs
// Decrypts Aliyun CAPTCHA 2.0 DeviceConfig to extract SESSION_KEY and metadata.
//
// Keys are auto-extracted from the FeiLin runtime at probe time and cached in
// fresh_feilin_keys.json.  Built-in defaults serve as fallback for the first
// cold run after a clean checkout.
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

// ---------------------------------------------------------------------------
// Built-in defaults — correct for the current FeiLin deployment.  When the
// upstream rotates keys the runtime capture will override these automatically.
// ---------------------------------------------------------------------------
const DEFAULT_IV_HEX = "d35db7e39ebbf3d001083105";
const DEFAULT_ACCESS_SEC = "FqJB6iRNVYdEGpwb";
const DEFAULT_WEB_AES_KEYS = {
  REQ:  "8KmHIQsc5+LZJA7uYex3WaHdkjgCtS6epbG/bc9xss0=",
  RES:  "9NhnQQ+LRrKCkAuxwZaUWGBtSzaFtFlNb/ksJCrCgrM=",
  UPLOAD: "+fR9tYzlKFr07pEbumd7+KnO3xLOkphCS+qKUbJiMfA=",
  PREID: "xLLw/t15vkI7QQBX/1scBbcb9fKx+ymxF0tJ3ds42B0=",
  FLAG:  "k+1RW0cz3iDi2RAbC/c3QKzTPiVwNmNO1910DXe6Gas=",
};

// ---------------------------------------------------------------------------
// Key loading — auto-detect from cached runtime capture, fall back to defaults
// ---------------------------------------------------------------------------
let _ivHex = DEFAULT_IV_HEX;
let _accessSec = DEFAULT_ACCESS_SEC;
let _webAesKeys = { ...DEFAULT_WEB_AES_KEYS };
let _loaded = false;

function tryLoadRuntimeKeys() {
  if (_loaded) return;
  _loaded = true;

  // 1) Per-worker output dir (most recent run in this pipeline)
  const outDir = process.env.PIPELINE_OUT;
  // 2) Shared session cache — written by the first worker that completes
  //    a probe, so subsequent workers can pick up extracted keys immediately.
  const sharedCache = path.join(__dirname, 'output', '_feilin_keys_cache.json');

  let captured = null;
  for (const candidate of [outDir, sharedCache]) {
    if (!candidate) continue;
    const keysFile = typeof candidate === 'string' && candidate.endsWith('.json')
      ? candidate
      : path.join(candidate, 'fresh_feilin_keys.json');
    if (!fs.existsSync(keysFile)) continue;
    try { captured = JSON.parse(fs.readFileSync(keysFile, 'utf8')); break; } catch { /* corrupt — try next */ }
  }
  if (!captured) return;

  // New format: {accessSec, webAesKeys:[], ...} from feilinCalls extraction.
  // Old format:   array of {fn, key, len} from __FEILIN_KEYS__ (deprecated).
  if (captured.accessSec && Array.isArray(captured.webAesKeys)) {
    _accessSec = String(captured.accessSec);
    // Slot order in feilinCalls: REQ, RES, UPLOAD, PREID, FLAG (first 5).
    const slotNames = ['REQ', 'RES', 'UPLOAD', 'PREID', 'FLAG'];
    const merged = { ...DEFAULT_WEB_AES_KEYS };
    captured.webAesKeys.slice(0, 5).forEach((b64, i) => {
      const name = slotNames[i];
      merged[name] = String(b64);
    });
    _webAesKeys = merged;
    // Validate: ACCESS_SEC must be able to decrypt at least RES
    const testDecrypted = _aesDecryptSingle(_webAesKeys.RES, _accessSec);
    if (!testDecrypted || testDecrypted.length !== 16) {
      // Extracted keys are invalid — restore built-in defaults
      _accessSec = DEFAULT_ACCESS_SEC;
      _webAesKeys = { ...DEFAULT_WEB_AES_KEYS };
      return;
    }
    console.error('decrypt_device_config: using runtime-extracted keys from FeiLin probe');
    return;
  }

  // Legacy format: array of {fn, key, len} entries from __FEILIN_KEYS__
  // (kept for backward compatibility; new runs use the object format above).
  if (Array.isArray(captured) && captured.length) {
    const keyCounts = new Map();
    for (const entry of captured) {
      if (entry.fn !== 'rM' || typeof entry.key !== 'string') continue;
      if (entry.key.length !== 16) continue;
      keyCounts.set(entry.key, (keyCounts.get(entry.key) || 0) + 1);
    }
    if (keyCounts.size) {
      let bestKey = '', bestCount = 0;
      for (const [k, c] of keyCounts) { if (c > bestCount) { bestKey = k; bestCount = c; } }
      if (bestCount >= 2) {
        const testDecrypted = _aesDecryptSingle(DEFAULT_WEB_AES_KEYS.RES, bestKey);
        if (testDecrypted && testDecrypted.length === 16) {
          _accessSec = bestKey;
          console.error('decrypt_device_config: using runtime-extracted keys (legacy __FEILIN_KEYS__ format)');
        }
      }
    }
  }
  // Unrecognized format — silently ignore, built-in defaults used.

  // Count how many times each unique key value is used in rM (decrypt) calls.
  // ACCESS_SEC is the 16-char key used repeatedly to unwrap WEB_AES_KEY values.
  const keyCounts = new Map();
  for (const entry of captured) {
    if (entry.fn !== 'rM' || typeof entry.key !== 'string') continue;
    if (entry.key.length !== 16) continue;
    keyCounts.set(entry.key, (keyCounts.get(entry.key) || 0) + 1);
  }
  if (!keyCounts.size) return;

  // The candidate used most often as an rM key is almost certainly ACCESS_SEC
  // (it decrypts all six WEB_AES_KEY slots).
  let bestKey = '', bestCount = 0;
  for (const [k, c] of keyCounts) { if (c > bestCount) { bestKey = k; bestCount = c; } }
  if (bestCount < 2) return; // need at least 2 calls to be confident

  // Validate: try decrypting the built-in RES key with this candidate.
  // A valid ACCESS_SEC produces a 16-byte hex-ish key.
  const testDecrypted = _aesDecryptSingle(DEFAULT_WEB_AES_KEYS.RES, bestKey);
  if (!testDecrypted || testDecrypted.length !== 16) return;

  _accessSec = bestKey;
  console.error('decrypt_device_config: using runtime-extracted ACCESS_SEC from FeiLin probe');
}

// ---------------------------------------------------------------------------
// AES-128-CBC primitives (matching FeiLin rk / rM)
// ---------------------------------------------------------------------------
function _ivFinal() {
  const raw = Buffer.from(_ivHex, 'hex');
  return Buffer.from(raw.toString('base64'), 'utf8');
}

function _aesDecryptSingle(cipherB64, keyStr) {
  try {
    const key = Buffer.from(keyStr, 'utf8');
    const decipher = crypto.createDecipheriv('aes-128-cbc', key, _ivFinal());
    decipher.setAutoPadding(true);
    let dec = decipher.update(cipherB64, 'base64', 'utf8');
    dec += decipher.final('utf8');
    return dec;
  } catch { return null; }
}

function aesDecrypt(cipherB64, keyStr) {
  tryLoadRuntimeKeys();
  const result = _aesDecryptSingle(cipherB64, keyStr);
  if (result == null) throw new Error(`AES-128-CBC decrypt failed (keyLen=${keyStr.length}, cipherLen=${cipherB64.length})`);
  return result;
}

function aesEncrypt(plaintext, keyStr) {
  tryLoadRuntimeKeys();
  const key = Buffer.from(keyStr, 'utf8');
  const cipher = crypto.createCipheriv('aes-128-cbc', key, _ivFinal());
  cipher.setAutoPadding(true);
  let enc = cipher.update(plaintext, 'utf8', 'base64');
  enc += cipher.final('base64');
  return enc;
}

// ---------------------------------------------------------------------------
// Key derivation
// ---------------------------------------------------------------------------
function getAccessSec() {
  tryLoadRuntimeKeys();
  return _accessSec;
}

function getWebAesKeys() {
  tryLoadRuntimeKeys();
  return { ..._webAesKeys };
}

function getRuntimeKeys() {
  tryLoadRuntimeKeys();
  const keys = {};
  for (const [name, encrypted] of Object.entries(_webAesKeys)) {
    const decrypted = _aesDecryptSingle(encrypted, _accessSec);
    if (!decrypted) throw new Error(`failed to decrypt WEB_AES_KEY.${name} — ACCESS_SEC may be stale`);
    keys[name] = decrypted;
  }
  return keys;
}

// ---------------------------------------------------------------------------
// DeviceConfig decryption
// ---------------------------------------------------------------------------
function decryptDeviceConfig(deviceConfigB64) {
  tryLoadRuntimeKeys();
  const runtimeKeys = getRuntimeKeys();
  const RES_KEY = runtimeKeys.RES;

  const decrypted = aesDecrypt(deviceConfigB64, RES_KEY);
  const parts = decrypted.split('#');

  // Structure:
  // [0]: SESSION_KEY (base64 encoded)
  // [1]: version ("MQ==" = "1")
  // [2]: session header (sessionId-h-timestamp-hash)
  // [3]: FeiLin version/hash
  // [4-6]: empty
  // [7]: timestamp
  // [8]: IP
  // [9]: flag
  const sessionKeyB64 = parts[0] || '';
  const sessionKey = sessionKeyB64 ? Buffer.from(sessionKeyB64, 'base64').toString('utf8') : '';
  const sessionHdr = parts[2] || '';
  const sessionId = sessionHdr.split('-h-')[0] || '';
  const feilinVer = parts[3] || '';
  const timestamp = parts[7] || '';
  const ip = parts[8] || '';

  return {
    sessionKey,
    sessionKeyB64,
    sessionId,
    sessionHdr,
    feilinVer,
    timestamp,
    ip,
    runtimeKeys,
  };
}

// ---------------------------------------------------------------------------
// Self-test
// ---------------------------------------------------------------------------
if (require.main === module) {
  console.log('=== Runtime Keys ===\n');
  const keys = getRuntimeKeys();
  for (const [name, val] of Object.entries(keys)) {
    console.log('  %s: %s (len=%d)', name, val, val.length);
  }

  const initFile = path.join(__dirname, 'output', 'fresh_init_response.json');
  if (fs.existsSync(initFile)) {
    const init = JSON.parse(fs.readFileSync(initFile, 'utf8'));
    if (init.DeviceConfig) {
      console.log('\n=== Decrypting DeviceConfig ===\n');
      const result = decryptDeviceConfig(init.DeviceConfig);
      console.log('  sessionKey:', result.sessionKey);
      console.log('  sessionId:', result.sessionId);
      console.log('  feilinVer:', result.feilinVer);
      console.log('  timestamp:', result.timestamp);
      console.log('  ip:', result.ip);
    }
  }
}

module.exports = { decryptDeviceConfig, getRuntimeKeys, getAccessSec, getWebAesKeys, aesDecrypt, aesEncrypt };
