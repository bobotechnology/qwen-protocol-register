use crate::api::{CaptchaApi, CaptureFixtures, form_params, init_input, user_certify_id};
use crate::artifact::ArtifactStore;
use crate::auth::{AuthFlow, account_from_response};
use crate::bundle::{load_dynamic_bundle, load_feilin_bundle};
use crate::device::{KeyStore, decrypt_device_config};
use crate::mail::{create_mailbox, wait_for_otp};
use crate::quickjs::{ProbeInput, run_probe};
use crate::report::TaskResult;
use crate::slider::solve_slider;
use crate::sparse::build_sparse_object;
use crate::transport::{HttpRequest, HttpTransport, ReqwestTransport, redact_proxy};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use regex::Regex;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

#[derive(Clone)]
pub struct PipelineConfig {
    pub fixtures_dir: PathBuf,
    pub output_dir: PathBuf,
    pub name: String,
    pub email: Option<String>,
    pub password: String,
    pub guerrilla_mail: bool,
    pub proxy: Option<String>,
    pub transport: Option<Arc<dyn HttpTransport>>,
    pub mail_transport: Option<Arc<dyn HttpTransport>>,
    pub timeout_ms: u64,
}

pub struct PipelineRun {
    pub result: TaskResult,
    pub output_dir: PathBuf,
}

pub async fn run_pipeline(config: PipelineConfig) -> PipelineRun {
    let output_dir = config.output_dir.clone();
    let result = match tokio::time::timeout(
        Duration::from_millis(config.timeout_ms),
        run_pipeline_inner(config.clone()),
    )
    .await
    {
        Ok(Ok(())) => collect_result(&config, None),
        Ok(Err(error)) => collect_result(&config, Some(sanitize_error(&format!("{error:#}")))),
        Err(_) => collect_result(
            &config,
            Some(format!("pipeline timed out after {}ms", config.timeout_ms)),
        ),
    };
    PipelineRun { result, output_dir }
}

async fn run_pipeline_inner(config: PipelineConfig) -> Result<()> {
    let store = ArtifactStore::new(&config.output_dir)?;
    let capture_file = std::env::var_os("CAPTURE_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| config.fixtures_dir.join("capture.jsonl"));
    let fixtures = CaptureFixtures::load(&capture_file)?;
    let credentials = load_api_credentials(&config.fixtures_dir)?;
    let transport: Arc<dyn HttpTransport> = match config.transport.clone() {
        Some(transport) => transport,
        None => Arc::new(ReqwestTransport::new(config.proxy.as_deref())?),
    };
    let api = CaptchaApi::new(transport.clone(), credentials.0, credentials.1, &fixtures)?;
    let registration_enabled = config.email.is_some() || config.guerrilla_mail;
    if registration_enabled && store.exists("registration_sent.lock") {
        bail!("registration already sent for this output state");
    }
    let mailbox = if config.guerrilla_mail {
        let direct: Arc<dyn HttpTransport> = match config.mail_transport.clone() {
            Some(transport) => transport,
            None => Arc::new(ReqwestTransport::new(None)?),
        };
        Some(create_mailbox(direct).await?)
    } else {
        None
    };
    let email = mailbox
        .as_ref()
        .map(|mailbox| mailbox.address.clone())
        .or_else(|| config.email.clone());
    let mut mailbox = mailbox;
    let mut registration = if registration_enabled {
        Some(
            AuthFlow::begin(
                transport.clone(),
                email.clone().context("registration email is missing")?,
            )
            .await?,
        )
    } else {
        None
    };
    let certify_id = if registration
        .as_ref()
        .is_some_and(AuthFlow::requires_captcha)
    {
        // Aliyun risk control rejects valid slides at random (F001). The
        // "duplicate submission" (F008) is keyed on the WAF traceid that we
        // forward as UserCertifyId — retrying with the SAME traceid always
        // yields F008. The only reliable retry is a fresh WAF session: re-run
        // AuthFlow::begin to obtain a new traceid, then run a new captcha.
        const MAX_CAPTCHA_ATTEMPTS: u32 = 3;
        let mut last_code = String::new();
        let mut certify_id: Option<String> = None;
        for captcha_attempt in 1..=MAX_CAPTCHA_ATTEMPTS {
            // On retry, re-run AuthFlow::begin to get a fresh WAF traceid.
            if captcha_attempt > 1 {
                let email = registration
                    .as_ref()
                    .map(|flow| flow.email.clone())
                    .context("registration email is missing")?;
                let new_flow = AuthFlow::begin(transport.clone(), email).await?;
                registration = Some(new_flow);
            }
            let mut input = init_input(&fixtures)?;
            input.insert(
                "UserCertifyId".into(),
                user_certify_id(Utc::now().timestamp_millis()),
            );
            if let Some(info) = registration
                .as_ref()
                .and_then(|registration| registration.info.as_ref())
            {
                input.insert("SceneId".into(), info.scene_id.clone().unwrap_or_default());
                input.insert("UserId".into(), info.user_id.clone().unwrap_or_default());
                input.insert(
                    "UserUserId".into(),
                    info.user_user_id.clone().unwrap_or_default(),
                );
                input.insert(
                    "UserCertifyId".into(),
                    info.traceid.clone().unwrap_or_default(),
                );
                input.insert(
                    "Language".into(),
                    info.language.clone().unwrap_or_else(|| "cn".into()),
                );
                input.insert("Mode".into(), "embed".into());
            }

            let init_response = api.init(&input).await?;
            let init_json: Value = serde_json::from_slice(&init_response.body)
                .context("InitCaptchaV2 returned invalid JSON")?;
            store.write_json("fresh_init_response.json", &init_json)?;
            if init_json.get("Code").and_then(Value::as_str) != Some("Success")
                || init_json
                    .get("DeviceConfig")
                    .and_then(Value::as_str)
                    .is_none()
            {
                bail!(
                    "InitCaptchaV2 failed: {}",
                    init_json
                        .get("Code")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                );
            }

            let image_base = "https://static-captcha-sgp.aliyuncs.com/";
            download_to_store(
                &transport,
                &format!("{image_base}{}", required_string(&init_json, "Image")?),
                &store,
                "fresh_back.png",
            )
            .await?;
            download_to_store(
                &transport,
                &format!(
                    "{image_base}{}",
                    required_string(&init_json, "PuzzleImage")?
                ),
                &store,
                "fresh_shadow.png",
            )
            .await?;
            let offset = solve_slider(
                &store.path("fresh_back.png"),
                &store.path("fresh_shadow.png"),
            )?;
            store.write_json("fresh_offset.json", &offset)?;

            let dynamic = load_dynamic_bundle(&init_json, transport.clone(), &store).await?;
            let keys = load_key_store(&store);
            let device = decrypt_device_config(required_string(&init_json, "DeviceConfig")?, &keys)?;
            let feilin = load_feilin_bundle(&device.feilin_ver, transport.clone(), &store).await?;
            store.write_json(
                "fresh_dynamic_manifest.json",
                &json!({
                    "loadedAt": Utc::now().to_rfc3339(),
                    "certifyId": init_json.get("CertifyId").cloned().unwrap_or(Value::Null),
                    "dynamicJS": dynamic,
                    "feilin": feilin,
                }),
            )?;
            let sparse_source =
                std::fs::read_to_string(config.fixtures_dir.join("sparse_ip_plaintext.txt"))?;
            let fixture = build_sparse_object(
                &sparse_source,
                &init_json,
                &device,
                Utc::now().timestamp_millis(),
            )?;
            store.write_json("adapted_device_object.json", &fixture)?;
            let probe = run_probe(ProbeInput {
                target_source: std::fs::read_to_string(
                    config.fixtures_dir.join("AliyunCaptcha.exposed.js"),
                )?,
                feilin_source: std::fs::read_to_string(&feilin.exposed_path)?,
                dynamic_source: std::fs::read_to_string(&dynamic.exposed_path)?,
                init_response: init_json.clone(),
                device_fixture: fixture,
                source_offset: offset.source_offset,
                transport: transport.clone(),
                output_dir: config.output_dir.clone(),
                scene_id: registration
                    .as_ref()
                    .and_then(|registration| registration.info.as_ref())
                    .and_then(|info| info.scene_id.clone()),
                app_name: Some("saf-captcha-waf".into()),
                require_telemetry: std::env::var("PROBE_REQUIRE_TELEMETRY").is_ok(),
            })
            .await?;
            if probe.keys.is_some() {
                let captured = store.read_json::<Value>("fresh_feilin_keys.json")?;
                let shared = ArtifactStore::new(shared_output_dir())?;
                shared.atomic_write_json("_feilin_keys_cache.json", &captured)?;
            }

            let captcha_param = serde_json::to_string(&probe.param)?;
            let old_verify = form_params(&fixtures.verify).unwrap_or_default();
            let mut verify_input = BTreeMap::from([
                (
                    "SceneId".into(),
                    required_string(&probe.param, "sceneId")?.into(),
                ),
                (
                    "CertifyId".into(),
                    required_string(&probe.param, "certifyId")?.into(),
                ),
                ("CaptchaVerifyParam".into(), captcha_param.clone()),
            ]);
            verify_input.insert(
                "UserUserId".into(),
                registration
                    .as_ref()
                    .and_then(|registration| registration.info.as_ref())
                    .and_then(|info| info.user_user_id.clone())
                    .or_else(|| old_verify.get("UserUserId").cloned())
                    .unwrap_or_default(),
            );
            let verify_response = api.verify(&verify_input).await?;
            let verify_json: Value = serde_json::from_slice(&verify_response.body)
                .context("VerifyCaptchaV2 returned invalid JSON")?;
            store.write_json(
                "fresh_verify.json",
                &json!({
                    "status": verify_response.status,
                    "response": verify_json,
                    "captchaVerifyParamLength": captcha_param.len(),
                    "attempt": captcha_attempt,
                }),
            )?;
            let verify_result = verify_json.get("Result").unwrap_or(&verify_json);
            let verified = verify_result.get("VerifyResult").and_then(Value::as_bool) == Some(true);
            if verified {
                let id = verify_result
                    .get("certifyId")
                    .and_then(Value::as_str)
                    .or_else(|| verify_result.get("CertifyId").and_then(Value::as_str))
                    .or_else(|| probe.param.get("certifyId").and_then(Value::as_str))
                    .context("VerifyCaptchaV2 did not return certifyId")?;
                certify_id = Some(id.to_string());
                break;
            }
            last_code = verify_result
                .get("VerifyCode")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            // Retry only on risk-control rejections; other codes are fatal.
            let retryable = matches!(last_code.as_str(), "F001" | "F008" | "F015");
            if !retryable || captcha_attempt == MAX_CAPTCHA_ATTEMPTS {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500 * captcha_attempt as u64)).await;
        }
        let Some(certify_id) = certify_id else {
            bail!("registration blocked: CAPTCHA {last_code}");
        };
        Some(certify_id)
    } else {
        None
    };
    if let Some(mut flow) = registration {
        if let Some(certify_id) = certify_id {
            let result = flow.request_otp(&transport, &certify_id).await;
            store.write_json(
                "fresh_otp_request.json",
                &json!({
                    "status": flow.response.status,
                    "response": flow.response.text(),
                }),
            )?;
            result?;
        } else if !flow.otp_requested() {
            bail!("OTP request did not complete");
        }
        store.write_json(
            "fresh_otp_request.json",
            &json!({
                "status": flow.response.status,
                "response": flow.response.text(),
            }),
        )?;
        let requested_after = Utc::now().timestamp();
        let (code, message_id, raw_body) = if let Some(mailbox) = mailbox.as_mut() {
            let otp = wait_for_otp(
                mailbox,
                requested_after,
                env_duration("TEMP_MAIL_TIMEOUT_MS", 120_000),
                env_duration("TEMP_MAIL_POLL_MS", 5_000),
            )
            .await?;
            (otp.code, Some(otp.message_id), Some(otp.raw_body))
        } else {
            let code = std::env::var("REGISTER_OTP_CODE").map_err(|_| {
                anyhow::anyhow!("REGISTER_OTP_CODE is required when no mailbox is configured")
            })?;
            (code, None, None)
        };
        store.write_json(
            "fresh_otp.json",
            &json!({"received": true, "messageId": message_id, "rawBody": raw_body}),
        )?;
        let response = flow.verify_otp(&transport, &code).await?;
        let mut account = account_from_response(&response)?;
        if account.get("token").is_none()
            && let Some(token) = account.get("access_token").cloned()
        {
            account["token"] = token;
        }
        store.write_json(
            "registration_sent.lock",
            &json!({"createdAt": Utc::now().to_rfc3339()}),
        )?;
        store.write_json(
            "fresh_registration.json",
            &json!({
                "status": response.status,
                "url": response.url,
                "contentType": response.header("content-type"),
                "bodyLength": response.body.len(),
                "response": response.text(),
            }),
        )?;
        store.write_json(
            "fresh_credentials.json",
            &json!({
                "email": account.get("email").cloned().unwrap_or_else(|| json!(flow.email)),
                "password": config.password,
                "name": account.get("name").cloned().unwrap_or_else(|| json!(config.name)),
                "userId": account.get("id").cloned().unwrap_or(Value::Null),
                "role": account.get("role").cloned().unwrap_or(Value::Null),
                "tier": account.get("tier").cloned().unwrap_or(Value::Null),
                "token_type": account.get("token_type").cloned().unwrap_or(Value::Null),
                "expires_at": account.get("expires_at").cloned().unwrap_or(Value::Null),
                "permissions": account.get("permissions").cloned().unwrap_or(Value::Null),
                "fullResponse": account,
                "createdAt": Utc::now().to_rfc3339(),
            }),
        )?;
    }
    Ok(())
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("missing required field {key}"))
}

async fn download_to_store(
    transport: &Arc<dyn HttpTransport>,
    url: &str,
    store: &ArtifactStore,
    name: &str,
) -> Result<()> {
    let response = transport.execute(HttpRequest::get(url)).await?;
    if !(200..300).contains(&response.status) {
        bail!("captcha image download failed: HTTP {}", response.status);
    }
    store.write_bytes(name, &response.body)?;
    Ok(())
}

fn load_api_credentials(fixtures_dir: &Path) -> Result<(String, String)> {
    let probe: Value = serde_json::from_slice(&std::fs::read(fixtures_dir.join("probe.json"))?)?;
    let state = probe
        .get("calls")
        .and_then(|calls| calls.get("stateValues"))
        .and_then(|values| values.get("Ee"));
    let key_id = std::env::var("ALIYUN_KEY_ID").ok().or_else(|| {
        state
            .and_then(|state| state.get("KEY_ID"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let key_secret = std::env::var("ALIYUN_KEY_SECRET").ok().or_else(|| {
        state
            .and_then(|state| state.get("KEY_SECRET"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    match (key_id, key_secret) {
        (Some(id), Some(secret)) if !id.is_empty() && !secret.is_empty() => Ok((id, secret)),
        _ => bail!(
            "Missing Aliyun credentials. Set ALIYUN_KEY_ID/ALIYUN_KEY_SECRET or ensure fixtures/probe.json is valid"
        ),
    }
}

fn load_key_store(store: &ArtifactStore) -> KeyStore {
    let local = store
        .read_optional_json::<Value>("fresh_feilin_keys.json")
        .0;
    let shared_path = shared_output_dir().join("_feilin_keys_cache.json");
    let shared = std::fs::read(shared_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    local
        .as_ref()
        .and_then(KeyStore::from_capture)
        .or_else(|| shared.as_ref().and_then(KeyStore::from_capture))
        .unwrap_or_default()
}

fn shared_output_dir() -> PathBuf {
    PathBuf::from("src").join("output")
}

fn env_duration(name: &str, default: u64) -> Duration {
    Duration::from_millis(
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default),
    )
}

fn sanitize_error(error: &str) -> String {
    static SENSITIVE_QUERY: OnceLock<Regex> = OnceLock::new();
    SENSITIVE_QUERY
        .get_or_init(|| {
            Regex::new(r"(?i)(u_atoken|u_asig|u_aref|access_key_id|access_key_secret|token|password)=([^&\s]+)")
                .expect("valid error redaction regex")
        })
        .replace_all(error, "$1=[redacted]")
        .into_owned()
}

fn collect_result(config: &PipelineConfig, pipeline_error: Option<String>) -> TaskResult {
    let store = match ArtifactStore::new(&config.output_dir) {
        Ok(store) => store,
        Err(error) => {
            return TaskResult {
                email: config.email.clone(),
                name: Some(config.name.clone()),
                password: Some(config.password.clone()),
                proxy: redact_proxy(config.proxy.as_deref()),
                error: Some(format!("{error:#}")),
                ..TaskResult::default()
            };
        }
    };
    let (credentials, credentials_error) =
        store.read_optional_json::<Value>("fresh_credentials.json");
    let (verify, verify_error) = store.read_optional_json::<Value>("fresh_verify.json");
    let (registration, registration_error) =
        store.read_optional_json::<Value>("fresh_registration.json");
    let result = verify
        .as_ref()
        .and_then(|value| value.get("response"))
        .map(|response| response.get("Result").unwrap_or(response));
    let full_response = credentials
        .as_ref()
        .and_then(|value| value.get("fullResponse"));
    let status = registration
        .as_ref()
        .and_then(|value| value.get("status"))
        .and_then(Value::as_u64)
        .map(|status| status as u16);
    let verified = result
        .and_then(|value| value.get("VerifyResult"))
        .and_then(Value::as_bool)
        == Some(true);
    let success = pipeline_error.is_none()
        && verified
        && status.is_some_and(|status| (200..300).contains(&status));
    let mut errors = [credentials_error, verify_error, registration_error]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if let Some(error) = pipeline_error {
        errors.push(error);
    }
    if !success && errors.is_empty() {
        errors.push("pipeline result is incomplete".into());
    }
    TaskResult {
        email: credentials
            .as_ref()
            .and_then(|value| value.get("email"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| config.email.clone()),
        name: credentials
            .as_ref()
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some(config.name.clone())),
        password: credentials
            .as_ref()
            .and_then(|value| value.get("password"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some(config.password.clone())),
        user_id: credentials
            .as_ref()
            .and_then(|value| value.get("userId"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                full_response
                    .and_then(|value| value.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
        token: full_response
            .and_then(|value| value.get("token"))
            .and_then(Value::as_str)
            .map(str::to_string),
        token_type: credentials
            .as_ref()
            .and_then(|value| value.get("token_type"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                full_response
                    .and_then(|value| value.get("token_type"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
        expires_at: credentials
            .as_ref()
            .and_then(|value| value.get("expires_at"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                full_response
                    .and_then(|value| value.get("expires_at"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
        role: credentials
            .as_ref()
            .and_then(|value| value.get("role"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                full_response
                    .and_then(|value| value.get("role"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
        permissions: credentials
            .as_ref()
            .and_then(|value| value.get("permissions"))
            .cloned()
            .or_else(|| {
                full_response
                    .and_then(|value| value.get("permissions"))
                    .cloned()
            }),
        proxy: redact_proxy(config.proxy.as_deref()),
        verify_code: result
            .and_then(|value| value.get("VerifyCode"))
            .and_then(Value::as_str)
            .map(str::to_string),
        verify_result: verified,
        registration_status: status,
        error: (!success).then(|| errors.join("; ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_legacy_result_from_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(directory.path()).unwrap();
        store
            .write_json(
                "fresh_verify.json",
                &json!({"response":{"Result":{"VerifyCode":"T001","VerifyResult":true}}}),
            )
            .unwrap();
        store
            .write_json("fresh_registration.json", &json!({"status":200}))
            .unwrap();
        store.write_json("fresh_credentials.json", &json!({"email":"synthetic@example.test","name":"synthetic","password":"pw","fullResponse":{"id":"user","token":"token","role":"user"}})).unwrap();
        let result = collect_result(
            &PipelineConfig {
                fixtures_dir: PathBuf::from("fixtures"),
                output_dir: directory.path().into(),
                name: "synthetic".into(),
                email: Some("synthetic@example.test".into()),
                password: "pw".into(),
                guerrilla_mail: false,
                proxy: None,
                transport: None,
                mail_transport: None,
                timeout_ms: 180_000,
            },
            None,
        );
        assert!(result.error.is_none());
        assert_eq!(result.user_id.as_deref(), Some("user"));
        assert_eq!(result.verify_code.as_deref(), Some("T001"));
    }

    #[test]
    fn failed_pipeline_does_not_claim_success() {
        let directory = tempfile::tempdir().unwrap();
        let result = collect_result(
            &PipelineConfig {
                fixtures_dir: PathBuf::from("fixtures"),
                output_dir: directory.path().into(),
                name: "synthetic".into(),
                email: None,
                password: "pw".into(),
                guerrilla_mail: false,
                proxy: None,
                transport: None,
                mail_transport: None,
                timeout_ms: 180_000,
            },
            Some("synthetic failure".into()),
        );
        assert!(result.error.unwrap().contains("synthetic failure"));
        assert!(!result.verify_result);
    }

    #[test]
    fn redacts_sensitive_query_values_from_errors() {
        let error =
            sanitize_error("request failed: https://example.test/?u_asig=secret&u_atoken=token");
        assert!(!error.contains("secret"));
        assert!(!error.contains("token=token"));
        assert!(error.contains("u_asig=[redacted]"));
    }

    #[tokio::test]
    #[ignore = "opt-in live third-party API smoke test"]
    async fn live_init_smoke() {
        if std::env::var("QWEN_LIVE_SMOKE").ok().as_deref() != Some("1") {
            return;
        }
        let fixtures_dir = PathBuf::from("fixtures");
        let fixtures = CaptureFixtures::load(&fixtures_dir.join("capture.jsonl")).unwrap();
        let (key_id, key_secret) = load_api_credentials(&fixtures_dir).unwrap();
        let proxy = std::env::var("PIPELINE_PROXY").ok();
        let transport: Arc<dyn HttpTransport> =
            Arc::new(ReqwestTransport::new(proxy.as_deref()).unwrap());
        let api = CaptchaApi::new(transport, key_id, key_secret, &fixtures).unwrap();
        let mut input = init_input(&fixtures).unwrap();
        input.insert(
            "UserCertifyId".into(),
            user_certify_id(Utc::now().timestamp_millis()),
        );
        let response = api.init(&input).await.unwrap();
        let json: Value = serde_json::from_slice(&response.body).unwrap();
        println!(
            "live init: http={}, code={}, device_config={}, static_path={}, images={}",
            response.status,
            json.get("Code")
                .and_then(Value::as_str)
                .unwrap_or("missing"),
            json.get("DeviceConfig").and_then(Value::as_str).is_some(),
            json.get("StaticPath").and_then(Value::as_str).is_some(),
            json.get("Image").and_then(Value::as_str).is_some()
                && json.get("PuzzleImage").and_then(Value::as_str).is_some()
        );
        assert!((200..300).contains(&response.status));
        assert_eq!(json.get("Code").and_then(Value::as_str), Some("Success"));
        assert!(json.get("DeviceConfig").and_then(Value::as_str).is_some());
        assert!(json.get("StaticPath").and_then(Value::as_str).is_some());
    }

    struct ReplayTransport {
        init: Value,
        dynamic: Vec<u8>,
        feilin: Vec<u8>,
        background: Vec<u8>,
        piece: Vec<u8>,
    }

    #[async_trait::async_trait]
    impl HttpTransport for ReplayTransport {
        async fn execute(&self, request: HttpRequest) -> Result<crate::transport::HttpResponse> {
            let url = url::Url::parse(&request.url)?;
            let (status, content_type, body) = if url.path().ends_with("/auths/otp/email/request") {
                if url.query_pairs().any(|(key, _)| key == "u_asig") {
                    (
                        200,
                        "application/json",
                        serde_json::to_vec(&json!({"success":true,"data":{"status":true}}))?,
                    )
                } else {
                    (200, "text/html", br#"<textarea id="renderData">var requestInfo = {"token":"waf-token","traceid":"trace","sceneId":"scene","userId":"user-id","userUserId":"user-user-id","language":"cn","refer":"ref"};</textarea>"#.to_vec())
                }
            } else if url.path().ends_with("/auths/otp/email/verify") {
                (
                    200,
                    "application/json",
                    serde_json::to_vec(
                        &json!({"success":true,"data":{"id":"user","email":"synthetic@example.test","token":"token","token_type":"Bearer","role":"user"}}),
                    )?,
                )
            } else if url.host_str() == Some("api.guerrillamail.com") {
                let action = url
                    .query_pairs()
                    .find(|(key, _)| key == "f")
                    .map(|(_, value)| value.into_owned());
                let value = match action.as_deref() {
                    Some("get_email_address") | Some("set_email_user") => {
                        json!({"sid_token":"sid","email_addr":"synthetic@example.test"})
                    }
                    Some("check_email") => {
                        json!({"list":[{"mail_id":"1","mail_date":Utc::now().timestamp()}]})
                    }
                    Some("fetch_email") => json!({"mail_body":"Your verification code is 123456."}),
                    _ => json!({}),
                };
                (200, "application/json", serde_json::to_vec(&value)?)
            } else if url.path().ends_with("/api/v1/auths/signup") {
                if url.query_pairs().any(|(key, _)| key == "u_asig") {
                    (
                        200,
                        "application/json",
                        serde_json::to_vec(
                            &json!({"id":"user","email":"synthetic@example.test","token":"token","token_type":"Bearer","role":"user"}),
                        )?,
                    )
                } else {
                    (200, "text/html", br#"<textarea id="renderData">var requestInfo = {"token":"waf-token","traceid":"trace","sceneId":"scene","userId":"user-id","userUserId":"user-user-id","language":"cn","refer":"ref"};</textarea>"#.to_vec())
                }
            } else if url.path().ends_with("/init") {
                (200, "application/json", serde_json::to_vec(&self.init)?)
            } else if url.path().ends_with("/verify") {
                (
                    200,
                    "application/json",
                    serde_json::to_vec(
                        &json!({"Code":"Success","Result":{"VerifyResult":true,"VerifyCode":"T001","certifyId":"challenge"}}),
                    )?,
                )
            } else if url.path().ends_with("/background.png") {
                (200, "image/png", self.background.clone())
            } else if url.path().ends_with("/piece.png") {
                (200, "image/png", self.piece.clone())
            } else if url.path().contains("/dynamicJS/") {
                (200, "text/javascript", self.dynamic.clone())
            } else if url.path().contains("/FeiLin/") {
                (200, "text/javascript", self.feilin.clone())
            } else {
                (404, "text/plain", b"not found".to_vec())
            };
            Ok(crate::transport::HttpResponse {
                status,
                status_text: "OK".into(),
                headers: BTreeMap::from([("content-type".into(), content_type.into())]),
                set_cookies: Vec::new(),
                body,
                url: request.url,
            })
        }
    }

    #[tokio::test]
    async fn replays_complete_pipeline_without_external_io() {
        let directory = tempfile::tempdir().unwrap();
        let fixtures = directory.path().join("fixtures");
        std::fs::create_dir(&fixtures).unwrap();
        let capture = [
            json!({"type":"request","method":"POST","url":"https://example.test/api/v1/auths/signup","postData":json!({"profile_image_url":"","module":"chat"}).to_string()}),
            json!({"type":"request","method":"POST","url":"https://example.test/init","postData":"Action=InitCaptchaV2&SceneId=fixture"}),
            json!({"type":"request","method":"POST","url":"https://example.test/verify","postData":"Action=VerifyCaptchaV2&UserUserId=fixture-user"}),
        ];
        std::fs::write(
            fixtures.join("capture.jsonl"),
            capture
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        std::fs::write(fixtures.join("probe.json"), json!({"calls":{"stateValues":{"Ee":{"KEY_ID":"synthetic-id","KEY_SECRET":"synthetic-secret"}}}}).to_string()).unwrap();
        std::fs::write(
            fixtures.join("sparse_ip_plaintext.txt"),
            vec![""; 133].join("#"),
        )
        .unwrap();
        let target = r#"
          window.__DECODED__ = {timestamp: '1', ip: '192.0.2.1', version: '1.2.3/feilin106.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'};
          window.__PROTO_INTERNALS__ = {
            ge: function() { return window.__DECODED__; },
            state: function() { return {Ie: {logs: [], initTime: Date.now(), _extend: function(value) { Object.assign(this, value); }}}; },
            Cn: function() {}, Sr: function() { return '2.0'; },
            Rt: {appName: {'2.0': 'synthetic'}, appKey: {'2.0': {sgp: 'synthetic'}}},
            br: function() { return []; }, tn: function() {}
          };
        "#;
        std::fs::write(fixtures.join("AliyunCaptcha.exposed.js"), target).unwrap();
        let dynamic = r#"
          var t = {};
          function C() {
            document.querySelector('#aliyunCaptcha-sliding-slider').addEventListener('mousedown', function() {});
            document.addEventListener('mouseup', function() {
              JSON.stringify({sceneId: 'scene', certifyId: 'challenge', deviceToken: 'synthetic-device', data: 'synthetic-data'});
            });
          }
          function P() {}
          P.reset = function() {};
          P.prototype.PowVerifyString = function() { P.reset(); };
          t.AliyunCaptcha = C;
          t=window,NodeList.prototype.forEach=Array.prototype.forEach,t.AliyunCaptcha=C
        "#;
        let feilin = "window.FEILIN={initFeiLin:function(){window.um.getToken=function(){return Promise.resolve('token')};window.z_um.getToken=window.um.getToken;return Promise.resolve({})}};";
        let runtime_keys = KeyStore::default().runtime_keys().unwrap();
        let plaintext = "c2Vzc2lvbg==#MQ==#session-h-rest#1.2.3/feilin106.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa####123#192.0.2.1#flag";
        let device_config = crate::device::aes_encrypt(plaintext, &runtime_keys.res).unwrap();
        let init = json!({"Code":"Success","CertifyId":"challenge","SceneId":"scene","DeviceConfig":device_config,"StaticPath":"1.2.3/pe.test","Image":"background.png","PuzzleImage":"piece.png"});
        let background =
            image::RgbaImage::from_fn(140, 40, |x, y| image::Rgba([(x + y) as u8, 20, 30, 255]));
        let piece = image::RgbaImage::from_fn(10, 20, |x, y| {
            if (2..8).contains(&x) && (2..18).contains(&y) {
                image::Rgba([120, 120, 120, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        });
        let mut background_bytes = std::io::Cursor::new(Vec::new());
        background
            .write_to(&mut background_bytes, image::ImageFormat::Png)
            .unwrap();
        let mut piece_bytes = std::io::Cursor::new(Vec::new());
        piece
            .write_to(&mut piece_bytes, image::ImageFormat::Png)
            .unwrap();
        let transport = Arc::new(ReplayTransport {
            init,
            dynamic: dynamic.as_bytes().to_vec(),
            feilin: feilin.as_bytes().to_vec(),
            background: background_bytes.into_inner(),
            piece: piece_bytes.into_inner(),
        });
        let run = run_pipeline(PipelineConfig {
            fixtures_dir: fixtures,
            output_dir: directory.path().join("output"),
            name: "synthetic".into(),
            email: Some("synthetic@example.test".into()),
            password: "synthetic-password".into(),
            guerrilla_mail: true,
            proxy: None,
            transport: Some(transport.clone()),
            mail_transport: Some(transport),
            timeout_ms: 180_000,
        })
        .await;
        assert!(
            run.result.error.is_none(),
            "pipeline error: {:?}",
            run.result.error
        );
        assert_eq!(run.result.verify_code.as_deref(), Some("T001"));
        assert_eq!(run.result.registration_status, Some(200));
        assert_eq!(run.result.user_id.as_deref(), Some("user"));
        assert!(run.output_dir.join("fresh_captcha_params.json").exists());
    }
}
