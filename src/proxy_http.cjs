const http = require('http');
const https = require('https');
const { HttpsProxyAgent } = require('https-proxy-agent');
const { SocksProxyAgent } = require('socks-proxy-agent');

const agentCache = new Map();

function proxyUrl() {
  return String(process.env.PIPELINE_PROXY || '').trim();
}

function redactProxy(value = proxyUrl()) {
  if (!value) return null;
  try {
    const parsed = new URL(value);
    if (parsed.username || parsed.password) {
      parsed.username = parsed.username ? '***' : '';
      parsed.password = parsed.password ? '***' : '';
    }
    return parsed.toString();
  } catch {
    return '[invalid proxy]';
  }
}

function getAgent(targetUrl) {
  const value = proxyUrl();
  if (!value) return undefined;
  const proxy = new URL(value);
  const key = `${proxy.href}|${targetUrl.protocol}`;
  if (agentCache.has(key)) return agentCache.get(key);

  let agent;
  if (proxy.protocol === 'http:' || proxy.protocol === 'https:') {
    agent = new HttpsProxyAgent(proxy);
  } else if (['socks:', 'socks4:', 'socks4a:', 'socks5:', 'socks5h:'].includes(proxy.protocol)) {
    agent = new SocksProxyAgent(proxy);
  } else {
    throw new Error(`unsupported proxy protocol: ${proxy.protocol}`);
  }
  agentCache.set(key, agent);
  return agent;
}

function requestBuffer(url, options = {}, redirects = 0) {
  const target = url instanceof URL ? url : new URL(url);
  const transport = target.protocol === 'http:' ? http : https;
  const body = options.body == null ? null : Buffer.isBuffer(options.body) ? options.body : Buffer.from(String(options.body));
  const headers = { ...(options.headers || {}) };
  if (body && !Object.keys(headers).some(key => key.toLowerCase() === 'content-length')) {
    headers['content-length'] = String(body.length);
  }

  return new Promise((resolve, reject) => {
    let agent;
    try { agent = getAgent(target); } catch (error) { reject(error); return; }
    const request = transport.request(target, {
      method: options.method || 'GET',
      headers,
      agent,
    }, response => {
      const status = response.statusCode || 0;
      if (status >= 300 && status < 400 && response.headers.location) {
        response.resume();
        if (redirects >= 5) return reject(new Error('too many HTTP redirects'));
        const next = new URL(response.headers.location, target);
        return requestBuffer(next, options, redirects + 1).then(resolve, reject);
      }
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => resolve({
        status,
        statusText: response.statusMessage || '',
        headers: Object.fromEntries(Object.entries(response.headers).map(([key, value]) => [key.toLowerCase(), Array.isArray(value) ? value.join(', ') : String(value ?? '')])),
        buffer: Buffer.concat(chunks),
        url: target.toString(),
      }));
      response.on('error', reject);
    });
    request.setTimeout(Number(options.timeout || 30000), () => request.destroy(new Error('HTTP request timeout')));
    request.on('error', reject);
    if (body) request.write(body);
    request.end();
  });
}

async function requestText(url, options = {}) {
  const response = await requestBuffer(url, options);
  return { ...response, text: response.buffer.toString('utf8') };
}

module.exports = { requestBuffer, requestText, proxyUrl, redactProxy };
