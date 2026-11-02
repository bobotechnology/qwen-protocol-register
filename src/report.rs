use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskResult {
    pub email: Option<String>,
    pub name: Option<String>,
    pub password: Option<String>,
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
    pub token: Option<String>,
    #[serde(rename = "tokenType")]
    pub token_type: Option<String>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<String>,
    pub role: Option<String>,
    pub permissions: Option<Value>,
    pub proxy: Option<String>,
    #[serde(rename = "verifyCode")]
    pub verify_code: Option<String>,
    #[serde(rename = "verifyResult")]
    pub verify_result: bool,
    #[serde(rename = "registrationStatus")]
    pub registration_status: Option<u16>,
    pub error: Option<String>,
}

impl TaskResult {
    pub fn success(&self) -> bool {
        self.error.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchReport {
    pub timestamp: String,
    pub elapsed: String,
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub results: Vec<TaskResult>,
}

impl BatchReport {
    pub fn new(timestamp: String, elapsed_seconds: f64, results: Vec<TaskResult>) -> Self {
        let success = results.iter().filter(|result| result.success()).count();
        Self {
            timestamp,
            elapsed: format!("{elapsed_seconds:.1}s"),
            total: results.len(),
            success,
            failed: results.len() - success,
            results,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_batch_json_shape() {
        let report = BatchReport::new(
            "2026-07-19T13:44:10.000Z".into(),
            51.26,
            vec![TaskResult {
                email: Some("synthetic@example.test".into()),
                verify_code: Some("T001".into()),
                verify_result: true,
                registration_status: Some(200),
                ..TaskResult::default()
            }],
        );
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["elapsed"], "51.3s");
        assert_eq!(value["success"], 1);
        assert_eq!(value["results"][0]["userId"], Value::Null);
        assert_eq!(value["results"][0]["verifyCode"], "T001");
        assert_eq!(value["results"][0]["error"], Value::Null);
    }
}
