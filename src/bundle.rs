use crate::artifact::ArtifactStore;
use crate::instrument::{expose_dynamic_runtime, expose_feilin_runtime};
use crate::quickjs::validate_javascript;
use crate::transport::{HttpRequest, HttpTransport};
use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;

const DYNAMIC_BASE_URL: &str = "https://g.alicdn.com/captcha-frontend/dynamicJS/";
const FEILIN_BASE_URL: &str = "https://g.alicdn.com/captcha-frontend/FeiLin/";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleInfo {
    #[serde(rename = "staticPath", skip_serializing_if = "Option::is_none")]
    pub static_path: Option<String>,
    #[serde(rename = "resourcePath", skip_serializing_if = "Option::is_none")]
    pub resource_path: Option<String>,
    pub url: String,
    #[serde(rename = "rawPath")]
    pub raw_path: String,
    #[serde(rename = "exposedPath")]
    pub exposed_path: String,
    pub sha256: String,
    pub bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrumented: Option<bool>,
}

async fn download(transport: &Arc<dyn HttpTransport>, url: &str) -> Result<Vec<u8>> {
    let request = HttpRequest::get(url).with_header("user-agent", "Mozilla/5.0");
    let response = transport.execute(request).await?;
    if response.status != 200 {
        bail!("dynamicJS download failed: HTTP {}", response.status);
    }
    Ok(response.body)
}

fn validate_static_path(path: &str) -> Result<()> {
    let pattern = Regex::new(r"^\d+\.\d+\.\d+/pe\.[A-Za-z0-9._-]+$")?;
    if !pattern.is_match(path) {
        bail!("unexpected InitCaptchaV2.StaticPath");
    }
    Ok(())
}

fn validate_feilin_path(path: &str) -> Result<()> {
    let pattern = Regex::new(r"^\d+\.\d+\.\d+/feilin\d+\.[a-f0-9]{32,64}$")?;
    if !pattern.is_match(path) {
        bail!("unexpected DeviceConfig FeiLin path");
    }
    Ok(())
}

pub async fn load_dynamic_bundle(
    init: &Value,
    transport: Arc<dyn HttpTransport>,
    store: &ArtifactStore,
) -> Result<BundleInfo> {
    let static_path = init
        .get("StaticPath")
        .and_then(Value::as_str)
        .context("InitCaptchaV2.StaticPath is missing")?;
    validate_static_path(static_path)?;
    let url = format!("{DYNAMIC_BASE_URL}{static_path}.js");
    let raw = download(&transport, &url).await?;
    let stem = static_path.replace('/', "_");
    let raw_path = store.path(format!("dynamic/{stem}.raw.js"));
    let exposed_path = store.path(format!("dynamic/{stem}.exposed.js"));
    store.write_bytes(format!("dynamic/{stem}.raw.js"), &raw)?;
    let source = String::from_utf8(raw.clone()).context("dynamicJS bundle is not UTF-8")?;
    let exposed = expose_dynamic_runtime(&source)?;
    if !exposed.code.contains("window.__DYN_INTERNALS__") {
        bail!("dynamicJS exposure marker missing after instrumentation");
    }
    validate_javascript(&exposed.code)?;
    store.write_bytes(
        format!("dynamic/{stem}.exposed.js"),
        exposed.code.as_bytes(),
    )?;
    store.write_bytes(
        "fresh_dynamic_path.txt",
        exposed_path.to_string_lossy().as_bytes(),
    )?;
    Ok(BundleInfo {
        static_path: Some(static_path.into()),
        resource_path: None,
        url,
        raw_path: raw_path.to_string_lossy().into_owned(),
        exposed_path: exposed_path.to_string_lossy().into_owned(),
        sha256: hex::encode(Sha256::digest(&raw)),
        bytes: raw.len(),
        instrumented: None,
    })
}

pub async fn load_feilin_bundle(
    resource_path: &str,
    transport: Arc<dyn HttpTransport>,
    store: &ArtifactStore,
) -> Result<BundleInfo> {
    validate_feilin_path(resource_path)?;
    let url = format!("{FEILIN_BASE_URL}{resource_path}.js");
    let raw = download(&transport, &url).await?;
    let stem = resource_path.replace('/', "_");
    let raw_path = store.path(format!("dynamic/{stem}.raw.js"));
    let exposed_path = store.path(format!("dynamic/{stem}.exposed.js"));
    store.write_bytes(format!("dynamic/{stem}.raw.js"), &raw)?;
    let source = String::from_utf8(raw.clone()).context("FeiLin bundle is not UTF-8")?;
    let exposed = expose_feilin_runtime(&source)?;
    if !exposed.code.contains("__FEILIN_INSTRUMENTATION__") {
        bail!("FeiLin instrumentation marker missing after instrumentation");
    }
    validate_javascript(&exposed.code)?;
    store.write_bytes(
        format!("dynamic/{stem}.exposed.js"),
        exposed.code.as_bytes(),
    )?;
    Ok(BundleInfo {
        static_path: None,
        resource_path: Some(resource_path.into()),
        url,
        raw_path: raw_path.to_string_lossy().into_owned(),
        exposed_path: exposed_path.to_string_lossy().into_owned(),
        sha256: hex::encode(Sha256::digest(&raw)),
        bytes: raw.len(),
        instrumented: Some(true),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::HttpResponse;
    use async_trait::async_trait;
    use std::collections::BTreeMap;

    struct FixtureTransport {
        body: Vec<u8>,
    }

    #[async_trait]
    impl HttpTransport for FixtureTransport {
        async fn execute(&self, request: HttpRequest) -> Result<HttpResponse> {
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: BTreeMap::new(),
                set_cookies: Vec::new(),
                body: self.body.clone(),
                url: request.url,
            })
        }
    }

    #[tokio::test]
    async fn downloads_and_instruments_dynamic_bundle() {
        let directory = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(directory.path()).unwrap();
        let source = b"function C(){};P.prototype.PowVerifyString=function(){P.reset()};t.AliyunCaptcha=C;t=window,NodeList.prototype.forEach=Array.prototype.forEach,t.AliyunCaptcha=C";
        let info = load_dynamic_bundle(
            &serde_json::json!({"StaticPath":"1.2.3/pe.test"}),
            Arc::new(FixtureTransport {
                body: source.to_vec(),
            }),
            &store,
        )
        .await
        .unwrap();
        assert!(info.exposed_path.ends_with(".exposed.js"));
        assert!(store.path("fresh_dynamic_path.txt").exists());
    }

    #[test]
    fn rejects_path_traversal_in_bundle_paths() {
        assert!(validate_static_path("../secret").is_err());
        assert!(validate_feilin_path("1.2.3/feilin106.not-a-hash").is_err());
    }
}
