#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { spawn } = require('child_process');

const PIPELINE = path.join(__dirname, 'run_protocol_pipeline.cjs');
const DEFAULT_OUTPUT = path.join(__dirname, '..', 'batch_results.json');
const DEFAULT_TIMEOUT_MS = 180000;
const STAGGER_MS = 500;

function usage() {
  console.log(`Usage: node src/batch_register.js [options]

Registration:
  -n, --total N            Total accounts to register (default: 1)
  -w, -c, --workers N      Concurrent workers (default: 1)
      --emails A,B,C       Explicit email list (comma-separated)
      --domain DOMAIN      Generated email domain (default: luckfeed.com)

Network:
      --proxy URL          Use one proxy for all workers
      --proxy-file FILE    Use proxy URLs from a file, round-robin

Runtime:
      --guerrilla-mail     Create disposable mailboxes and activate accounts
      --output FILE        Results file (default: batch_results.json)
      --timeout MS         Per-task timeout (default: 180000)
      --cleanup             Remove per-worker temp directories after success
  -h, --help               Show this help`);
}

function requireValue(argv, index, option) {
  const value = argv[index + 1];
  if (value == null || value.startsWith('-')) {
    throw new Error(`${option} requires a value`);
  }
  return value;
}

function positiveInteger(value, option) {
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 1) {
    throw new Error(`${option} must be a positive integer`);
  }
  return number;
}

function parseArgs(argv) {
  const options = {
    total: 1,
    totalExplicit: false,
    workers: 1,
    guerrillaMail: false,
    proxy: null,
    proxyFile: null,
    emails: [],
    domain: 'luckfeed.com',
    output: DEFAULT_OUTPUT,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    cleanup: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '-h' || arg === '--help') {
      options.help = true;
    } else if (arg === '-n' || arg === '--total') {
      options.total = positiveInteger(requireValue(argv, i, arg), arg);
      options.totalExplicit = true;
      i += 1;
    } else if (arg === '-w' || arg === '-c' || arg === '--workers') {
      options.workers = positiveInteger(requireValue(argv, i, arg), arg);
      i += 1;
    } else if (arg === '--guerrilla-mail') {
      options.guerrillaMail = true;
    } else if (arg === '--proxy') {
      options.proxy = requireValue(argv, i, arg);
      i += 1;
    } else if (arg === '--proxy-file') {
      options.proxyFile = path.resolve(requireValue(argv, i, arg));
      i += 1;
    } else if (arg === '--emails') {
      options.emails = requireValue(argv, i, arg)
        .split(',')
        .map(value => value.trim())
        .filter(Boolean);
      i += 1;
    } else if (arg === '--domain') {
      options.domain = requireValue(argv, i, arg).trim().replace(/^@/, '');
      i += 1;
    } else if (arg === '--output') {
      options.output = path.resolve(requireValue(argv, i, arg));
      i += 1;
    } else if (arg === '--timeout') {
      options.timeoutMs = positiveInteger(requireValue(argv, i, arg), arg);
      i += 1;
    } else if (arg === '--cleanup') {
      options.cleanup = true;
    } else {
      throw new Error(`unknown option: ${arg}`);
    }
  }

  if (options.proxy && options.proxyFile) {
    throw new Error('--proxy and --proxy-file cannot be used together');
  }
  if (options.guerrillaMail && options.emails.length) {
    throw new Error('--guerrilla-mail and --emails cannot be used together');
  }
  if (!options.domain || /[\s/@]/.test(options.domain)) {
    throw new Error('--domain must be a valid domain name');
  }
  if (options.emails.length && !options.totalExplicit) {
    options.total = options.emails.length;
  }
  if (options.emails.length && options.total > options.emails.length) {
    throw new Error('--total cannot exceed the number of --emails entries');
  }

  options.workers = Math.min(options.workers, options.total);
  return options;
}

function randomString(length) {
  return crypto.randomBytes(Math.ceil(length / 2)).toString('hex').slice(0, length);
}

function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function redactProxy(value) {
  if (!value) return null;
  try {
    const parsed = new URL(value);
    if (parsed.username) parsed.username = '***';
    if (parsed.password) parsed.password = '***';
    return parsed.toString();
  } catch {
    return '[invalid proxy]';
  }
}

function loadProxies(options) {
  if (options.proxy) return [options.proxy];
  if (!options.proxyFile) return [];
  const proxies = fs.readFileSync(options.proxyFile, 'utf8')
    .split(/\r?\n/)
    .map(line => line.trim())
    .filter(line => line && !line.startsWith('#'));
  if (!proxies.length) throw new Error(`proxy file is empty: ${options.proxyFile}`);
  return proxies;
}

function readJson(file) {
  if (!fs.existsSync(file)) return { value: null, error: null };
  try {
    return { value: JSON.parse(fs.readFileSync(file, 'utf8')), error: null };
  } catch (error) {
    return { value: null, error: `${path.basename(file)}: ${error.message}` };
  }
}

function verifyValues(document) {
  const response = document?.response;
  const result = response?.Result || response;
  return {
    code: result?.VerifyCode || null,
    verified: result?.VerifyResult === true,
  };
}

function collectResult(context, processResult) {
  const credentialsFile = path.join(context.outputDir, 'fresh_credentials.json');
  const verifyFile = path.join(context.outputDir, 'fresh_verify.json');
  const registrationFile = path.join(context.outputDir, 'fresh_registration.json');
  const credentials = readJson(credentialsFile);
  const verify = readJson(verifyFile);
  const registration = readJson(registrationFile);
  const verification = verifyValues(verify.value);
  const creds = credentials.value;
  const fullResponse = creds?.fullResponse;
  const status = Number.isInteger(registration.value?.status)
    ? registration.value.status
    : null;

  const errors = [credentials.error, verify.error, registration.error].filter(Boolean);
  if (processResult.timedOut) {
    errors.push(`pipeline timed out after ${context.timeoutMs}ms`);
  } else if (processResult.spawnError) {
    errors.push(processResult.spawnError.message);
  } else if (processResult.code !== 0) {
    const tail = processResult.stderr.trim().split(/\r?\n/).filter(Boolean).slice(-2).join(' | ');
    errors.push(`pipeline exited with code ${processResult.code}${tail ? `: ${tail}` : ''}`);
  }

  const success = processResult.code === 0
    && !processResult.timedOut
    && verification.verified
    && status != null
    && status >= 200
    && status < 300;
  if (!success && !errors.length) errors.push('pipeline result is incomplete');

  return {
    email: creds?.email || context.email,
    name: creds?.name || context.name,
    password: creds?.password || context.password,
    userId: creds?.userId || fullResponse?.id || null,
    token: fullResponse?.token || null,
    tokenType: creds?.token_type || fullResponse?.token_type || null,
    expiresAt: creds?.expires_at || fullResponse?.expires_at || null,
    role: creds?.role || fullResponse?.role || null,
    permissions: creds?.permissions || fullResponse?.permissions || null,
    proxy: redactProxy(context.proxy),
    verifyCode: verification.code,
    verifyResult: verification.verified,
    registrationStatus: status,
    error: success ? null : errors.join('; '),
  };
}

function prefixOutput(stream, prefix, sink, onText) {
  let pending = '';
  stream.setEncoding('utf8');
  stream.on('data', chunk => {
    onText(chunk);
    pending += chunk;
    const lines = pending.split(/\r?\n/);
    pending = lines.pop();
    for (const line of lines) sink.write(`${prefix}${line}\n`);
  });
  stream.on('end', () => {
    if (pending) sink.write(`${prefix}${pending}\n`);
  });
}

function executePipeline(context, workerNumber) {
  return new Promise(resolve => {
    fs.mkdirSync(context.outputDir, { recursive: true });
    const env = {
      ...process.env,
      PIPELINE_OUT: context.outputDir,
      TEMP_MAIL_ENABLED: context.guerrillaMail ? '1' : '0',
      REGISTER_NAME: context.name,
      REGISTER_PASSWORD: context.password,
    };
    if (context.email) env.REGISTER_EMAIL = context.email;
    else delete env.REGISTER_EMAIL;
    if (context.proxy) env.PIPELINE_PROXY = context.proxy;
    else delete env.PIPELINE_PROXY;

    const child = spawn(process.execPath, [PIPELINE], {
      cwd: __dirname,
      env,
      windowsHide: true,
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let stderr = '';
    let spawnError = null;
    let timedOut = false;
    let settled = false;
    let forceKillTimer = null;
    prefixOutput(child.stdout, `[W${workerNumber}] `, process.stdout, () => {});
    prefixOutput(child.stderr, `[W${workerNumber}] `, process.stderr, chunk => {
      stderr = (stderr + chunk).slice(-8192);
    });

    const timeout = setTimeout(() => {
      timedOut = true;
      child.kill('SIGTERM');
      forceKillTimer = setTimeout(() => child.kill('SIGKILL'), 3000);
      forceKillTimer.unref();
    }, context.timeoutMs);

    child.on('error', error => {
      spawnError = error;
    });
    child.on('close', (code, signal) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      if (forceKillTimer) clearTimeout(forceKillTimer);
      resolve({ code, signal, stderr, spawnError, timedOut });
    });
  });
}

async function runTask(index, workerNumber, options, proxies) {
  const name = `sandbox_${randomString(8)}`;
  const password = `Qw9!${randomString(20)}#R7`;
  const email = options.guerrillaMail
    ? null
    : options.emails[index] || `reg_${randomString(10)}@${options.domain}`;
  const proxy = proxies.length ? proxies[index % proxies.length] : null;
  const outputDir = path.join(
    __dirname,
    'output',
    `worker_${index + 1}_${Date.now()}_${randomString(6)}`
  );
  const context = {
    outputDir,
    name,
    password,
    email,
    proxy,
    guerrillaMail: options.guerrillaMail,
    timeoutMs: options.timeoutMs,
  };
  const processResult = await executePipeline(context, workerNumber);
  return collectResult(context, processResult);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    usage();
    return;
  }
  if (!fs.existsSync(PIPELINE)) throw new Error(`pipeline not found: ${PIPELINE}`);
  const proxies = loadProxies(options);
  const startedAt = Date.now();
  const results = new Array(options.total);
  let nextTask = 0;
  let completed = 0;

  console.log(`Starting ${options.total} registration(s) with ${options.workers} worker(s)`);
  if (proxies.length) console.log(`Loaded ${proxies.length} proxy endpoint(s)`);

  async function worker(slot) {
    if (slot > 0) await delay(slot * STAGGER_MS);
    while (true) {
      const index = nextTask;
      nextTask += 1;
      if (index >= options.total) return;
      let result;
      try {
        result = await runTask(index, slot + 1, options, proxies);
      } catch (error) {
        result = {
          email: options.emails[index] || null,
          name: null,
          password: null,
          userId: null,
          token: null,
          tokenType: null,
          expiresAt: null,
          role: null,
          permissions: null,
          proxy: redactProxy(proxies.length ? proxies[index % proxies.length] : null),
          verifyCode: null,
          verifyResult: false,
          registrationStatus: null,
          error: error.message,
        };
      }
      results[index] = result;
      completed += 1;
      const state = result.error ? 'FAIL' : 'OK';
      const identity = result.email || `task-${index + 1}`;
      console.log(`[${completed}/${options.total}] ${state} ${identity} (${result.verifyCode || 'N/A'})`);
    }
  }

  await Promise.all(Array.from({ length: options.workers }, (_, slot) => worker(slot)));

  const elapsedSeconds = (Date.now() - startedAt) / 1000;
  const success = results.filter(result => result && !result.error).length;
  const report = {
    timestamp: new Date().toISOString(),
    elapsed: `${elapsedSeconds.toFixed(1)}s`,
    total: options.total,
    success,
    failed: options.total - success,
    results,
  };

  fs.mkdirSync(path.dirname(options.output), { recursive: true });
  fs.writeFileSync(options.output, JSON.stringify(report, null, 2) + '\n', 'utf8');

  console.log(`\n=== Batch Results (${report.elapsed}) ===`);
  console.log(`Total:     ${report.total}`);
  console.log(`Success:   ${report.success}`);
  console.log(`Failed:    ${report.failed}`);
  for (const result of results) {
    const state = result.error ? 'FAIL' : 'OK';
    const parts = [
      `  [${state}]`,
      result.email || '(email unavailable)',
      'pw=***',
      `verify=${result.verifyCode || 'N/A'}`,
    ];
    if (result.registrationStatus != null) parts.push(`reg=${result.registrationStatus}`);
    if (result.error) parts.push(`err=${result.error}`);
    console.log(parts.join('  '));
  }
  console.log(`\nResults written to: ${options.output}`);
  if (options.cleanup) {
    const outputDir = path.join(__dirname, 'output');
    try {
      fs.rmSync(outputDir, { recursive: true, force: true });
      console.log(`Cleaned up temp directory: ${outputDir}`);
    } catch (cleanupError) {
      console.error(`Cleanup failed: ${cleanupError.message}`);
    }
  }
  if (report.failed > 0) process.exitCode = 1;
}

main().catch(error => {
  console.error(`Batch FAILED: ${error.message}`);
  process.exitCode = 1;
});
