use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, LOCATION, SET_COOKIE};
use std::collections::BTreeMap;
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: "GET".into(),
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: BTreeMap<String, String>,
    pub set_cookies: Vec<String>,
    pub body: Vec<u8>,
    pub url: String,
}

impl HttpResponse {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[async_trait]
pub trait HttpTransport: Send + Sync {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse>;
}

#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new(proxy: Option<&str>) -> Result<Self> {
        let mut builder = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none());
        if let Some(proxy) = proxy.filter(|value| !value.trim().is_empty()) {
            let parsed = Url::parse(proxy).context("invalid proxy URL")?;
            match parsed.scheme() {
                "http" | "https" | "socks" | "socks4" | "socks4a" | "socks5" | "socks5h" => {}
                scheme => bail!("unsupported proxy protocol: {scheme}:"),
            }
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
        Ok(Self {
            client: builder.build()?,
        })
    }
}

fn convert_headers(headers: BTreeMap<String, String>, body: Option<&Vec<u8>>) -> Result<HeaderMap> {
    let mut output = HeaderMap::new();
    let has_length = headers
        .keys()
        .any(|name| name.eq_ignore_ascii_case("content-length"));
    for (name, value) in headers {
        output.insert(
            HeaderName::from_bytes(name.as_bytes())?,
            HeaderValue::from_str(&value)?,
        );
    }
    if let Some(body) = body.filter(|_| !has_length) {
        output.insert(
            reqwest::header::CONTENT_LENGTH,
            HeaderValue::from_str(&body.len().to_string())?,
        );
    }
    Ok(output)
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
        // Transport-level retry: only fires on send/body errors (timeout, TLS
        // reset, connection drop). Requests with a JSON body are idempotent on
        // our targets (OTP request/verify, Guerrilla Mail, static downloads).
        const MAX_ATTEMPTS: u32 = 3;
        let mut last_error: Option<anyhow::Error> = None;
        for attempt in 1..=MAX_ATTEMPTS {
            match self.execute_once(&request).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("HTTP request failed")))
    }
}

impl ReqwestTransport {
    async fn execute_once(&self, request: &HttpRequest) -> Result<HttpResponse> {
        let mut target = Url::parse(&request.url).with_context(|| "invalid request URL")?;
        if target.scheme() != "http" && target.scheme() != "https" {
            bail!("unsupported request protocol: {}:", target.scheme());
        }
        for redirect in 0..=5 {
            let method = reqwest::Method::from_bytes(request.method.as_bytes())?;
            let mut builder = self
                .client
                .request(method, target.clone())
                .headers(convert_headers(
                    request.headers.clone(),
                    request.body.as_ref(),
                )?)
                .timeout(request.timeout);
            if let Some(body) = &request.body {
                builder = builder.body(body.clone());
            }
            let response = builder.send().await.context("HTTP request failed")?;
            let status = response.status();
            if status.is_redirection()
                && let Some(location) = response.headers().get(LOCATION)
            {
                if redirect == 5 {
                    bail!("too many HTTP redirects");
                }
                target = target.join(location.to_str()?)?;
                continue;
            }
            let status_text = status.canonical_reason().unwrap_or_default().to_string();
            let mut headers = BTreeMap::new();
            for name in response.headers().keys() {
                let values = response
                    .headers()
                    .get_all(name)
                    .iter()
                    .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
                    .collect::<Vec<_>>();
                headers.insert(name.as_str().to_ascii_lowercase(), values.join(", "));
            }
            let set_cookies = response
                .headers()
                .get_all(SET_COOKIE)
                .iter()
                .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
                .collect();
            let body = response.bytes().await?.to_vec();
            return Ok(HttpResponse {
                status: status.as_u16(),
                status_text,
                headers,
                set_cookies,
                body,
                url: target.to_string(),
            });
        }
        bail!("too many HTTP redirects")
    }
}

pub fn redact_proxy(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let Ok(mut parsed) = Url::parse(value) else {
        return Some("[invalid proxy]".into());
    };
    if !parsed.username().is_empty() {
        let _ = parsed.set_username("***");
    }
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("***"));
    }
    Some(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_credentials_are_redacted() {
        let output = redact_proxy(Some("socks5://user:pass@localhost:1080")).unwrap();
        assert!(!output.contains("user"));
        assert!(!output.contains("pass"));
        assert!(output.contains("***"));
        assert_eq!(
            redact_proxy(Some("not a url")).as_deref(),
            Some("[invalid proxy]")
        );
    }

    #[test]
    fn request_content_length_is_implicit() {
        let headers = convert_headers(BTreeMap::new(), Some(&vec![1, 2, 3])).unwrap();
        assert_eq!(headers.get("content-length").unwrap(), "3");
    }
}
