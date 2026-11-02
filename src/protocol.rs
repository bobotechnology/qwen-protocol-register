use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::Utc;
use hmac::{Hmac, Mac};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const COMPONENT_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

pub fn percent(value: impl AsRef<str>) -> String {
    utf8_percent_encode(value.as_ref(), COMPONENT_SET)
        .to_string()
        .replace('*', "%2A")
}

pub fn timestamp() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn nonce() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub canonical: String,
    pub string_to_sign: String,
    pub signature: String,
}

pub fn sign_v2(params: &BTreeMap<String, String>, secret: &str) -> Signature {
    let canonical = params
        .iter()
        .filter(|(key, _)| key.as_str() != "Signature")
        .map(|(key, value)| format!("{}={}", percent(key), percent(value)))
        .collect::<Vec<_>>()
        .join("&");
    let string_to_sign = format!("POST&%2F&{}", percent(&canonical));
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(format!("{secret}&").as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(string_to_sign.as_bytes());
    Signature {
        canonical,
        string_to_sign,
        signature: BASE64.encode(mac.finalize().into_bytes()),
    }
}

pub fn form<'a>(params: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in params {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

pub fn password_hash(password: &str) -> String {
    hex::encode(Sha256::digest(password.as_bytes()))
}

pub fn decode_request_info(value: &str) -> anyhow::Result<String> {
    Ok(String::from_utf8(BASE64.decode(value)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BTreeMap<String, String> {
        [
            ("Action", "InitCaptchaV2"),
            ("Format", "JSON"),
            ("SignatureMethod", "HMAC-SHA1"),
            ("SignatureNonce", "00000000-0000-4000-8000-000000000000"),
            ("SignatureVersion", "1.0"),
            ("Timestamp", "2026-07-16T00:00:00Z"),
            ("Version", "2023-03-05"),
            ("SceneId", "scene ~*"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
    }

    #[test]
    fn matches_legacy_signature_vector() {
        let signed = sign_v2(&sample(), "test-secret");
        assert_eq!(
            signed.canonical,
            "Action=InitCaptchaV2&Format=JSON&SceneId=scene%20~%2A&SignatureMethod=HMAC-SHA1&SignatureNonce=00000000-0000-4000-8000-000000000000&SignatureVersion=1.0&Timestamp=2026-07-16T00%3A00%3A00Z&Version=2023-03-05"
        );
        assert_eq!(
            signed.string_to_sign,
            "POST&%2F&Action%3DInitCaptchaV2%26Format%3DJSON%26SceneId%3Dscene%2520~%252A%26SignatureMethod%3DHMAC-SHA1%26SignatureNonce%3D00000000-0000-4000-8000-000000000000%26SignatureVersion%3D1.0%26Timestamp%3D2026-07-16T00%253A00%253A00Z%26Version%3D2023-03-05"
        );
        assert_eq!(signed.signature, "3teAbL/0sKBI8BgcGxJzcmMwXwQ=");
        assert_eq!(percent("a b+~*"), "a%20b%2B~%2A");
    }

    #[test]
    fn matches_legacy_form_and_password_vectors() {
        assert_eq!(form([("a", "x y"), ("b", "~*")]), "a=x+y&b=%7E*");
        assert_eq!(
            password_hash("correct horse battery staple"),
            "c4bbcb1fbec99d65bf59d85c8cb62ee2db963f0fe106f483d9afa73bd4e39a8a"
        );
    }

    #[test]
    fn signature_ignores_previous_signature() {
        let mut params = sample();
        params.insert("Signature".into(), "stale".into());
        assert_eq!(
            sign_v2(&params, "test-secret"),
            sign_v2(&sample(), "test-secret")
        );
    }
}
