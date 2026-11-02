use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

const DEFAULT_IV_HEX: &str = "d35db7e39ebbf3d001083105";
const DEFAULT_ACCESS_SEC: &str = "FqJB6iRNVYdEGpwb";
const SLOT_NAMES: [&str; 5] = ["REQ", "RES", "UPLOAD", "PREID", "FLAG"];
const DEFAULT_WEB_AES_KEYS: [&str; 5] = [
    "8KmHIQsc5+LZJA7uYex3WaHdkjgCtS6epbG/bc9xss0=",
    "9NhnQQ+LRrKCkAuxwZaUWGBtSzaFtFlNb/ksJCrCgrM=",
    "+fR9tYzlKFr07pEbumd7+KnO3xLOkphCS+qKUbJiMfA=",
    "xLLw/t15vkI7QQBX/1scBbcb9fKx+ymxF0tJ3ds42B0=",
    "k+1RW0cz3iDi2RAbC/c3QKzTPiVwNmNO1910DXe6Gas=",
];

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeKeys {
    #[serde(rename = "REQ")]
    pub req: String,
    #[serde(rename = "RES")]
    pub res: String,
    #[serde(rename = "UPLOAD")]
    pub upload: String,
    #[serde(rename = "PREID")]
    pub preid: String,
    #[serde(rename = "FLAG")]
    pub flag: String,
}

impl RuntimeKeys {
    pub fn get(&self, name: &str) -> Option<&str> {
        match name {
            "REQ" => Some(&self.req),
            "RES" => Some(&self.res),
            "UPLOAD" => Some(&self.upload),
            "PREID" => Some(&self.preid),
            "FLAG" => Some(&self.flag),
            _ => None,
        }
    }

    fn from_slice(values: &[String]) -> Self {
        Self {
            req: values.first().cloned().unwrap_or_default(),
            res: values.get(1).cloned().unwrap_or_default(),
            upload: values.get(2).cloned().unwrap_or_default(),
            preid: values.get(3).cloned().unwrap_or_default(),
            flag: values.get(4).cloned().unwrap_or_default(),
        }
    }

    fn as_array(&self) -> [&str; 5] {
        [&self.req, &self.res, &self.upload, &self.preid, &self.flag]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyStore {
    pub access_sec: String,
    pub web_aes_keys: RuntimeKeys,
}

impl Default for KeyStore {
    fn default() -> Self {
        Self {
            access_sec: DEFAULT_ACCESS_SEC.into(),
            web_aes_keys: RuntimeKeys::from_slice(
                &DEFAULT_WEB_AES_KEYS
                    .iter()
                    .map(|key| (*key).into())
                    .collect::<Vec<String>>(),
            ),
        }
    }
}

impl KeyStore {
    pub fn from_capture(value: &Value) -> Option<Self> {
        if let Some(access_sec) = value.get("accessSec").and_then(Value::as_str) {
            let captured = value.get("webAesKeys")?.as_array()?;
            let mut merged = KeyStore {
                access_sec: access_sec.to_string(),
                ..KeyStore::default()
            };
            let mut values = merged.web_aes_keys.as_array().map(str::to_string);
            for (index, entry) in captured.iter().take(5).enumerate() {
                if let Some(key) = entry.as_str() {
                    values[index] = key.to_string();
                }
            }
            merged.web_aes_keys = RuntimeKeys::from_slice(&values);
            return merged.validated().ok();
        }

        let entries = value.as_array()?;
        let mut counts = BTreeMap::<String, usize>::new();
        for entry in entries {
            if entry.get("fn").and_then(Value::as_str) != Some("rM") {
                continue;
            }
            if let Some(key) = entry.get("key").and_then(Value::as_str)
                && key.chars().count() == 16
            {
                *counts.entry(key.to_string()).or_default() += 1;
            }
        }
        let (key, count) = counts.into_iter().max_by_key(|(_, count)| *count)?;
        if count < 2 {
            return None;
        }
        let candidate = KeyStore {
            access_sec: key,
            ..KeyStore::default()
        };
        candidate.validated().ok()
    }

    fn validated(self) -> Result<Self> {
        let decoded = aes_decrypt_single(&self.web_aes_keys.res, &self.access_sec)?;
        if decoded.len() != 16 {
            bail!("runtime-extracted ACCESS_SEC failed validation");
        }
        Ok(self)
    }

    pub fn runtime_keys(&self) -> Result<RuntimeKeys> {
        let mut decrypted = Vec::new();
        for (name, encrypted) in SLOT_NAMES.iter().zip(self.web_aes_keys.as_array()) {
            let value = aes_decrypt_single(encrypted, &self.access_sec)
                .with_context(|| format!("failed to decrypt WEB_AES_KEY.{name}"))?;
            decrypted.push(value);
        }
        Ok(RuntimeKeys::from_slice(&decrypted))
    }
}

fn iv_final() -> Result<[u8; 16]> {
    let raw = hex::decode(DEFAULT_IV_HEX)?;
    let encoded = BASE64.encode(raw);
    encoded
        .as_bytes()
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid AES IV derivation"))
}

pub fn aes_decrypt_single(cipher_b64: &str, key: &str) -> Result<String> {
    let iv = iv_final()?;
    let ciphertext = BASE64.decode(cipher_b64)?;
    let plaintext = Aes128CbcDec::new_from_slices(key.as_bytes(), &iv)
        .context("AES-128-CBC key or IV has invalid length")?
        .decrypt_padded_vec_mut::<Pkcs7>(&ciphertext)
        .map_err(|_| anyhow::anyhow!("AES-128-CBC decrypt failed"))?;
    String::from_utf8(plaintext).context("AES plaintext is not UTF-8")
}

pub fn aes_encrypt(plaintext: &str, key: &str) -> Result<String> {
    let iv = iv_final()?;
    let encrypted = Aes128CbcEnc::new_from_slices(key.as_bytes(), &iv)
        .context("AES-128-CBC key or IV has invalid length")?
        .encrypt_padded_vec_mut::<Pkcs7>(plaintext.as_bytes());
    Ok(BASE64.encode(encrypted))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceConfig {
    #[serde(rename = "sessionKey")]
    pub session_key: String,
    #[serde(rename = "sessionKeyB64")]
    pub session_key_b64: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "sessionHdr")]
    pub session_hdr: String,
    #[serde(rename = "feilinVer")]
    pub feilin_ver: String,
    pub timestamp: String,
    pub ip: String,
    #[serde(rename = "runtimeKeys")]
    pub runtime_keys: RuntimeKeys,
}

pub fn decrypt_device_config(device_config_b64: &str, keys: &KeyStore) -> Result<DeviceConfig> {
    let runtime_keys = keys.runtime_keys()?;
    let decrypted = aes_decrypt_single(device_config_b64, &runtime_keys.res)?;
    let parts = decrypted.split('#').collect::<Vec<_>>();
    let field = |index: usize| parts.get(index).copied().unwrap_or_default().to_string();
    let session_key_b64 = field(0);
    let session_key = if session_key_b64.is_empty() {
        String::new()
    } else {
        String::from_utf8(BASE64.decode(&session_key_b64)?)?
    };
    let session_hdr = field(2);
    Ok(DeviceConfig {
        session_key,
        session_key_b64,
        session_id: session_hdr
            .split("-h-")
            .next()
            .unwrap_or_default()
            .to_string(),
        session_hdr,
        feilin_ver: field(3),
        timestamp: field(7),
        ip: field(8),
        runtime_keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_legacy_aes_cbc_vector() {
        let plaintext = "session#MQ==#id-h-rest#feilin####123#127.0.0.1#flag";
        let encrypted = aes_encrypt(plaintext, "0123456789abcdef").unwrap();
        assert_eq!(
            encrypted,
            "2R/dkWyT4xqZDx6S9VPlOkrvdcovi3d+Fh6KpkzRpuLnkg8nPoGmJr171zK/utW0pIxwA1TVfeDdls0UYvAElQ=="
        );
        assert_eq!(
            aes_decrypt_single(&encrypted, "0123456789abcdef").unwrap(),
            plaintext
        );
    }

    #[test]
    fn rejects_invalid_ciphertext_without_exposing_key() {
        let error = aes_decrypt_single("not-base64", "0123456789abcdef").unwrap_err();
        assert!(!error.to_string().contains("0123456789abcdef"));
    }

    #[test]
    fn parses_device_config_fields_with_synthetic_keys() {
        let access = "0123456789abcdef";
        let res = "fedcba9876543210";
        let keys = KeyStore {
            access_sec: access.into(),
            web_aes_keys: RuntimeKeys {
                req: aes_encrypt("0123456789abcdef", access).unwrap(),
                res: aes_encrypt(res, access).unwrap(),
                upload: aes_encrypt("0123456789abcdef", access).unwrap(),
                preid: aes_encrypt("0123456789abcdef", access).unwrap(),
                flag: aes_encrypt("0123456789abcdef", access).unwrap(),
            },
        };
        let session_b64 = BASE64.encode("session-key");
        let plaintext = format!("{session_b64}#MQ==#id-h-rest#feilin####123#192.0.2.1#flag");
        let encrypted = aes_encrypt(&plaintext, res).unwrap();
        let parsed = decrypt_device_config(&encrypted, &keys).unwrap();
        assert_eq!(parsed.session_key, "session-key");
        assert_eq!(parsed.session_id, "id");
        assert_eq!(parsed.feilin_ver, "feilin");
        assert_eq!(parsed.timestamp, "123");
        assert_eq!(parsed.ip, "192.0.2.1");
    }
}
