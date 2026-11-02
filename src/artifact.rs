use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)
            .with_context(|| format!("create artifact directory {}", root.display()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, name: impl AsRef<Path>) -> PathBuf {
        self.root.join(name)
    }

    pub fn exists(&self, name: impl AsRef<Path>) -> bool {
        self.path(name).exists()
    }

    pub fn write_bytes(&self, name: impl AsRef<Path>, bytes: &[u8]) -> Result<PathBuf> {
        let path = self.path(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
        Ok(path)
    }

    pub fn write_json<T: Serialize>(&self, name: impl AsRef<Path>, value: &T) -> Result<PathBuf> {
        let mut bytes = serde_json::to_vec_pretty(value)?;
        bytes.push(b'\n');
        self.write_bytes(name, &bytes)
    }

    pub fn read_json<T: DeserializeOwned>(&self, name: impl AsRef<Path>) -> Result<T> {
        let path = self.path(name);
        let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
    }

    pub fn read_optional_json<T: DeserializeOwned>(
        &self,
        name: impl AsRef<Path>,
    ) -> (Option<T>, Option<String>) {
        let path = self.path(name);
        if !path.exists() {
            return (None, None);
        }
        match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(value) => (Some(value), None),
                Err(error) => (
                    None,
                    Some(format!(
                        "{}: {error}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )),
                ),
            },
            Err(error) => (None, Some(format!("{}: {error}", path.display()))),
        }
    }

    pub fn atomic_write_json<T: Serialize>(
        &self,
        name: impl AsRef<Path>,
        value: &T,
    ) -> Result<PathBuf> {
        let path = self.path(name);
        let temporary = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
        let mut bytes = serde_json::to_vec_pretty(value)?;
        bytes.push(b'\n');
        self.write_bytes(&temporary, &bytes)?;
        std::fs::rename(&temporary, &path)
            .with_context(|| format!("replace {}", path.display()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn writes_pretty_json_with_trailing_newline() {
        let directory = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(directory.path()).unwrap();
        store
            .write_json("result.json", &json!({"ok":true}))
            .unwrap();
        let text = std::fs::read_to_string(store.path("result.json")).unwrap();
        assert_eq!(text, "{\n  \"ok\": true\n}\n");
    }
}
