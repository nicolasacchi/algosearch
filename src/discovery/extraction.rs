use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Extract a brace-balanced substring starting from the first `{` in `text`.
/// Returns the content between (and including) the outermost braces.
/// Handles string literals (skips braces inside quotes).
pub fn extract_brace_balanced(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0;
    let mut in_string: Option<u8> = None; // b'\'' or b'"' or b'`'
    let mut escape = false;

    for i in start..bytes.len() {
        let ch = bytes[i];

        if escape {
            escape = false;
            continue;
        }

        if ch == b'\\' {
            escape = true;
            continue;
        }

        if let Some(quote) = in_string {
            if ch == quote {
                in_string = None;
            }
            continue;
        }

        match ch {
            b'\'' | b'"' | b'`' => in_string = Some(ch),
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }

    None
}

/// Extract key-value pairs from a JS object literal.
/// Handles both quoted and unquoted keys, and single/double quoted values.
static KV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"['"]?(\w+)['"]?\s*[:=]\s*['"`]([^'"`]+)['"`]"#).unwrap()
});

pub fn extract_kv_from_js_object(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for cap in KV_RE.captures_iter(text) {
        if let (Some(key), Some(value)) = (cap.get(1), cap.get(2)) {
            map.insert(key.as_str().to_string(), value.as_str().to_string());
        }
    }
    map
}

/// Traverse a serde_json::Value using JSON pointer-like paths.
/// Tries multiple paths, returns the first hit.
#[allow(dead_code)]
pub fn extract_from_json_value(
    value: &serde_json::Value,
    paths: &[&str],
) -> Option<String> {
    for path in paths {
        if let Some(v) = value.pointer(path) {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brace_balanced_simple() {
        let text = r#"{ appId: 'BH4D9OD16A', apiKey: 'abc123' }"#;
        let result = extract_brace_balanced(text);
        assert_eq!(result, Some(text));
    }

    #[test]
    fn test_brace_balanced_nested() {
        let text = r#"{ outer: { inner: 'value' } }"#;
        let result = extract_brace_balanced(text);
        assert_eq!(result, Some(text));
    }

    #[test]
    fn test_brace_balanced_with_string_braces() {
        let text = r#"{ key: 'value with { braces }' }"#;
        let result = extract_brace_balanced(text);
        assert_eq!(result, Some(text));
    }

    #[test]
    fn test_brace_balanced_with_escaped_quotes() {
        let text = r#"{ key: 'it\'s ok' }"#;
        let result = extract_brace_balanced(text);
        assert_eq!(result, Some(text));
    }

    #[test]
    fn test_brace_balanced_no_opening() {
        let text = "no braces here";
        let result = extract_brace_balanced(text);
        assert_eq!(result, None);
    }

    #[test]
    fn test_brace_balanced_unclosed() {
        let text = r#"{ key: 'value'"#;
        let result = extract_brace_balanced(text);
        assert_eq!(result, None);
    }

    #[test]
    fn test_brace_balanced_prefix() {
        let text = r#"docsearch({ appId: 'BH4D9OD16A' })"#;
        let result = extract_brace_balanced(text);
        assert_eq!(result, Some("{ appId: 'BH4D9OD16A' }"));
    }

    #[test]
    fn test_kv_extraction_single_quotes() {
        let text = r#"{ appId: 'BH4D9OD16A', apiKey: 'd9aa2d7a17b51cc4b053e1ee0bd1d4b5' }"#;
        let kv = extract_kv_from_js_object(text);
        assert_eq!(kv.get("appId"), Some(&"BH4D9OD16A".to_string()));
        assert_eq!(
            kv.get("apiKey"),
            Some(&"d9aa2d7a17b51cc4b053e1ee0bd1d4b5".to_string())
        );
    }

    #[test]
    fn test_kv_extraction_double_quotes() {
        let text = r#"{ "appId": "BH4D9OD16A", "apiKey": "abc123" }"#;
        let kv = extract_kv_from_js_object(text);
        assert_eq!(kv.get("appId"), Some(&"BH4D9OD16A".to_string()));
    }

    #[test]
    fn test_kv_extraction_unquoted_keys() {
        let text = r#"{ appId: "BH4D9OD16A" }"#;
        let kv = extract_kv_from_js_object(text);
        assert_eq!(kv.get("appId"), Some(&"BH4D9OD16A".to_string()));
    }

    #[test]
    fn test_json_value_extraction() {
        let json: serde_json::Value = serde_json::json!({
            "props": {
                "algolia": {
                    "appId": "BH4D9OD16A"
                }
            }
        });
        let result = extract_from_json_value(&json, &["/props/algolia/appId"]);
        assert_eq!(result, Some("BH4D9OD16A".to_string()));
    }

    #[test]
    fn test_json_value_extraction_miss() {
        let json: serde_json::Value = serde_json::json!({"other": "data"});
        let result = extract_from_json_value(&json, &["/props/algolia/appId"]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_json_value_extraction_multiple_paths() {
        let json: serde_json::Value = serde_json::json!({"config": {"app_id": "BH4D9OD16A"}});
        let result = extract_from_json_value(&json, &["/algolia/appId", "/config/app_id"]);
        assert_eq!(result, Some("BH4D9OD16A".to_string()));
    }
}
