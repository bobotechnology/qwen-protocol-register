use crate::api::browser_headers;
use crate::register::{WafRequestInfo, extract_waf_request_info};
use crate::transport::{HttpRequest, HttpResponse, HttpTransport};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use url::Url;

pub const OTP_REQUEST_URL: &str = "https://auth.qwen.ai/api/v2/auths/otp/email/request";
pub const OTP_VERIFY_URL: &str = "https://auth.qwen.ai/api/v2/auths/otp/email/verify";

pub struct AuthFlow {
    pub email: String,
    pub info: Option<WafRequestInfo>,
    pub response: HttpResponse,
    cookies: BTreeMap<String, String>,
}

impl AuthFlow {
    pub async fn begin(transport: Arc<dyn HttpTransport>, email: String) -> Result<Self> {
        let body = json!({
            "identifier": email,
            "accept_terms": true,
            "language": "en-US",
        });
        let mut cookies = BTreeMap::new();
        let response = post_json(&transport, OTP_REQUEST_URL, &body, &[], &mut cookies).await?;
        let info = extract_waf_request_info(&response.text())?;
        if let Some(info) = &info {
            if info.token.as_deref().unwrap_or_default().is_empty()
                || info.traceid.as_deref().unwrap_or_default().is_empty()
                || info.scene_id.as_deref().unwrap_or_default().is_empty()
            {
                bail!("OTP response did not contain usable WAF requestInfo");
            }
        } else if !(200..300).contains(&response.status) {
            bail!("OTP request failed with HTTP {}", response.status);
        }
        Ok(Self {
            email,
            info,
            response,
            cookies,
        })
    }

    pub fn requires_captcha(&self) -> bool {
        self.info.is_some()
    }

    pub fn otp_requested(&self) -> bool {
        response_data(&self.response)
            .get("status")
            .and_then(Value::as_bool)
            == Some(true)
    }

    pub async fn request_otp(
        &mut self,
        transport: &Arc<dyn HttpTransport>,
        asig: &str,
    ) -> Result<()> {
        let Some(info) = &self.info else {
            if self.otp_requested() {
                return Ok(());
            }
            bail!("OTP request did not request a verification code");
        };
        if asig.is_empty() {
            bail!("VerifyCaptchaV2 did not return a certifyId for OTP request");
        }
        let body = json!({
            "identifier": self.email,
            "accept_terms": true,
            "language": "en-US",
        });
        const MAX_ATTEMPTS: u32 = 3;
        for attempt in 1..=MAX_ATTEMPTS {
            self.response = post_json(
                transport,
                OTP_REQUEST_URL,
                &body,
                &[
                    ("u_atoken", info.token.as_deref()),
                    ("u_asig", Some(asig)),
                    ("u_aref", info.refer.as_deref()),
                ],
                &mut self.cookies,
            )
            .await?;
            if (200..300).contains(&self.response.status) && self.otp_requested() {
                return Ok(());
            }
            let is_rate_limited = response_data(&self.response)
                .get("code")
                .and_then(Value::as_str)
                .is_some_and(|code| code.contains("Too_Many"));
            if !is_rate_limited || attempt == MAX_ATTEMPTS {
                bail!("OTP request was not accepted after CAPTCHA verification");
            }
            tokio::time::sleep(std::time::Duration::from_secs(3 * attempt as u64)).await;
        }
        Ok(())
    }

    pub async fn verify_otp(
        &self,
        transport: &Arc<dyn HttpTransport>,
        code: &str,
    ) -> Result<HttpResponse> {
        if !code.bytes().all(|byte| byte.is_ascii_digit()) || code.len() != 6 {
            bail!("OTP code must contain exactly six digits");
        }
        let body = json!({
            "identifier": self.email,
            "code": code,
            "name": Value::Null,
        });
        let mut cookies = self.cookies.clone();
        let response = post_json(transport, OTP_VERIFY_URL, &body, &[], &mut cookies).await?;
        if !(200..300).contains(&response.status) {
            bail!("OTP verification failed with HTTP {}", response.status);
        }
        let data = response_data(&response);
        if data.get("id").and_then(Value::as_str).is_none()
            && data.get("token").and_then(Value::as_str).is_none()
            && data.get("access_token").and_then(Value::as_str).is_none()
        {
            bail!(
                "OTP verification did not return an account (body={})",
                std::str::from_utf8(&response.body).unwrap_or("<non-utf8>")
            );
        }
        Ok(response)
    }
}

fn response_data(response: &HttpResponse) -> Value {
    serde_json::from_slice::<Value>(&response.body)
        .ok()
        .map(|value| value.get("data").cloned().unwrap_or(value))
        .unwrap_or(Value::Null)
}

async fn post_json(
    transport: &Arc<dyn HttpTransport>,
    url: &str,
    body: &Value,
    query: &[(&str, Option<&str>)],
    cookies: &mut BTreeMap<String, String>,
) -> Result<HttpResponse> {
    let mut target = Url::parse(url)?;
    for (key, value) in query {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            target.query_pairs_mut().append_pair(key, value);
        }
    }
    let mut headers = browser_headers();
    headers.insert("content-type".into(), "application/json".into());
    headers.insert("accept".into(), "application/json, text/plain, */*".into());
    headers.insert("x-request-id".into(), uuid::Uuid::new_v4().to_string());
    headers.insert("bx-v".into(), "2.5.37".into());
    headers.insert("Version".into(), "0.3.28".into());
    headers.insert("source".into(), "h5".into());
    headers.insert("x-request-origin".into(), "https://chat.qwen.ai".into());
    headers.insert(
        "Timezone".into(),
        chrono::Local::now()
            .format("%a %b %d %Y %H:%M:%S GMT%z")
            .to_string(),
    );
    if !cookies.is_empty() {
        headers.insert(
            "cookie".into(),
            cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
    let target_url = target.to_string();
    let response = transport
        .execute(HttpRequest {
            method: "POST".into(),
            url: target_url,
            headers,
            body: Some(serde_json::to_vec(body)?),
            timeout: std::time::Duration::from_secs(60),
        })
        .await?;
    for cookie in &response.set_cookies {
        if let Some((name, rest)) = cookie.split_once('=')
            && let Some(value) = rest.split(';').next()
        {
            cookies.insert(name.to_string(), value.to_string());
        }
    }
    Ok(response)
}

pub fn account_from_response(response: &HttpResponse) -> Result<Value> {
    let data = response_data(response);
    if data.is_object() {
        Ok(data)
    } else {
        Err(anyhow::anyhow!("OTP account response is not an object"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct FakeTransport {
        responses: Mutex<Vec<HttpResponse>>,
        requests: Mutex<Vec<HttpRequest>>,
    }

    impl FakeTransport {
        fn new(responses: Vec<HttpResponse>) -> Self {
            Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl HttpTransport for FakeTransport {
        async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
            self.requests.lock().unwrap().push(request);
            Ok(self.responses.lock().unwrap().remove(0))
        }
    }

    fn response(status: u16, body: Value) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".into(),
            headers: BTreeMap::from([("content-type".into(), "application/json".into())]),
            set_cookies: Vec::new(),
            body: serde_json::to_vec(&body).unwrap(),
            url: OTP_REQUEST_URL.into(),
        }
    }

    #[test]
    fn validates_six_digit_otp_before_network() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let transport = Arc::new(FakeTransport::new(Vec::new()));
            let flow = AuthFlow {
                email: "synthetic@example.test".into(),
                info: None,
                response: response(200, json!({"data":{"status":true}})),
                cookies: BTreeMap::new(),
            };
            assert!(
                flow.verify_otp(&(transport.clone() as Arc<dyn HttpTransport>), "12-34")
                    .await
                    .is_err()
            );
            assert!(transport.requests.lock().unwrap().is_empty());
        });
    }

    #[tokio::test]
    async fn verifies_current_otp_contract_and_unwraps_account() {
        let transport = Arc::new(FakeTransport::new(vec![response(
            200,
            json!({"success":true,"data":{"id":"account-id","token":"token"}}),
        )]));
        let flow = AuthFlow {
            email: "synthetic@example.test".into(),
            info: None,
            response: response(200, json!({"data":{"status":true}})),
            cookies: BTreeMap::new(),
        };
        let result = flow
            .verify_otp(&(transport.clone() as Arc<dyn HttpTransport>), "123456")
            .await
            .unwrap();
        let requests = transport.requests.lock().unwrap();
        assert_eq!(requests[0].url, OTP_VERIFY_URL);
        let body: Value = serde_json::from_slice(requests[0].body.as_ref().unwrap()).unwrap();
        assert_eq!(body["identifier"], "synthetic@example.test");
        assert_eq!(body["code"], "123456");
        assert_eq!(body["name"], Value::Null);
        drop(requests);
        assert_eq!(account_from_response(&result).unwrap()["id"], "account-id");
    }

    #[tokio::test]
    async fn failed_request_does_not_report_otp_sent() {
        let flow = AuthFlow {
            email: "synthetic@example.test".into(),
            info: None,
            response: response(400, json!({"message":"rejected"})),
            cookies: BTreeMap::new(),
        };
        assert!(!flow.otp_requested());
    }
}
