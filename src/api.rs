use crate::protocol::{form, nonce, sign_v2, timestamp};
use crate::transport::{HttpRequest, HttpResponse, HttpTransport};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

pub fn browser_headers() -> BTreeMap<String, String> {
    [
        ("accept", "*/*"),
        ("accept-language", "zh-CN"),
        ("content-type", "application/x-www-form-urlencoded; charset=UTF-8"),
        ("origin", "https://chat.qwen.ai"),
        ("referer", "https://chat.qwen.ai/auth?mode=register"),
        (
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36",
        ),
        ("sec-ch-ua", "\"Not;A=Brand\";v=\"8\", \"Chromium\";v=\"150\", \"Google Chrome\";v=\"150\""),
        ("sec-ch-ua-mobile", "?0"),
        ("sec-ch-ua-platform", "\"Windows\""),
    ]
    .into_iter()
    .map(|(name, value)| (name.into(), value.into()))
    .collect()
}

#[derive(Debug, Clone)]
pub struct CaptureFixtures {
    pub init: Value,
    pub verify: Value,
    pub signup: Value,
}

impl CaptureFixtures {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read capture fixtures {}", path.display()))?;
        let records = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let init = records
            .iter()
            .find(|record| {
                record.get("type").and_then(Value::as_str) == Some("request")
                    && record
                        .get("postData")
                        .and_then(Value::as_str)
                        .is_some_and(|body| body.contains("Action=InitCaptchaV2"))
                    && !form_params(record).is_some_and(|params| {
                        params
                            .get("CertifyId")
                            .is_some_and(|value| !value.is_empty())
                    })
            })
            .cloned();
        let verify = records
            .iter()
            .find(|record| {
                record.get("type").and_then(Value::as_str) == Some("request")
                    && record
                        .get("postData")
                        .and_then(Value::as_str)
                        .is_some_and(|body| body.contains("Action=VerifyCaptchaV2"))
            })
            .cloned();
        let signup = records
            .iter()
            .find(|record| {
                record.get("type").and_then(Value::as_str) == Some("request")
                    && record.get("method").and_then(Value::as_str) == Some("POST")
                    && record
                        .get("url")
                        .and_then(Value::as_str)
                        .is_some_and(|url| url.ends_with("/api/v1/auths/signup"))
            })
            .cloned();
        let (Some(init), Some(verify), Some(signup)) = (init, verify, signup) else {
            bail!("capture fixtures not found");
        };
        Ok(Self {
            init,
            verify,
            signup,
        })
    }
}

pub fn form_params(record: &Value) -> Option<BTreeMap<String, String>> {
    let body = record.get("postData")?.as_str()?;
    Some(
        url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect(),
    )
}

pub fn init_input(fixtures: &CaptureFixtures) -> Result<BTreeMap<String, String>> {
    let params = form_params(&fixtures.init).context("Init fixture has no form body")?;
    let common = [
        "AccessKeyId",
        "SignatureMethod",
        "SignatureVersion",
        "Format",
        "Timestamp",
        "Version",
        "Action",
        "SignatureNonce",
        "Signature",
        "CertifyId",
        "DeviceToken",
    ];
    Ok(params
        .into_iter()
        .filter(|(key, _)| !common.contains(&key.as_str()))
        .collect())
}

#[derive(Clone)]
pub struct CaptchaApi {
    transport: Arc<dyn HttpTransport>,
    key_id: String,
    key_secret: String,
    init_endpoint: String,
    verify_endpoint: String,
}

impl CaptchaApi {
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        key_id: String,
        key_secret: String,
        fixtures: &CaptureFixtures,
    ) -> Result<Self> {
        Ok(Self {
            transport,
            key_id,
            key_secret,
            init_endpoint: fixtures
                .init
                .get("url")
                .and_then(Value::as_str)
                .context("Init fixture URL is missing")?
                .into(),
            verify_endpoint: fixtures
                .verify
                .get("url")
                .and_then(Value::as_str)
                .context("Verify fixture URL is missing")?
                .into(),
        })
    }

    async fn post_form(
        &self,
        url: &str,
        params: &BTreeMap<String, String>,
    ) -> Result<HttpResponse> {
        let body = form(
            params
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );
        let request = HttpRequest {
            method: "POST".into(),
            url: url.into(),
            headers: browser_headers(),
            body: Some(body.into_bytes()),
            timeout: std::time::Duration::from_secs(30),
        };
        self.transport.execute(request).await
    }

    fn signed_params(
        &self,
        action: &str,
        input: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        let mut params = BTreeMap::from([
            ("AccessKeyId".into(), self.key_id.clone()),
            ("SignatureMethod".into(), "HMAC-SHA1".into()),
            ("SignatureVersion".into(), "1.0".into()),
            ("Format".into(), "JSON".into()),
            ("Timestamp".into(), timestamp()),
            ("Version".into(), "2023-03-05".into()),
            ("Action".into(), action.into()),
        ]);
        params.extend(input.clone());
        params.insert("SignatureNonce".into(), nonce());
        let signature = sign_v2(&params, &self.key_secret).signature;
        params.insert("Signature".into(), signature);
        params
    }

    pub async fn init(&self, input: &BTreeMap<String, String>) -> Result<HttpResponse> {
        let params = self.signed_params("InitCaptchaV2", input);
        let url = self.init_endpoint.clone();
        self.post_form(&url, &params).await
    }

    pub async fn verify(&self, input: &BTreeMap<String, String>) -> Result<HttpResponse> {
        let params = self.signed_params("VerifyCaptchaV2", input);
        let url = self.verify_endpoint.clone();
        self.post_form(&url, &params).await
    }
}

pub fn user_certify_id(now_ms: i64) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let suffix = format!(
        "0a03e5{now_ms}{:09}e{:03x}",
        rng.gen_range(0..1_000_000_000u32),
        rng.gen_range(0..4096u32)
    );
    suffix.chars().take(30).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn init_input_drops_common_and_stale_challenge_fields() {
        let fixtures = CaptureFixtures {
            init: json!({"postData":"Action=InitCaptchaV2&SignatureNonce=old&SceneId=s&CertifyId=old&DeviceToken=old"}),
            verify: json!({}),
            signup: json!({}),
        };
        let input = init_input(&fixtures).unwrap();
        assert_eq!(input.get("SceneId").map(String::as_str), Some("s"));
        assert!(!input.contains_key("SignatureNonce"));
        assert!(!input.contains_key("CertifyId"));
    }

    #[test]
    fn generated_user_certify_id_has_legacy_length_limit() {
        assert!(user_certify_id(1_700_000_000_000).len() <= 30);
    }
}
