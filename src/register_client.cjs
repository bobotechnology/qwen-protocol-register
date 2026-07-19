const fs = require('fs');
const crypto = require('crypto');
const { BROWSER_HEADERS, passwordHash } = require('./protocol_client.cjs');
const { requestText } = require('./proxy_http.cjs');
const { createMailbox } = require('./guerrilla_mail_client.cjs');

function loadSignupTemplate(captureFile) {
  const lines = fs.readFileSync(captureFile, 'utf8').trim().split(/\r?\n/);
  const request = lines.map(JSON.parse).find(item =>
    item.type === 'request' && item.method === 'POST' && /\/api\/v1\/auths\/signup$/.test(item.url)
  );
  if (!request) throw new Error('signup request fixture not found');
  return { request, body: JSON.parse(request.postData) };
}

function buildSignupBody(template, account = {}) {
  const name = account.name || process.env.REGISTER_NAME;
  const email = account.email || process.env.REGISTER_EMAIL;
  const password = account.password || process.env.REGISTER_PASSWORD;
  if (!name || !email || !password) {
    throw new Error('REGISTER_NAME, REGISTER_EMAIL, and REGISTER_PASSWORD are required');
  }
  return {
    name,
    email,
    password: passwordHash(password),
    agree: true,
    profile_image_url: process.env.REGISTER_PROFILE_IMAGE_URL || template.profile_image_url,
    oauth_sub: '',
    oauth_token: '',
    module: process.env.REGISTER_MODULE || template.module || 'chat',
  };
}

async function postSignup(url, body, query = {}) {
  const target = new URL(url);
  for (const [key, value] of Object.entries(query)) {
    if (value != null && value !== '') target.searchParams.set(key, String(value));
  }
  const headers = {
    ...BROWSER_HEADERS,
    'content-type': 'application/json',
    'x-request-id': crypto.randomUUID(),
    'bx-v': '2.5.36',
  };
  if (process.env.REGISTER_COOKIE) headers.cookie = process.env.REGISTER_COOKIE;
  const response = await requestText(target, { method: 'POST', headers, body: JSON.stringify(body) });
  return {
    status: response.status,
    headers: response.headers,
    text: response.text,
    url: target.toString(),
  };
}

function extractWafRequestInfo(html) {
  const match = String(html).match(/<textarea[^>]+id="renderData"[^>]*>var requestInfo = ([\s\S]*?)<\/textarea>/i);
  if (!match) return null;
  const source = match[1].trim().replace(/;\s*$/, '');
  return JSON.parse(source);
}

async function beginRegistration(captureFile) {
  const fixture = loadSignupTemplate(captureFile);
  const mailbox = process.env.TEMP_MAIL_ENABLED === '1' ? await createMailbox() : null;
  const body = buildSignupBody(fixture.body, mailbox ? {
    email: mailbox.address,
    name: process.env.REGISTER_NAME || `sandbox_${mailbox.address.split('@')[0]}`,
    password: process.env.REGISTER_PASSWORD,
  } : {});
  const response = await postSignup(fixture.request.url, body);
  const info = extractWafRequestInfo(response.text);
  if (!info && response.headers['content-type']?.includes('application/json')) {
    return { endpoint: fixture.request.url, body, response, info: null, mailbox };
  }
  if (!info?.token || !info.traceid || !info.sceneId) {
    throw new Error('signup response did not contain a usable WAF requestInfo');
  }
  return { endpoint: fixture.request.url, body, response, info, mailbox };
}

async function completeRegistration(context, asig) {
  if (!context.info) return context.response;
  if (!asig) throw new Error('VerifyCaptchaV2 did not return a certifyId for u_asig');
  return postSignup(context.endpoint, context.body, {
    u_atoken: context.info.token,
    u_asig: asig,
    u_aref: context.info.refer || undefined,
  });
}

function redactRegistrationResponse(text) {
  // No redaction needed for local registration machine
  return text;
}

module.exports = { beginRegistration, completeRegistration, extractWafRequestInfo, redactRegistrationResponse };
