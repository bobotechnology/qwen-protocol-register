// protocol_lab/decrypt_device_config.cjs
// Decrypts Aliyun CAPTCHA 2.0 DeviceConfig to extract SESSION_KEY and metadata
const crypto = require("crypto");

const IV_RAW_HEX = "d35db7e39ebbf3d001083105";
const IV_FINAL = (() => {
  const raw = Buffer.from(IV_RAW_HEX, "hex");
  return Buffer.from(raw.toString("base64"), "utf8");
})();

const ACCESS_SEC = "FqJB6iRNVYdEGpwb";

// Static WEB_AES_SECRET_KEY values (encrypted with ACCESS_SEC)
const WEB_AES_SECRET_KEY = {
  REQ: "8KmHIQsc5+LZJA7uYex3WaHdkjgCtS6epbG/bc9xss0=",
  RES: "9NhnQQ+LRrKCkAuxwZaUWGBtSzaFtFlNb/ksJCrCgrM=",
  UPLOAD: "+fR9tYzlKFr07pEbumd7+KnO3xLOkphCS+qKUbJiMfA=",
  PREID: "xLLw/t15vkI7QQBX/1scBbcb9fKx+ymxF0tJ3ds42B0=",
  FLAG: "k+1RW0cz3iDi2RAbC/c3QKzTPiVwNmNO1910DXe6Gas=",
};

// AES-128-CBC decrypt (matching FeiLin rM)
function aesDecrypt(cipherB64, keyStr) {
  const key = Buffer.from(keyStr, "utf8");
  const decipher = crypto.createDecipheriv("aes-128-cbc", key, IV_FINAL);
  decipher.setAutoPadding(true);
  let dec = decipher.update(cipherB64, "base64", "utf8");
  dec += decipher.final("utf8");
  return dec;
}

// AES-128-CBC encrypt (matching FeiLin rk)
function aesEncrypt(plaintext, keyStr) {
  const key = Buffer.from(keyStr, "utf8");
  const cipher = crypto.createCipheriv("aes-128-cbc", key, IV_FINAL);
  cipher.setAutoPadding(true);
  let enc = cipher.update(plaintext, "utf8", "base64");
  enc += cipher.final("base64");
  return enc;
}

// Decrypt WEB_AES_SECRET_KEY values to get runtime keys
function getRuntimeKeys() {
  const keys = {};
  for (const [name, encrypted] of Object.entries(WEB_AES_SECRET_KEY)) {
    keys[name] = aesDecrypt(encrypted, ACCESS_SEC);
  }
  return keys;
}

// Decrypt DeviceConfig and extract session data
function decryptDeviceConfig(deviceConfigB64) {
  const runtimeKeys = getRuntimeKeys();
  const RES_KEY = runtimeKeys.RES;

  // Decrypt DeviceConfig with RES key
  const decrypted = aesDecrypt(deviceConfigB64, RES_KEY);
  const parts = decrypted.split("#");

  // Structure:
  // [0]: SESSION_KEY (base64 encoded)
  // [1]: version ("MQ==" = "1")
  // [2]: session header (sessionId-h-timestamp-hash)
  // [3]: FeiLin version/hash
  // [4-6]: empty
  // [7]: timestamp
  // [8]: IP
  // [9]: flag

  const sessionKeyB64 = parts[0];
  const sessionKey = Buffer.from(sessionKeyB64, "base64").toString("utf8");
  const sessionHdr = parts[2] || "";
  const sessionId = sessionHdr.split("-h-")[0];
  const feilinVer = parts[3] || "";
  const timestamp = parts[7] || "";
  const ip = parts[8] || "";

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

// Self-test
if (require.main === module) {
  const fs = require("fs");
  const path = require("path");

  console.log("=== Runtime Keys ===\n");
  const keys = getRuntimeKeys();
  for (const [name, val] of Object.entries(keys)) {
    console.log("  %s: %s (len=%d)", name, val, val.length);
  }
  console.log("  FINAL_KEY (UPLOAD):", keys.UPLOAD);
  console.log("  W_TOKEN_KEY (FLAG):", keys.FLAG);

  // Test with fresh init
  const initFile = path.join(__dirname, "output", "fresh_init_response.json");
  if (fs.existsSync(initFile)) {
    const init = JSON.parse(fs.readFileSync(initFile, "utf8"));
    if (init.DeviceConfig) {
      console.log("\n=== Decrypting DeviceConfig ===\n");
      const result = decryptDeviceConfig(init.DeviceConfig);
      console.log("  sessionKey:", result.sessionKey);
      console.log("  sessionId:", result.sessionId);
      console.log("  feilinVer:", result.feilinVer);
      console.log("  timestamp:", result.timestamp);
      console.log("  ip:", result.ip);
    }
  }
}

module.exports = { decryptDeviceConfig, getRuntimeKeys, aesDecrypt, aesEncrypt, ACCESS_SEC, IV_FINAL, WEB_AES_SECRET_KEY };
