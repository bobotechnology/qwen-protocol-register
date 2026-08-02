# Qwen Protocol Register

Pure-protocol Qwen account registration with dynamic **Alibaba Cloud Captcha 2.0** instrumentation and email activation — no browser, Playwright, or Puppeteer.

> **Educational purposes only.** This project demonstrates advanced reverse engineering techniques for studying CAPTCHA systems and WAF bypass mechanisms.

## Features

- **Pure Protocol** — Zero browser dependencies; everything runs via HTTP requests
- **Alibaba Cloud Captcha 2.0 Bypass** — Full reverse engineering of `InitCaptchaV2` / `VerifyCaptchaV2` flow
- **Dynamic JS Deobfuscation** — Runtime instrumentation of FeiLin + dynamicJS modules with AST probes
- **High-Precision Slider Solver** — OpenCV edge-fusion algorithm (>95% accuracy) replacing template matching
- **Batch Registration** — Concurrent multi-account registration with configurable workers
- **Auto-Activation** — Guerrilla Mail integration for automatic email verification
- **Self-Healing Crypto** — ACCESS_SEC and WEB_AES_KEY slots auto-extracted from FeiLin runtime, no manual env vars needed when upstream rotates keys
- **Proxy Support** — SOCKS5/HTTP proxy chaining through local Xray or direct connections

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  batch_register.js                   │
│              (concurrent task scheduler)              │
├──────────┬──────────┬──────────┬────────────────────┤
│ worker_0 │ worker_1 │ worker_2 │      worker_N      │
└────┬─────┴────┬─────┴────┬─────┴────────┬───────────┘
     │          │          │              │
     ▼          ▼          ▼              ▼
┌─────────────────────────────────────────────────────┐
│            run_protocol_pipeline.cjs                  │
│                                                      │
│  1. beginRegistration() ──► Guerrilla Mail mailbox   │
│  2. InitCaptchaV2       ──► Aliyun API               │
│  3. solve_slider.py     ──► OpenCV edge detection    │
│  4. probe_bundle.cjs    ──► VM sandbox execution     │
│  5. extract FeiLin keys  ──► auto-detect ACCESS_SEC   │
│  6. VerifyCaptchaV2     ──► Aliyun API (T001)        │
│  7. completeRegistration ──► Target site signup       │
│  8. waitForActivation   ──► Email polling            │
└─────────────────────────────────────────────────────┘
```

### Key Components

| Module | Purpose |
|--------|---------|
| `protocol_client.cjs` | Alibaba Cloud API signing (HMAC-SHA1) and request building |
| `decrypt_device_config.cjs` | AES-ECB decryption of DeviceConfig; auto-extracts keys from FeiLin runtime probe |
| `dynamic_bundle_loader.cjs` | Download + instrument dynamicJS/FeiLin bundles at runtime |
| `probe_bundle.cjs` | VM sandbox with full browser API stubs for captcha JS |
| `expose_feilin_runtime.cjs` | Structural instrumentation of FeiLin crypto module |
| `solve_slider.py` | Multi-feature edge fusion slider gap detection |
| `guerrilla_mail_client.cjs` | Guerrilla Mail API client for disposable email |
| `register_client.cjs` | Target site signup protocol implementation |

## Prerequisites

- **Node.js** >= 18.0.0
- **Python** >= 3.10 with OpenCV (`pip install opencv-python-headless numpy`)
- **Proxy** (recommended) — Xray or any SOCKS5 proxy for IP rotation

## Quick Start

```bash
# 1. Install dependencies
npm install

# 2. Install Python dependencies
pip install opencv-python-headless numpy

# 3. Run batch registration (direct connection)
node src/batch_register.js --total 5 --workers 2 --guerrilla-mail

# 4. With proxy
node src/batch_register.js --total 20 --workers 4 --proxy socks5://127.0.0.1:10808 --guerrilla-mail
```

## CLI Options

```
Registration:
  -n, --total N            Total accounts to register (default: 1)
  -w, -c, --workers N      Concurrent workers (default: 1)
      --emails A,B,C       Explicit email list (comma-separated)
      --domain DOMAIN      Email domain (default: luckfeed.com)

Network:
      --proxy URL          Use proxy for all workers
      --proxy-file FILE    One proxy URL per line

Runtime:
      --guerrilla-mail     Auto-create disposable email + activate
      --output FILE        Results JSON path (default: batch_results.json)
      --timeout MS         Per-task timeout (default: 180000)
      --cleanup             Remove per-worker temp directories after run
  -h, --help               Show help
```

## Output

### batch_results.json

```json
{
  "timestamp": "2026-07-19T13:44:10.000Z",
  "elapsed": "51.3s",
  "total": 1,
  "success": 1,
  "failed": 0,
  "results": [
    {
      "email": "reg_xxx@guerrillamailblock.com",
      "name": "sandbox_xxxxxxxx",
      "password": "Qw9!RandomPass#R7",
      "userId": "00000000-0000-0000-0000-000000000000",
      "token": "eyJ...",
      "tokenType": "Bearer",
      "expiresAt": "2026-07-20T13:44:10Z",
      "role": "user",
      "permissions": {},
      "proxy": null,
      "verifyCode": "T001",
      "verifyResult": true,
      "registrationStatus": 200,
      "error": null
    }
  ]
}
```

### `fresh_credentials.json` (per worker)

Contains full registration data including JWT token, user ID, and permissions.

## How It Works

### 1. Captcha Challenge Acquisition

Calls `InitCaptchaV2` to receive:
- `CertifyId` — unique challenge identifier
- `DeviceConfig` — AES-encrypted device fingerprint config
- `StaticPath` — dynamicJS bundle version selector

> **Key extraction is automatic.** During VM sandbox execution the FeiLin runtime's internal `rc()` cipher is traced. `ACCESS_SEC` and all five `WEB_AES_KEY` slots (REQ/RES/UPLOAD/PREID/FLAG) are captured from the first probe run and cached to `src/output/_feilin_keys_cache.json`. The built-in defaults serve as fallback only on a fresh checkout — every subsequent run uses live-extracted keys.

### 2. Dynamic Module Instrumentation

Downloads the challenge-specific `dynamicJS` and `FeiLin` bundles from `g.alicdn.com`, then:
- Injects structural probes via AST-free string instrumentation
- Executes in a Node.js `vm` sandbox with full browser API stubs
- Intercepts `CryptoJS.AES.encrypt` to capture key/IV/plaintext
- Extracts `deviceToken` and `CaptchaVerifyParam.data`

### 3. Slider Gap Detection

OpenCV-based multi-feature fusion:
- **Alpha boundary** — PNG transparency edge as template skeleton
- **Canny edges** — structural contour matching
- **Brightness/saturation variance** — detects shadow-filled gaps
- **Boundary gradient analysis** — robust against feathering and anti-aliasing

### 4. Verification & Registration

- Signs requests with `HMAC-SHA1` using Aliyun AccessKey from the captcha bundle
- Posts `CaptchaVerifyParam` to `VerifyCaptchaV2`
- On `T001` (success), completes registration with the WAF `u_asig` token
- Polls Guerrilla Mail inbox for activation link

## Project Structure

```
├── src/                          # Source code
│   ├── batch_register.js         # Entry point & task scheduler
│   ├── run_protocol_pipeline.cjs # Per-account pipeline orchestrator
│   ├── protocol_client.cjs       # Aliyun API client
│   ├── register_client.cjs       # Target site registration
│   ├── probe_bundle.cjs          # VM sandbox for captcha JS
│   ├── dynamic_bundle_loader.cjs # Dynamic JS fetcher
│   ├── decrypt_device_config.cjs # DeviceConfig decryption
│   ├── build_sparse_fixture.cjs  # Runtime device fixture builder
│   ├── expose_feilin_runtime.cjs # FeiLin instrumentation
│   ├── expose_dynamic_runtime.cjs# dynamicJS instrumentation
│   ├── extract_params.cjs        # Parameter extraction
│   ├── guerrilla_mail_client.cjs # Email client
│   ├── proxy_http.cjs            # Proxy transport layer
│   └── solve_slider.py           # OpenCV slider solver
├── fixtures/                     # Required runtime fixtures
│   ├── capture.jsonl             # API request templates
│   ├── probe.json                # Captcha credential seed
│   ├── sparse_ip_plaintext.txt   # Device fixture source
│   └── AliyunCaptcha.exposed.js  # Pre-instrumented captcha bundle
├── package-lock.json
├── package.json
├── requirements.txt
├── .gitignore
└── LICENSE
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `PIPELINE_PROXY` | Proxy URL for API calls | *(none)* |
| `TEMP_MAIL_TIMEOUT_MS` | Email polling timeout | `120000` |
| `TEMP_MAIL_POLL_MS` | Email poll interval | `5000` |
| `FIXTURES_DIR` | Override fixtures directory | `./fixtures` |
| `CAPTURE_FILE` | Override capture fixture path | `fixtures/capture.jsonl` |
| `FEILIN_KEYS_DEBUG` | Log key-loading decisions to stderr | *(none)* |

### Adding Proxy Support

The proxy parameter accepts standard URL formats:
```
socks5://user:pass@host:port
socks5h://host:port
http://host:port
https://host:port
```

For chain proxying (local Xray → upstream proxy):
```bash
# Xray config routes all traffic through upstream
node src/batch_register.js --proxy socks5://127.0.0.1:10808 --guerrilla-mail
```

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| `F015` | Slider position inaccurate | Adjust `solve_slider.py` tolerance |
| `F001` | Captcha timed out | Increase pipeline timeout |
| `F008` | Duplicate submission | Use fresh Guerrilla Mail address |
| HTTP 429 | Rate limited | Add proxy or reduce concurrency |

## Disclaimer

This project is for **educational and research purposes only**. The authors are not responsible for any misuse. Respect the terms of service of any platform you interact with.

## License

MIT
