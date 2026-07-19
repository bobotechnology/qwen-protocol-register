// Download and prepare the dynamicJS bundle selected by InitCaptchaV2.
// Raw downloads and derived instrumented files are kept separate so a run is replayable.
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { execFileSync } = require("child_process");
const { requestBuffer } = require("./proxy_http.cjs");

function getWorkDir() { return process.env.PIPELINE_OUT || path.join(__dirname, "output"); }
function getDynamicRoot() { return path.join(getWorkDir(), "dynamic"); }
const BASE_URL = "https://g.alicdn.com/captcha-frontend/dynamicJS/";
const FEILIN_BASE_URL = "https://g.alicdn.com/captcha-frontend/FeiLin/";
const STATIC_PATH_RE = /^3\.28\.0\/pe\.[A-Za-z0-9._-]+$/;
const FEILIN_PATH_RE = /^1\.4\.2\/feilin\d+\.[a-f0-9]{32,64}$/;

async function download(url) {
  const response = await requestBuffer(url, { headers: { "user-agent": "Mozilla/5.0" } });
  if (response.status !== 200) throw new Error(`dynamicJS download failed: HTTP ${response.status}`);
  return response.buffer;
}

function safeStem(staticPath) { return staticPath.replaceAll("/", "_"); }

async function loadDynamicBundle(initResponse) {
  const staticPath = String(initResponse?.StaticPath || "");
  if (!STATIC_PATH_RE.test(staticPath)) {
    throw new Error(`unexpected InitCaptchaV2.StaticPath: ${JSON.stringify(staticPath)}`);
  }
  const url = BASE_URL + staticPath + ".js";
  const raw = await download(url);
  const stem = safeStem(staticPath);
  fs.mkdirSync(getDynamicRoot(), { recursive: true });
  const rawPath = path.join(getDynamicRoot(), stem + ".raw.js");
  const exposedPath = path.join(getDynamicRoot(), stem + ".exposed.js");
  fs.writeFileSync(rawPath, raw);
  const sha256 = crypto.createHash("sha256").update(raw).digest("hex");
  const exposeScript = path.join(__dirname, "expose_dynamic_runtime.cjs");
  execFileSync(process.execPath, [exposeScript, rawPath, exposedPath], { stdio: "inherit" });
  // Validate the derived artifact without executing untrusted code in this process.
  execFileSync(process.execPath, ["--check", exposedPath], { stdio: "pipe" });
  const exposed = fs.readFileSync(exposedPath, "utf8");
  if (!exposed.includes("window.__DYN_INTERNALS__")) {
    throw new Error("dynamicJS exposure marker missing after instrumentation");
  }
  // Existing VM tools consume this pointer. Refresh it only after the current
  // download, instrumentation, and validation have all succeeded.
  fs.writeFileSync(path.join(getWorkDir(), "fresh_dynamic_path.txt"), exposedPath);
  return { staticPath, url, rawPath, exposedPath, sha256, bytes: raw.length };
}

async function loadFeilinBundle(feilinPath) {
  const resourcePath = String(feilinPath || "");
  if (!FEILIN_PATH_RE.test(resourcePath)) {
    throw new Error(`unexpected DeviceConfig FeiLin path: ${JSON.stringify(resourcePath)}`);
  }
  const url = FEILIN_BASE_URL + resourcePath + ".js";
  const raw = await download(url);
  const stem = resourcePath.replaceAll("/", "_");
  fs.mkdirSync(getDynamicRoot(), { recursive: true });
  const rawPath = path.join(getDynamicRoot(), stem + ".raw.js");
  const exposedPath = path.join(getDynamicRoot(), stem + ".exposed.js");
  fs.writeFileSync(rawPath, raw);
  const exposeScript = path.join(__dirname, "expose_feilin_runtime.cjs");
  // Instrumentation is generated from this exact download. A rotated symbol
  // may reduce probe coverage, but it must not switch execution to raw code.
  execFileSync(process.execPath, [exposeScript, rawPath, exposedPath], { stdio: "inherit" });
  execFileSync(process.execPath, ["--check", exposedPath], { stdio: "pipe" });
  const derived = fs.readFileSync(exposedPath, "utf8");
  if (!derived.includes("__FEILIN_INSTRUMENTATION__")) {
    throw new Error("FeiLin instrumentation marker missing after instrumentation");
  }
  return { resourcePath, url, rawPath, exposedPath, instrumented: true, sha256: crypto.createHash("sha256").update(raw).digest("hex"), bytes: raw.length };
}

module.exports = { loadDynamicBundle, loadFeilinBundle, STATIC_PATH_RE, FEILIN_PATH_RE };
