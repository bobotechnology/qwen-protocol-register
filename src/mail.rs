use crate::transport::{HttpRequest, HttpTransport};
use anyhow::{Result, bail};
use rand::{Rng, distributions::Alphanumeric};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use url::Url;

const API_URL: &str = "https://api.guerrillamail.com/ajax.php";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/150 Safari/537.36";

pub struct Session {
    transport: Arc<dyn HttpTransport>,
    cookies: BTreeMap<String, String>,
    sid: Option<String>,
    email: Option<String>,
}

impl Session {
    pub fn new(transport: Arc<dyn HttpTransport>) -> Self {
        Self {
            transport,
            cookies: BTreeMap::new(),
            sid: None,
            email: None,
        }
    }

    async fn call(&mut self, mut params: BTreeMap<String, String>) -> Result<Value> {
        if let Some(sid) = &self.sid {
            params.insert("sid".into(), sid.clone());
        }
        params.entry("lang".into()).or_insert_with(|| "en".into());
        let mut url = Url::parse(API_URL)?;
        url.query_pairs_mut().extend_pairs(params.iter());
        let mut request = HttpRequest::get(url.to_string())
            .with_header("Accept", "application/json, text/plain, */*")
            .with_header("User-Agent", USER_AGENT)
            .with_header("Referer", "https://www.guerrillamail.com/")
            .with_header("Origin", "https://www.guerrillamail.com");
        if !self.cookies.is_empty() {
            request.headers.insert(
                "Cookie".into(),
                self.cookies
                    .iter()
                    .map(|(name, value)| format!("{name}={value}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
        }
        let response = self.transport.execute(request).await?;
        for cookie in response.set_cookies {
            if let Some((name, rest)) = cookie.split_once('=')
                && let Some(value) = rest.split(';').next()
            {
                self.cookies.insert(name.to_string(), value.to_string());
            }
        }
        if !(200..300).contains(&response.status) {
            bail!("Guerrilla Mail HTTP {}", response.status);
        }
        let data: Value = serde_json::from_slice(&response.body)
            .map_err(|_| anyhow::anyhow!("Guerrilla Mail returned invalid JSON"))?;
        if let Some(sid) = data.get("sid_token").and_then(Value::as_str) {
            self.sid = Some(sid.to_string());
        }
        if let Some(email) = data.get("email_addr").and_then(Value::as_str) {
            self.email = Some(email.to_string());
        }
        Ok(data)
    }

    pub async fn create(mut self, prefix: &str) -> Result<Mailbox> {
        self.call(BTreeMap::from([("f".into(), "get_email_address".into())]))
            .await?;
        if self.sid.is_none() {
            bail!("Guerrilla Mail did not return sid_token");
        }
        if !prefix.is_empty() {
            self.call(BTreeMap::from([
                ("f".into(), "set_email_user".into()),
                ("email_user".into(), prefix.into()),
            ]))
            .await?;
        }
        let Some(address) = self.email.clone() else {
            bail!("Guerrilla Mail did not return email address");
        };
        let sid = self.sid.clone().unwrap_or_default();
        Ok(Mailbox {
            address,
            authorization: format!("Guerrilla:{sid}"),
            session: self,
        })
    }

    pub async fn list_messages(&mut self) -> Result<Vec<Value>> {
        let data = self
            .call(BTreeMap::from([
                ("f".into(), "check_email".into()),
                ("seq".into(), "0".into()),
            ]))
            .await?;
        Ok(data
            .get("list")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    pub async fn get_message(&mut self, id: &str) -> Result<Value> {
        self.call(BTreeMap::from([
            ("f".into(), "fetch_email".into()),
            ("email_id".into(), id.into()),
        ]))
        .await
    }
}

pub struct Mailbox {
    pub address: String,
    pub authorization: String,
    pub session: Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub url: String,
    pub extractor: String,
    pub message_id: String,
}

pub async fn create_mailbox(transport: Arc<dyn HttpTransport>) -> Result<Mailbox> {
    let suffix = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect::<String>()
        .to_ascii_lowercase();
    Session::new(transport)
        .create(&format!("reg_{suffix}"))
        .await
}

fn collect_strings(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(text) => output.push(text.clone()),
        Value::Array(values) => {
            for value in values {
                collect_strings(value, output);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_strings(value, output);
            }
        }
        _ => {}
    }
}

fn activation_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| Regex::new(r#"https?://[^\s\"'<>]+"#).expect("valid activation URL regex"))
}

fn activation_from_values(values: &[String], activation_host: &str) -> Option<(String, String)> {
    let mut candidates = Vec::new();
    for value in values {
        let normalized = value
            .replace("\\/", "/")
            .replace("&amp;", "&")
            .replace("&AMP;", "&")
            .replace("&quot;", "\"")
            .replace("&QUOT;", "\"")
            .replace("=3D", "=")
            .replace("=3d", "=");
        for found in activation_pattern().find_iter(&normalized) {
            let candidate = found.as_str().trim_end_matches([')', ',', '.', ';']);
            let Ok(url) = Url::parse(candidate) else {
                continue;
            };
            if !activation_host.is_empty() && url.host_str() != Some(activation_host) {
                continue;
            }
            let path_text = format!("{}{}", url.path(), url.query().unwrap_or_default());
            let lower = path_text.to_ascii_lowercase();
            if [
                "unsubscribe",
                "privacy",
                "terms",
                "support",
                "manage-preferences",
            ]
            .iter()
            .any(|word| lower.contains(word))
            {
                continue;
            }
            if ![
                "activate",
                "activation",
                "verify",
                "confirm",
                "validate",
                "email",
                "auth",
                "token",
                "code",
            ]
            .iter()
            .any(|word| lower.contains(word))
            {
                continue;
            }
            candidates.push((
                if url.query().is_some() { 110 } else { 100 },
                url.to_string(),
            ));
        }
    }
    candidates.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    candidates
        .into_iter()
        .next()
        .map(|(_, url)| (url, "same-host-keyword".into()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtpCode {
    pub code: String,
    pub message_id: String,
    pub raw_body: String,
}

fn otp_code_from_values(values: &[String]) -> Option<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"(?:^|[^0-9])([0-9]{6})(?:[^0-9]|$)").expect("valid OTP regex")
    });
    for value in values {
        let normalized = value
            .replace("=\r\n", "")
            .replace("=\n", "")
            .replace("=3D", "=")
            .replace("=3d", "=");
        if let Some(captures) = pattern.captures(&normalized) {
            return captures.get(1).map(|code| code.as_str().to_string());
        }
    }
    None
}

pub async fn wait_for_otp(
    mailbox: &mut Mailbox,
    requested_after: i64,
    timeout: Duration,
    interval: Duration,
) -> Result<OtpCode> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut seen = HashSet::new();
    let mut consecutive_errors = 0u32;
    let mut last_error = None;
    while tokio::time::Instant::now() < deadline {
        match mailbox.session.list_messages().await {
            Ok(items) => {
                consecutive_errors = 0;
                for item in items {
                    let id =
                        item.get("mail_id")
                            .or_else(|| item.get("id"))
                            .map(|value| match value {
                                Value::String(text) => text.clone(),
                                other => other.to_string(),
                            });
                    let Some(id) = id.filter(|id| seen.insert(id.clone())) else {
                        continue;
                    };
                    // Skip Guerrilla Mail own welcome / system emails
                    let sender = item
                        .get("mail_from")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if sender.contains("guerrilla") {
                        continue;
                    }
                    let is_fresh = item
                        .get("mail_date")
                        .and_then(Value::as_i64)
                        .or_else(|| item.get("created_at").and_then(Value::as_i64))
                        .is_none_or(|timestamp| timestamp >= requested_after - 2);
                    if !is_fresh {
                        continue;
                    }
                    let message = mailbox.session.get_message(&id).await?;
                    let raw_body = serde_json::to_string(&message).unwrap_or_else(|_| "<unserializable>".into());
                    let mut strings = Vec::new();
                    collect_strings(&message, &mut strings);
                    if let Some(code) = otp_code_from_values(&strings) {
                        return Ok(OtpCode {
                            code,
                            message_id: id,
                            raw_body,
                        });
                    }
                }
            }
            Err(error) => {
                consecutive_errors += 1;
                last_error = Some(error.to_string());
                if consecutive_errors >= 5 {
                    bail!("mailbox failed 5 consecutive times while waiting for OTP");
                }
            }
        }
        tokio::time::sleep(interval).await;
    }
    bail!(
        "OTP email timeout after {}ms{}",
        timeout.as_millis(),
        last_error
            .map(|error| format!(" (last: {error})"))
            .unwrap_or_default()
    )
}

pub fn activation_url(message: &Value, activation_host: &str) -> Option<String> {
    let mut strings = Vec::new();
    collect_strings(message, &mut strings);
    activation_from_values(&strings, activation_host).map(|(url, _)| url)
}

pub async fn wait_for_activation(
    mailbox: &mut Mailbox,
    activation_host: &str,
    timeout: Duration,
    interval: Duration,
) -> Result<Activation> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut seen = HashSet::new();
    let mut consecutive_errors = 0u32;
    let mut last_error = None;
    while tokio::time::Instant::now() < deadline {
        match mailbox.session.list_messages().await {
            Ok(items) => {
                consecutive_errors = 0;
                for item in items {
                    let id =
                        item.get("mail_id")
                            .or_else(|| item.get("id"))
                            .map(|value| match value {
                                Value::String(text) => text.clone(),
                                other => other.to_string(),
                            });
                    let Some(id) = id.filter(|id| seen.insert(id.clone())) else {
                        continue;
                    };
                    let message = mailbox.session.get_message(&id).await?;
                    let mut strings = Vec::new();
                    collect_strings(&message, &mut strings);
                    if let Some((url, extractor)) =
                        activation_from_values(&strings, activation_host)
                    {
                        return Ok(Activation {
                            url,
                            extractor,
                            message_id: id,
                        });
                    }
                }
            }
            Err(error) => {
                consecutive_errors += 1;
                last_error = Some(error.to_string());
                if consecutive_errors >= 5 {
                    bail!(
                        "Guerrilla Mail failed 5 consecutive times (last: {})",
                        last_error.unwrap_or_default()
                    );
                }
            }
        }
        tokio::time::sleep(interval).await;
    }
    bail!(
        "Guerrilla Mail activation email timeout after {}ms{}",
        timeout.as_millis(),
        last_error
            .map(|error| format!(" (last: {error})"))
            .unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn otp_extraction_handles_boundaries_and_quoted_printable() {
        assert_eq!(
            otp_code_from_values(&["Your code: 123456. Token: 9999999".into()]).as_deref(),
            Some("123456")
        );
        assert_eq!(otp_code_from_values(&["12345".into()]), None);
        assert_eq!(otp_code_from_values(&["1234567".into()]), None);
        assert_eq!(
            otp_code_from_values(&["Code: 123=\r\n456".into()]).as_deref(),
            Some("123456")
        );
    }

    #[test]
    fn activation_requires_matching_host_and_relevant_path() {
        let message = json!({
            "body": "https://evil.test/activate?code=1 https://chat.example.test/privacy https://chat.example.test/auth/activate?token=abc"
        });
        assert_eq!(
            activation_url(&message, "chat.example.test").as_deref(),
            Some("https://chat.example.test/auth/activate?token=abc")
        );
    }

    #[test]
    fn activation_normalizes_html_and_quoted_printable() {
        let message =
            json!({"body":"https:\\/\\/chat.example.test\\/activate?token=3Dabc&amp;x=1"});
        assert!(
            activation_url(&message, "chat.example.test")
                .unwrap()
                .contains("token=abc&x=1")
        );
    }

    #[test]
    fn activation_rejects_unrelated_urls() {
        assert!(
            activation_url(&json!("https://chat.example.test/"), "chat.example.test").is_none()
        );
        assert!(
            activation_url(&json!("https://other.test/activate"), "chat.example.test").is_none()
        );
    }
}
