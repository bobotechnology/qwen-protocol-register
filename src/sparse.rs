use crate::device::DeviceConfig;
use anyhow::{Result, bail};
use serde_json::{Map, Value};

pub fn build_sparse_object(
    source: &str,
    init: &Value,
    device: &DeviceConfig,
    now_ms: i64,
) -> Result<Value> {
    let mut fields: Vec<&str> = source.split('#').collect();
    if fields.len() != 133 {
        bail!("expected 133 fields, got {}", fields.len());
    }
    let certify_id = init
        .get("CertifyId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("InitCaptchaV2 response is missing CertifyId"))?;
    let mut owned = fields
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    owned[72] = (now_ms + 500).to_string();
    owned[74] = (now_ms + 1000).to_string();
    owned[77] = certify_id.to_string();
    owned[87] = now_ms.to_string();
    owned[42] = device.ip.clone();
    fields.clear();

    let object = owned
        .into_iter()
        .enumerate()
        .map(|(index, value)| (format!("k{index}"), Value::String(value)))
        .collect::<Map<_, _>>();
    Ok(Value::Object(object))
}

pub fn extract_captcha_param(probe: &Value) -> Result<Value> {
    let candidates = probe
        .get("calls")
        .and_then(|calls| calls.get("jsonCalls"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("result").and_then(Value::as_str))
        .filter_map(|result| serde_json::from_str::<Value>(result).ok())
        .filter(|value| {
            ["sceneId", "certifyId", "deviceToken", "data"]
                .iter()
                .all(|key| {
                    value.get(*key).is_some_and(|field| match field {
                        Value::Null => false,
                        Value::String(text) => !text.is_empty(),
                        Value::Bool(flag) => *flag,
                        _ => true,
                    })
                })
        })
        .collect::<Vec<_>>();
    candidates
        .last()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("generated CaptchaVerifyParam not found"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn adapts_browser_sparse_positions() {
        let source = vec![""; 133].join("#");
        let init = json!({"CertifyId":"challenge"});
        let device = DeviceConfig {
            session_key: String::new(),
            session_key_b64: String::new(),
            session_id: String::new(),
            session_hdr: String::new(),
            feilin_ver: String::new(),
            timestamp: String::new(),
            ip: "192.0.2.1".into(),
            runtime_keys: Default::default(),
        };
        let value = build_sparse_object(&source, &init, &device, 1000).unwrap();
        assert_eq!(value["k42"], "192.0.2.1");
        assert_eq!(value["k72"], "1500");
        assert_eq!(value["k74"], "2000");
        assert_eq!(value["k77"], "challenge");
        assert_eq!(value["k87"], "1000");
        assert_eq!(value.as_object().unwrap().len(), 133);
    }

    #[test]
    fn sparse_builder_preserves_trailing_newline_like_legacy() {
        let source = format!("{}\n", vec![""; 132].join("#"));
        let error = build_sparse_object(
            &source,
            &json!({"CertifyId":"x"}),
            &DeviceConfig::default(),
            0,
        )
        .unwrap_err();
        assert!(error.to_string().contains("133"));
    }

    #[test]
    fn extracts_last_complete_json_call() {
        let probe = json!({"calls":{"jsonCalls":[
            {"result":"{\"sceneId\":\"s\",\"certifyId\":\"a\",\"deviceToken\":\"t\",\"data\":\"first\"}"},
            {"result":"not json"},
            {"result":"{\"sceneId\":\"s\",\"certifyId\":\"b\",\"deviceToken\":\"t\",\"data\":\"last\"}"}
        ]}});
        let value = extract_captcha_param(&probe).unwrap();
        assert_eq!(value["certifyId"], "b");
        assert_eq!(value["data"], "last");
    }
}
