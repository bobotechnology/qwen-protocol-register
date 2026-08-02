// protocol_lab/guerrilla_mail_client.cjs
// Guerrilla Mail AJAX API client - drop-in replacement for temp_mail_client.cjs
// Docs: https://www.guerrillamail.com/GuerrillaMailAPI.html
// Requires cookie tracking (PHPSESSID = sid_token).
const https = require('https');
const { URL, URLSearchParams } = require('url');

const API_HOST = 'api.guerrillamail.com';
const API_PATH = '/ajax.php';
const USER_AGENT = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/150 Safari/537.36';

function buildUrl(params) {
  const qs = new URLSearchParams(params).toString();
  return `https://${API_HOST}${API_PATH}?${qs}`;
}

class Session {
  constructor() {
    this.cookies = {};
    this.sid = null;
    this.email = null;
  }

  async call(params) {
    if (this.sid) params.sid = this.sid;
    params.lang = params.lang || 'en';
    const url = buildUrl(params);
    return new Promise((resolve, reject) => {
      const u = new URL(url);
      const headers = {
        'Accept': 'application/json, text/plain, */*',
        'User-Agent': USER_AGENT,
        'Referer': 'https://www.guerrillamail.com/',
        'Origin': 'https://www.guerrillamail.com',
      };
      const cookieStr = Object.entries(this.cookies).map(([k, v]) => `${k}=${v}`).join('; ');
      if (cookieStr) headers['Cookie'] = cookieStr;
      const req = https.request(
        { hostname: u.hostname, path: u.pathname + u.search, method: 'GET', headers, timeout: 30000 },
        (res) => {
          for (const h of (res.headers['set-cookie'] || [])) {
            const m = h.match(/^([^=]+)=([^;]+)/);
            if (m) this.cookies[m[1]] = m[2];
          }
          let body = '';
          res.on('data', (c) => (body += c));
          res.on('end', () => {
            if (res.statusCode < 200 || res.statusCode >= 300) {
              return reject(new Error(`Guerrilla Mail HTTP ${res.statusCode}: ${body.slice(0, 200)}`));
            }
            try {
              const data = JSON.parse(body);
              if (data.sid_token) this.sid = data.sid_token;
              if (data.email_addr) this.email = data.email_addr;
              resolve(data);
            } catch (e) {
              reject(new Error('Guerrilla Mail returned invalid JSON: ' + body.slice(0, 200)));
            }
          });
          res.on('error', reject);
        }
      );
      req.setTimeout(30000, () => req.destroy(new Error('Guerrilla Mail request timeout')));
      req.on('error', reject);
      req.end();
    });
  }

  async create(userPrefix) {
    await this.call({ f: 'get_email_address' });
    if (!this.sid) throw new Error('Guerrilla Mail did not return sid_token');
    if (userPrefix) {
      await this.call({ f: 'set_email_user', email_user: userPrefix });
    }
    if (!this.email) throw new Error('Guerrilla Mail did not return email address');
    return { address: this.email, sid: this.sid, session: this };
  }

  async listMessages() {
    const data = await this.call({ f: 'check_email', seq: '0' });
    return Array.isArray(data.list) ? data.list : [];
  }

  async getMessage(mailId) {
    return this.call({ f: 'fetch_email', email_id: String(mailId) });
  }
}

function activationUrlFromValues(values, activationHost) {
  activationHost = activationHost || '';
  const candidates = [];
  for (const value of values) {
    const normalized = String(value)
      .replace(/\\\//g, '/')
      .replace(/&amp;/gi, '&')
      .replace(/&quot;/gi, '"')
      .replace(/=3D/gi, '=');
    const urlPattern = /https?:\/\/[^\s"'<>]+/gi;
    for (const match of normalized.matchAll(urlPattern)) {
      const candidate = match[0].replace(/[),.;]+$/, '');
      try {
        const url = new URL(candidate);
        const targetHost = !activationHost || url.hostname === activationHost;
        if (!targetHost) continue;
        const pathText = url.pathname + url.search;
        if (/unsubscribe|privacy|terms|support|manage-preferences/i.test(pathText)) continue;
        if (!/activate|activation|verify|confirm|validate|email|auth|token|code/i.test(pathText)) continue;
        candidates.push({ url: url.toString(), score: url.search ? 110 : 100, extractor: 'same-host-keyword' });
      } catch {}
    }
  }
  candidates.sort((a, b) => b.score - a.score);
  return candidates[0] || null;
}

function collectStrings(value, output) {
  if (!output) output = [];
  if (typeof value === 'string') output.push(value);
  else if (Array.isArray(value)) for (const item of value) collectStrings(item, output);
  else if (value && typeof value === 'object') for (const item of Object.values(value)) collectStrings(item, output);
  return output;
}

async function createMailbox(options) {
  options = options || {};
  const session = new Session();
  const prefix = options.prefix || ('reg_' + Math.random().toString(36).slice(2, 10));
  const info = await session.create(prefix);
  return {
    address: info.address,
    authorization: 'Guerrilla:' + info.sid,
    session,
    guerrilla: true,
  };
}

async function listMessages(mailbox) {
  if (!mailbox.session) throw new Error('Guerrilla Mail mailbox missing session');
  return mailbox.session.listMessages();
}

async function getMessage(mailbox, id) {
  if (!mailbox.session) throw new Error('Guerrilla Mail mailbox missing session');
  return mailbox.session.getMessage(id);
}

async function waitForActivation(mailbox, options) {
  options = options || {};
  const timeoutMs = Number(options.timeoutMs || process.env.TEMP_MAIL_TIMEOUT_MS || 120000);
  const intervalMs = Number(options.intervalMs || process.env.TEMP_MAIL_POLL_MS || 5000);
  const maxConsecutiveErrors = Number(options.maxConsecutiveErrors || 5);
  const deadline = Date.now() + timeoutMs;
  const activationHost = options.activationHost || process.env.REGISTER_ACTIVATION_HOST || '';
  const seen = new Set();
  let lastErr = null;
  let consecutiveErrors = 0;
  while (Date.now() < deadline) {
    try {
      const items = await listMessages(mailbox);
      consecutiveErrors = 0;  // reset on success
      for (const item of items) {
        const id = item.mail_id ?? item.id;
        if (id == null || seen.has(String(id))) continue;
        seen.add(String(id));
        const message = await getMessage(mailbox, id);
        const strings = collectStrings(message);
        const result = activationUrlFromValues(strings, activationHost);
        if (result) {
          return { url: result.url, extractor: result.extractor, messageId: String(id), guerrilla: true };
        }
      }
    } catch (e) {
      consecutiveErrors++;
      lastErr = e.message;
      if (consecutiveErrors >= maxConsecutiveErrors) {
        throw new Error(
          `Guerrilla Mail failed ${consecutiveErrors} consecutive times (last: ${lastErr}). ` +
          `API may be unreachable. Polling stopped after ${Math.round((Date.now() + timeoutMs - deadline) / 1000)}s.`
        );
      }
    }
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new Error('Guerrilla Mail activation email timeout after ' + timeoutMs + 'ms' + (lastErr ? ' (last: ' + lastErr + ')' : ''));
}

function activationUrl(message, activationHost) {
  activationHost = activationHost || '';
  const result = activationUrlFromValues(collectStrings(message), activationHost);
  return result ? result.url : null;
}

module.exports = { createMailbox, listMessages, getMessage, waitForActivation, activationUrl, Session };
