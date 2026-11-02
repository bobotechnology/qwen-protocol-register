# Qwen Protocol Register

Rust implementation of the protocol registration pipeline, including concurrent
batch execution, CAPTCHA runtime instrumentation, slider solving, WAF retry,
and email OTP verification. The registration command has no Node.js or
Python runtime dependency.

The runtime executes the session-selected JavaScript bundles inside an embedded
QuickJS context. Rust owns the protocol state machine, HTTP transport, SWC
instrumentation, cryptography, image processing, artifacts, and reporting.

## Build and test

```bash
cargo test
cargo fmt --check
cargo clippy -- -D warnings
```

On macOS installations where the default Command Line Tools SDK cannot link,
use an installed compatible SDK explicitly:

```bash
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk cargo test
```

## Registration CLI

```bash
cargo run -- --total 1 --workers 1 --guerrilla-mail
cargo run -- --total 20 --workers 4 --proxy socks5://127.0.0.1:10808 --guerrilla-mail
REGISTER_OTP_CODE=123456 cargo run -- --emails first@example.test --output results.json
```

Options retain the former interface:

```text
-n, --total N            Total accounts to register (default: 1)
-w, -c, --workers N      Concurrent workers (default: 1)
    --emails A,B,C       Explicit email list
    --domain DOMAIN      Generated email domain (default: luckfeed.com)
    --proxy URL          One proxy for all workers
    --proxy-file FILE    Proxy URLs, round-robin
    --guerrilla-mail     Create disposable mailboxes and read OTP codes
    --output FILE        Results JSON path (default: batch_results.json)
    --timeout MS         Per-task timeout (default: 180000)
    --cleanup            Remove successful worker artifact directories
-h, --help               Show help
```

`--proxy` and `--proxy-file` are mutually exclusive. `--guerrilla-mail` and
`--emails` are mutually exclusive. A failed task makes the batch exit nonzero.
With explicit `--emails`, provide the six-digit `REGISTER_OTP_CODE` received by
that mailbox and use one address per run. The program does not read personal
mailboxes. `--guerrilla-mail` polls the new message automatically.

## Offline fixture inspector

These commands read local files only and print aggregate counts, not URLs,
payloads, tokens, or secrets:

```bash
cargo run -- capture fixtures/capture.jsonl
cargo run -- probe fixtures/probe.json
cargo run -- sparse fixtures/sparse_ip_plaintext.txt
```

## Configuration

- `ALIYUN_KEY_ID`, `ALIYUN_KEY_SECRET`: override credentials from `fixtures/probe.json`.
- `FIXTURES_DIR`: fixture directory, default `fixtures`.
- `CAPTURE_FILE`: override the capture JSONL path.
- `PIPELINE_PROXY`: default proxy when the CLI does not provide one.
- `TEMP_MAIL_TIMEOUT_MS`, `TEMP_MAIL_POLL_MS`: OTP email polling controls.
- `REGISTER_OTP_CODE`: six-digit OTP for an explicit email address.
- `PROBE_FEILIN_WAIT_MS`: QuickJS FeiLin initialization wait, default `8000`.
- `SOLVER_MIN_OFFSET`, `SOLVER_CAPTURE_DIR`, `SOLVER_ANOMALY_DIR`, and
  `SOLVER_ANOMALY_THRESHOLD`: slider search and diagnostics.
- `PROBE_REQUIRE_TELEMETRY`: require successful Log2 and Log3 responses.

Secrets must be supplied through environment variables or local ignored files.
Tracked fixtures and historical source may contain credential-shaped material;
rotate any real credentials and replace them with synthetic values before
sharing the repository.

## Artifacts and results

Each task writes to `src/output/worker_<id>_<timestamp>_<suffix>/`:

- `fresh_init_response.json`, `fresh_offset.json`, and `fresh_verify.json`.
- `fresh_otp_request.json`, `fresh_otp.json`, `fresh_registration.json`, and
  `fresh_credentials.json`. The OTP itself is not written to `fresh_otp.json`.
- `adapted_device_object.json`, `fresh_captcha_params.json`, `probe.json`, and
  downloaded/instrumented bundles under `dynamic/`.
- `registration_sent.lock`, which prevents a duplicate submission when reusing
  an output directory.

The batch report preserves the former `batch_results.json` shape, including
camelCase fields, `elapsed`, `success`, `failed`, and per-task verification and
registration status. Proxy credentials are redacted from reports. Local
artifact files can contain registration credentials and must be protected.

## Source layout

- `src/pipeline.rs`: end-to-end protocol state machine.
- `src/auth.rs`: current authentication headers, WAF retry, and OTP exchange.
- `src/batch.rs`: concurrent CLI scheduler and report generation.
- `src/quickjs.rs`, `src/browser.js`, `src/probe_driver.js`: embedded runtime.
- `src/instrument.rs`: SWC-based dynamic and FeiLin instrumentation.
- `src/slider.rs`: pure Rust slider scoring and image diagnostics.
- `src/protocol.rs`, `src/device.rs`, `src/sparse.rs`: deterministic protocol
  primitives and fixture adaptation.
- `src/transport.rs`, `src/mail.rs`, `src/register.rs`: I/O adapters.

The test suite includes synthetic golden vectors, QuickJS bundle loading,
instrumentation checks, slider tests, and a complete offline registration
replay using fake HTTP transport. It does not create real accounts.

## License

MIT
