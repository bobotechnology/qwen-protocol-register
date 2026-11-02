use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn inspect_capture(text: &str) -> Result<Value, String> {
    let mut types = BTreeMap::<String, usize>::new();
    let mut methods = BTreeMap::<String, usize>::new();
    let mut invalid_lines = Vec::new();
    let mut records = 0usize;

    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => {
                invalid_lines.push(index + 1);
                continue;
            }
        };
        if !value.is_object() {
            invalid_lines.push(index + 1);
            continue;
        }
        records += 1;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let method = value
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        *types.entry(kind.to_string()).or_default() += 1;
        *methods.entry(method.to_string()).or_default() += 1;
    }

    if records == 0 && !invalid_lines.is_empty() {
        return Err("capture contains no valid object records".into());
    }
    Ok(json!({
        "records": records,
        "types": types,
        "methods": methods,
        "invalidLines": invalid_lines,
    }))
}

pub fn inspect_probe(text: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
    let calls = value
        .get("calls")
        .and_then(Value::as_object)
        .ok_or("probe must contain a calls object")?;
    let json_calls = calls.get("jsonCalls").and_then(Value::as_array);
    let count = json_calls.map_or(0, Vec::len);
    let structured_results = json_calls
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry
                .get("result")
                .and_then(Value::as_str)
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .is_some()
        })
        .count();
    Ok(json!({
        "callGroups": calls.len(),
        "jsonCalls": count,
        "structuredResults": structured_results,
    }))
}

pub fn inspect_sparse(text: &str) -> Result<Value, String> {
    let trimmed = text.trim_end_matches(['\r', '\n']);
    let fields: Vec<&str> = trimmed.split('#').collect();
    if fields.len() != 133 {
        return Err(format!("expected 133 fields, got {}", fields.len()));
    }
    Ok(json!({
        "fields": fields.len(),
        "nonEmpty": fields.iter().filter(|s| !s.is_empty()).count(),
        "characters": trimmed.chars().count(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_counts_without_echoing_sensitive_data() {
        let input = r#"{"type":"request","method":"POST","url":"https://example.test/private","headers":{"authorization":"secret"}}"#;
        let output = inspect_capture(input).unwrap().to_string();
        assert!(output.contains("\"records\":1"));
        assert!(!output.contains("secret"));
        assert!(!output.contains("private"));
    }

    #[test]
    fn capture_reports_invalid_lines() {
        let output = inspect_capture("bad\n{}\n").unwrap();
        assert_eq!(output["invalidLines"], json!([1]));
        assert_eq!(output["records"], 1);
    }

    #[test]
    fn probe_summarizes_without_revealing_values() {
        let output =
            inspect_probe(r#"{"calls":{"jsonCalls":[{"result":"{\"token\":\"secret\"}"}]}}"#)
                .unwrap();
        assert_eq!(output["structuredResults"], 1);
        assert!(!output.to_string().contains("secret"));
    }

    #[test]
    fn sparse_validates_field_count() {
        assert_eq!(
            inspect_sparse(&vec![""; 133].join("#")).unwrap()["fields"],
            133
        );
        assert!(inspect_sparse("a#b").is_err());
    }
}
