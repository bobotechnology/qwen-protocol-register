use crate::api::{CaptureFixtures, browser_headers};
use crate::mail::Mailbox;
use crate::protocol::password_hash;
use crate::transport::{HttpRequest, HttpResponse, HttpTransport};
use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::sync::{Arc, OnceLock};
use url::Url;

#[derive(Debug, Clone)]
pub struct Account {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WafRequestInfo {
    pub token: Option<String>,
    pub traceid: Option<String>,
    #[serde(rename = "sceneId")]
    pub scene_id: Option<String>,
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
    #[serde(rename = "userUserId")]
    pub user_user_id: Option<String>,
    pub language: Option<String>,
    pub refer: Option<String>,
}

pub struct Registration {
    pub endpoint: String,
    pub body: Value,
    pub response: HttpResponse,
    pub info: Option<WafRequestInfo>,
    pub mailbox: Option<Mailbox>,
}

pub fn build_signup_body(template: &Value, account: &Account) -> Value {
    let profile = std::env::var("REGISTER_PROFILE_IMAGE_URL")
        .ok()
        .or_else(|| {
            template
                .get("profile_image_url")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let module = std::env::var("REGISTER_MODULE")
        .ok()
        .or_else(|| {
            template
                .get("module")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "chat".into());
    let mut body = Map::new();
    body.insert("name".into(), json!(account.name));
    body.insert("email".into(), json!(account.email));
    body.insert("password".into(), json!(password_hash(&account.password)));
    body.insert("agree".into(), json!(true));
    body.insert("profile_image_url".into(), json!(profile));
    body.insert("oauth_sub".into(), json!(""));
    body.insert("oauth_token".into(), json!(""));
    body.insert("module".into(), json!(module));
    Value::Object(body)
}

async fn post_signup(
    transport: &Arc<dyn HttpTransport>,
    url: &str,
    body: &Value,
    query: &[(&str, Option<&str>)],
) -> Result<HttpResponse> {
    let mut target = Url::parse(url)?;
    {
        let mut pairs = target.query_pairs_mut();
        for (key, value) in query {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                pairs.append_pair(key, value);
            }
        }
    }
    let mut headers = browser_headers();
    headers.insert("content-type".into(), "application/json".into());
    headers.insert("x-request-id".into(), uuid::Uuid::new_v4().to_string());
    headers.insert("bx-v".into(), "2.5.36".into());
    if let Ok(cookie) = std::env::var("REGISTER_COOKIE") {
        headers.insert("cookie".into(), cookie);
    }
    let target_url = target.to_string();
    let request = HttpRequest {
        method: "POST".into(),
        url: target_url,
        headers,
        body: Some(serde_json::to_vec(body)?),
        timeout: std::time::Duration::from_secs(30),
    };
    transport.execute(request).await
}

pub fn extract_waf_request_info(html: &str) -> Result<Option<WafRequestInfo>> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(
            r#"(?is)<textarea[^>]+id="renderData"[^>]*>var requestInfo = ([\s\S]*?)</textarea>"#,
        )
        .expect("valid WAF renderData regex")
    });
    let Some(captures) = pattern.captures(html) else {
        return Ok(None);
    };
    let source = captures[1].trim().trim_end_matches(';').trim();
    Ok(Some(serde_json::from_str(source)?))
}

pub async fn begin_registration(
    transport: Arc<dyn HttpTransport>,
    fixtures: &CaptureFixtures,
    account: Account,
    mailbox: Option<Mailbox>,
) -> Result<Registration> {
    let endpoint = fixtures
        .signup
        .get("url")
        .and_then(Value::as_str)
        .context("signup request URL is missing")?
        .to_string();
    let template: Value = serde_json::from_str(
        fixtures
            .signup
            .get("postData")
            .and_then(Value::as_str)
            .context("signup request body is missing")?,
    )?;
    let body = build_signup_body(&template, &account);
    let response = post_signup(&transport, &endpoint, &body, &[]).await?;
    let info = extract_waf_request_info(&response.text())?;
    if info.is_none()
        && response
            .header("content-type")
            .is_some_and(|value| value.contains("application/json"))
    {
        return Ok(Registration {
            endpoint,
            body,
            response,
            info: None,
            mailbox,
        });
    }
    let info = info.context("signup response did not contain a usable WAF requestInfo")?;
    if info.token.as_deref().unwrap_or_default().is_empty()
        || info.traceid.as_deref().unwrap_or_default().is_empty()
        || info.scene_id.as_deref().unwrap_or_default().is_empty()
    {
        bail!("signup response did not contain a usable WAF requestInfo");
    }
    Ok(Registration {
        endpoint,
        body,
        response: response.clone(),
        info: Some(info),
        mailbox,
    })
}

pub async fn complete_registration(
    transport: &Arc<dyn HttpTransport>,
    registration: &Registration,
    asig: &str,
) -> Result<HttpResponse> {
    let Some(info) = &registration.info else {
        return Ok(registration.response.clone());
    };
    if asig.is_empty() {
        bail!("VerifyCaptchaV2 did not return a certifyId for u_asig");
    }
    post_signup(
        transport,
        &registration.endpoint,
        &registration.body,
        &[
            ("u_atoken", info.token.as_deref()),
            ("u_asig", Some(asig)),
            ("u_aref", info.refer.as_deref()),
        ],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signup_body_hashes_password_and_preserves_template_defaults() {
        let account = Account {
            name: "synthetic".into(),
            email: "synthetic@example.test".into(),
            password: "correct horse battery staple".into(),
        };
        let body = build_signup_body(
            &json!({"profile_image_url":"https://example.test/avatar.png","module":"chat"}),
            &account,
        );
        assert_eq!(
            body["password"],
            "c4bbcb1fbec99d65bf59d85c8cb62ee2db963f0fe106f483d9afa73bd4e39a8a"
        );
        assert_eq!(body["agree"], true);
        assert_eq!(body["oauth_sub"], "");
        assert_eq!(body["module"], "chat");
    }

    #[test]
    fn parses_waf_render_data_without_executing_html() {
        let html = r#"<textarea id="renderData">var requestInfo = {"token":"t","traceid":"tr","sceneId":"s","userId":"u","userUserId":"uu","language":"cn","refer":"r"};</textarea>"#;
        let info = extract_waf_request_info(html).unwrap().unwrap();
        assert_eq!(info.scene_id.as_deref(), Some("s"));
        assert_eq!(info.traceid.as_deref(), Some("tr"));
    }

    #[test]
    fn ignores_unrelated_textareas() {
        assert!(
            extract_waf_request_info("<textarea id=\"other\">{}</textarea>")
                .unwrap()
                .is_none()
        );
    }
}
