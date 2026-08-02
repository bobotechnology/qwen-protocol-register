// protocol_lab/run_protocol_pipeline.cjs
// Complete end-to-end pure-protocol CAPTCHA pipeline
// Init → DeviceConfig decrypt → CaptchaVerifyParam → Verify
const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");
const { initCaptcha, verifyCaptcha } = require("./protocol_client.cjs");
const { decryptDeviceConfig } = require("./decrypt_device_config.cjs");
const { loadDynamicBundle, loadFeilinBundle } = require("./dynamic_bundle_loader.cjs");
const { beginRegistration, completeRegistration, redactRegistrationResponse } = require("./register_client.cjs");
const { requestBuffer, redactProxy } = require("./proxy_http.cjs");
const { waitForActivation } = require("./guerrilla_mail_client.cjs");

const SEED_DIR = process.env.FIXTURES_DIR || path.join(__dirname, "..", "fixtures");
const OUT = process.env.PIPELINE_OUT || SEED_DIR;

async function downloadFile(url, destination) {
  const response = await requestBuffer(url);
  if (response.status < 200 || response.status >= 300) throw new Error(`captcha image download failed: HTTP ${response.status}`);
  fs.writeFileSync(destination, response.buffer);
}

async function run() {
  fs.mkdirSync(OUT, { recursive: true });
  if (process.env.PIPELINE_PROXY) console.log("Proxy:", redactProxy());
  // --- Load credentials from probe ---
  const probeFile = path.join(SEED_DIR, "probe.json");
  if (!fs.existsSync(probeFile)) throw new Error("probe_latest_full.json not found");
  const probe = JSON.parse(fs.readFileSync(probeFile, "utf8"));
  const keyId = process.env.ALIYUN_KEY_ID || probe.calls?.stateValues?.Ee?.KEY_ID;
  const keySecret = process.env.ALIYUN_KEY_SECRET || probe.calls?.stateValues?.Ee?.KEY_SECRET;
  if (!keyId || !keySecret) throw new Error("Missing Aliyun credentials. Set ALIYUN_KEY_ID/ALIYUN_KEY_SECRET or ensure fixtures/probe.json is valid");

  // --- Load endpoints from capture ---
  const captureFile = process.env.CAPTURE_FILE || path.join(SEED_DIR, "capture.jsonl");
  const lines = fs.readFileSync(captureFile, "utf8").trim().split(/\r?\n/).map(JSON.parse);
  
  const initSample = lines.find(x => x.type === "request" && x.postData?.includes("Action=InitCaptchaV2") && !new URLSearchParams(x.postData).get("CertifyId"));
  const verifySample = lines.find(x => x.type === "request" && x.postData?.includes("Action=VerifyCaptchaV2"));
  if (!initSample || !verifySample) throw new Error("Capture fixtures not found");

  const initParams = Object.fromEntries(new URLSearchParams(initSample.postData));
  const common = new Set(["AccessKeyId", "SignatureMethod", "SignatureVersion", "Format", "Timestamp", "Version", "Action", "SignatureNonce", "Signature"]);
  const initInput = Object.fromEntries(Object.entries(initParams).filter(([k]) => !common.has(k)));
  delete initInput.CertifyId;
  delete initInput.DeviceToken;
  initInput.UserCertifyId = "0a03e5" + Date.now() + String(Math.random()).slice(2, 11) + "e" + Math.random().toString(16).slice(2, 5);
  initInput.UserCertifyId = initInput.UserCertifyId.slice(0, 30);

  const cfg = { keyId, keySecret, initEndpoint: initSample.url, verifyEndpoint: verifySample.url };
  const oldVerifyParams = Object.fromEntries(new URLSearchParams(verifySample.postData));
  const registrationEnabled = Boolean(process.env.REGISTER_EMAIL || process.env.TEMP_MAIL_ENABLED === '1');
  const registrationLock = path.join(OUT, "registration_sent.lock");
  if (registrationEnabled && fs.existsSync(registrationLock)) {
    throw new Error("registration already sent for this output state");
  }
  const registration = registrationEnabled ? await beginRegistration(captureFile) : null;
  if (registration?.info) {
    initInput.SceneId = registration.info.sceneId;
    initInput.UserId = registration.info.userId;
    initInput.UserUserId = registration.info.userUserId;
    initInput.UserCertifyId = registration.info.traceid;
    initInput.Language = registration.info.language || "cn";
    initInput.Mode = "embed";
  }

  // ============================================
  // Step 1: InitCaptchaV2
  // ============================================
  console.log("[1/3] Calling InitCaptchaV2...");
  const initResp = await initCaptcha(cfg, initInput);
  const initJson = JSON.parse(initResp.text);
  console.log("  Status:", initResp.status);
  console.log("  Code:", initJson.Code);
  console.log("  CertifyId:", initJson.CertifyId);
  console.log("  StaticPath:", initJson.StaticPath);
  console.log("  DeviceConfig:", initJson.DeviceConfig ? initJson.DeviceConfig.substring(0, 40) + "..." : "NONE");

  if (initJson.Code !== "Success" || !initJson.DeviceConfig) {
    console.error("Init failed!");
    process.exit(1);
  }
  fs.writeFileSync(path.join(OUT, "fresh_init_response.json"), JSON.stringify(initJson, null, 2));

  // The runtime probe must use this challenge's gap position. A fixed offset
  // produces a well-formed token but the service rejects it as F015.
  console.log("\n[1a/4] Solving this InitCaptchaV2 challenge...");
  const imageBase = "https://static-captcha-sgp.aliyuncs.com/";
  await downloadFile(imageBase + initJson.Image, path.join(OUT, "fresh_back.png"));
  await downloadFile(imageBase + initJson.PuzzleImage, path.join(OUT, "fresh_shadow.png"));
  const offsetRaw = execFileSync("python", [
    path.join(__dirname, "solve_slider.py"),
    path.join(OUT, "fresh_back.png"),
    path.join(OUT, "fresh_shadow.png"),
  ], { encoding: "utf8", timeout: 30000 });
  // OpenCV/libpng may emit warnings to stdout. Extract the last JSON-looking line.
  const jsonLine = offsetRaw.split(/\r?\n/).filter(s => s.trim().startsWith('{')).pop();
  if (!jsonLine) throw new Error(`solve_slider.py produced no JSON output. Raw stdout (${offsetRaw.length} bytes):\n${offsetRaw.slice(0, 500)}`);
  let offset;
  try {
    offset = JSON.parse(jsonLine);
  } catch (e) {
    throw new Error(`solve_slider.py produced invalid JSON: ${e.message}\nRaw JSON line: ${jsonLine.slice(0, 300)}`);
  }
  fs.writeFileSync(path.join(OUT, "fresh_offset.json"), JSON.stringify(offset, null, 2));
  console.log("  sourceOffset:", offset.sourceOffset, "displayOffset:", offset.displayOffset);
  // Keep the solver's source coordinate. The SDK applies its own 296->300
  // scaling and displacement curve inside the runtime probe.
  // The solver marks the visible slot edge; the SDK's piece collision anchor
  // is two source pixels to its left for these 296px challenge assets.
  const runtimeSourceOffset = Number(offset.sourceOffset);
  console.log("  runtimeSourceOffset:", runtimeSourceOffset);

  // The server may rotate this bundle on every InitCaptchaV2 response. Never
  // use a checked-in or previously captured pe.* file for this session.
  console.log("\n[1b/4] Loading dynamicJS selected by this InitCaptchaV2...");
  const dynamic = await loadDynamicBundle(initJson);
  console.log("  URL:", dynamic.url);
  console.log("  SHA-256:", dynamic.sha256);
  console.log("  Raw:", dynamic.rawPath);
  console.log("  Exposed:", dynamic.exposedPath);
  console.log("  Dynamic module: loaded and syntax-validated");

  // ============================================
  // Step 2: Decrypt DeviceConfig → extract SESSION_KEY
  // ============================================
  console.log("\n[2/3] Decrypting DeviceConfig...");
  const dc = decryptDeviceConfig(initJson.DeviceConfig);
  console.log("  SESSION_KEY:", dc.sessionKey);
  console.log("  SessionId:", dc.sessionId);
  console.log("  FeiLin:", dc.feilinVer);
  console.log("  Server IP:", dc.ip);

  console.log("\n[2b/4] Loading FeiLin selected by this DeviceConfig...");
  const feilin = await loadFeilinBundle(dc.feilinVer);
  console.log("  URL:", feilin.url);
  console.log("  SHA-256:", feilin.sha256);
  console.log("  Raw:", feilin.rawPath);
  console.log("  FeiLin module: loaded and syntax-validated");

  fs.writeFileSync(path.join(OUT, "fresh_dynamic_manifest.json"), JSON.stringify({
    loadedAt: new Date().toISOString(),
    certifyId: initJson.CertifyId,
    dynamicJS: dynamic,
    feilin,
  }, null, 2));

  // Execute the current FeiLin + dynamicJS pair in the existing VM probe and
  // take its runtime-produced token. The static crypto builder is retained as
  // a vector/reference tool, but is not used for the live verification path.
  console.log("\n[2c/4] Generating CaptchaVerifyParam from the current runtime...");
  execFileSync(process.execPath, [path.join(__dirname, "build_sparse_fixture.cjs")], { stdio: "inherit" });
  const probeEnv = {
    ...process.env,
    PIPELINE_OUT: OUT,
    PROBE_POSTLOADS: [feilin.exposedPath, dynamic.exposedPath].join(path.delimiter),
    PROBE_DEVICE_CONFIG_FILE: path.join(OUT, "fresh_init_response.json"),
    PROBE_INIT_RESPONSE_FILE: path.join(OUT, "fresh_init_response.json"),
    PROBE_DEVICE_FIXTURE_FILE: path.join(OUT, "adapted_device_object.json"),
    PROBE_GATHER_COST: "725",
    PROBE_APP_NAME: "saf-captcha-waf",
    PROBE_MODE: "embed",
    PROBE_CAPTCHA_MODE: "popup",
    PROBE_SOURCE_OFFSET: String(runtimeSourceOffset),
  };
  execFileSync(process.execPath, [path.join(__dirname, "probe_bundle.cjs"), path.join(SEED_DIR, "AliyunCaptcha.exposed.js")], { env: probeEnv, stdio: "inherit" });
  execFileSync(process.execPath, [path.join(__dirname, "extract_params.cjs")], { stdio: "inherit" });
  const runtimeResult = JSON.parse(fs.readFileSync(path.join(OUT, "fresh_captcha_params.json"), "utf8"))[0];
  console.log("  runtime deviceToken: Base64 SG_WEB envelope");
  console.log("  runtime data length:", runtimeResult.data.length);

  // ============================================
  // Step 3: Generate CaptchaVerifyParam.data
  // ============================================
  console.log("\n[3/3] Generating CaptchaVerifyParam...");
  const result = runtimeResult;
  console.log("  data length:", result.data.length);
  console.log("  deviceToken:", result.deviceToken.substring(0, 60) + "...");

  // Save params
  fs.writeFileSync(path.join(OUT, "fresh_captcha_params.json"), JSON.stringify([result], null, 2));

  // ============================================
  // Step 4: VerifyCaptchaV2
  // ============================================
  console.log("\n[4/4] Calling VerifyCaptchaV2...");
  const captchaVerifyParam = JSON.stringify(result);
  const verifyResp = await verifyCaptcha(cfg, {
    SceneId: result.sceneId,
    CertifyId: result.certifyId,
    CaptchaVerifyParam: captchaVerifyParam,
    UserUserId: registration?.info?.userUserId || oldVerifyParams.UserUserId,
  });
  const verifyJson = JSON.parse(verifyResp.text);
  const verifyResult = verifyJson.Result || verifyJson;
  console.log("  Status:", verifyResp.status);
  console.log("  Code:", verifyJson.Code);
  console.log("  VerifyResult:", verifyResult.VerifyResult);
  console.log("  VerifyCode:", verifyResult.VerifyCode);
  console.log("  Message:", verifyJson.Message);
  console.log("  ResultCode:", verifyJson.ResultCode);

  fs.writeFileSync(path.join(OUT, "fresh_verify.json"), JSON.stringify({
    status: verifyResp.status,
    response: verifyJson,
    captchaVerifyParamLength: captchaVerifyParam.length,
  }, null, 2));

  if (registration) {
    if (!verifyResult.VerifyResult) {
      throw new Error(`registration blocked: CAPTCHA ${verifyResult.VerifyCode}`);
    }
    fs.writeFileSync(registrationLock, JSON.stringify({
      certifyId: verifyResult.certifyId || result.certifyId,
      createdAt: new Date().toISOString(),
    }, null, 2));
    const registrationResp = await completeRegistration(registration, verifyResult.certifyId || result.certifyId);
    const safeRegistrationUrl = new URL(registrationResp.url);
    for (const key of ["u_atoken", "u_asig", "u_aref"]) safeRegistrationUrl.searchParams.delete(key);
    const registrationJson = {
      status: registrationResp.status,
      url: safeRegistrationUrl.toString(),
      contentType: registrationResp.headers['content-type'],
      bodyLength: registrationResp.text.length,
      response: redactRegistrationResponse(registrationResp.text),
    };
    fs.writeFileSync(path.join(OUT, "fresh_registration.json"), JSON.stringify(registrationJson, null, 2));
    // Save credentials for post-registration login
    let registrationData = null;
    try { registrationData = JSON.parse(registrationResp.text); } catch {}
    const credentialsJson = {
      email: registrationData?.email || registration.body.email,
      password: process.env.REGISTER_PASSWORD,
      name: registration.body.name,
      userId: registrationData?.id || null,
      role: registrationData?.role || null,
      tier: registrationData?.tier || null,
      token_type: registrationData?.token_type || null,
      expires_at: registrationData?.expires_at || null,
      permissions: registrationData?.permissions || null,
      fullResponse: registrationData,
      createdAt: new Date().toISOString(),
    };
    fs.writeFileSync(path.join(OUT, "fresh_credentials.json"), JSON.stringify(credentialsJson, null, 2));
    if (registrationResp.status < 200 || registrationResp.status >= 300 || /aliyun_waf_aa|renderData/.test(registrationResp.text)) {
      throw new Error('registration was not accepted after CAPTCHA verification');
    }
    console.log("  Registration: accepted");
    if (registration.mailbox && process.env.TEMP_MAIL_ACTIVATE !== '0') {
      console.log("  Waiting for activation email...");
      const activation = await waitForActivation(registration.mailbox, {
        activationHost: new URL(registration.endpoint).hostname,
      });
      const activationResponse = await requestBuffer(activation.url, { headers: { accept: 'text/html,*/*' } });
      if (activationResponse.status < 200 || activationResponse.status >= 400) {
        throw new Error(`activation link returned HTTP ${activationResponse.status}`);
      }
      fs.writeFileSync(path.join(OUT, "fresh_activation.json"), JSON.stringify({
        status: activationResponse.status,
        messageId: activation.messageId,
        extractor: activation.extractor,
        opened: true,
      }, null, 2));
      console.log("  Activation: received and opened");
    }
  }

  // Summary
  console.log("\n=== SUMMARY ===");
  console.log("  CertifyId:", initJson.CertifyId);
  console.log("  SESSION_KEY:", dc.sessionKey);
  console.log("  CaptchaVerifyParam.data:", result.data.length, "bytes");
  console.log("  VerifyResult:", verifyResult.VerifyResult);
  console.log("  VerifyCode:", verifyResult.VerifyCode);

  return { init: initJson, dynamic, feilin, deviceConfig: dc, param: result, verify: verifyJson, registration };
}

run().then(r => {
  console.log("\nPipeline complete.");
}).catch(e => {
  console.error("Pipeline FAILED:", e.message);
  console.error(e.stack);
  process.exit(1);
});
