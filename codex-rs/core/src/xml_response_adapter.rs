//! XML Response Adapter for models like glm-4.6 that use XML-style tags
//! Transforms XML-formatted responses into OpenAI-compatible JSON format

use regex_lite::Regex;
use serde_json::{Value, json};
use tracing::debug;

/// Transforms XML-style model responses into OpenAI-compatible JSON format
pub struct XmlResponseAdapter {
    _model_name: String,
}

impl XmlResponseAdapter {
    pub fn new(model_name: String) -> Self {
        Self {
            _model_name: model_name,
        }
    }

    /// Transform XML-formatted response chunk into OpenAI JSON format
    pub fn transform_chunk(&self, raw_data: &str) -> Option<Value> {
        // First check if this already looks like JSON
        if raw_data.trim().starts_with('{') {
            // If it's already JSON, check if it needs transformation
            if let Ok(json_chunk) = serde_json::from_str::<Value>(raw_data) {
                // Check if there's text content that might contain XML
                if let Some(content) = extract_assistant_content(&json_chunk)
                    && contains_xml_tags(&content)
                {
                    debug!("Found XML in JSON content, transforming");
                    return self.transform_xml_in_json(json_chunk, &content);
                }
                return Some(json_chunk);
            }
        }

        // If it's pure XML or mixed content, parse it
        if contains_xml_tags(raw_data) {
            debug!("Transforming pure XML response");
            return self.parse_xml_response(raw_data);
        }

        None
    }

    /// Transform XML tags found within JSON content
    fn transform_xml_in_json(&self, mut json_chunk: Value, content: &str) -> Option<Value> {
        // Parse the XML content
        let parsed = self.parse_xml_content(content);

        if let Some(parsed) = parsed {
            // Replace the content with the parsed version
            if let Some(choices) = json_chunk
                .get_mut("choices")
                .and_then(|c| c.as_array_mut())
                .and_then(|arr| arr.get_mut(0))
            {
                // Handle different response formats
                if let Some(delta) = choices.get_mut("delta") {
                    // For streaming responses
                    *delta = parsed;
                } else if let Some(message) = choices.get_mut("message") {
                    // For non-streaming responses
                    *message = parsed;
                }
            }
        }

        Some(json_chunk)
    }

    /// Parse pure XML response into OpenAI format
    fn parse_xml_response(&self, xml_content: &str) -> Option<Value> {
        let parsed_delta = self.parse_xml_content(xml_content)?;

        // Wrap in OpenAI response structure
        Some(json!({
            "choices": [{
                "index": 0,
                "delta": parsed_delta,
                "finish_reason": null
            }]
        }))
    }

    /// Parse XML content and extract structured data
    fn parse_xml_content(&self, content: &str) -> Option<Value> {
        let mut delta = json!({});
        let mut has_content = false;
        let mut tool_calls = Vec::new();

        // Extract and handle <think> tags (reasoning)
        let think_re = Regex::new(r"<think>([\s\S]*?)</think>").ok()?;
        if let Some(cap) = think_re.captures(content) {
            let reasoning_text = cap.get(1)?.as_str().trim();
            delta["reasoning"] = json!({
                "text": reasoning_text
            });
        }

        // Extract and handle <tool_call> tags
        let tool_call_re = Regex::new(r"<tool_call>([\s\S]*?)</tool_call>").ok()?;
        for cap in tool_call_re.captures_iter(content) {
            if let Some(tool_content) = cap.get(1)
                && let Some(tool_call) = self.parse_tool_call(tool_content.as_str())
            {
                tool_calls.push(tool_call);
                has_content = true;
            }
        }

        // If we found tool calls, add them to delta
        if !tool_calls.is_empty() {
            delta["tool_calls"] = json!(tool_calls);
            return Some(delta);
        }

        // Extract plain text (outside of XML tags)
        let mut plain_text = content.to_string();

        // Remove all XML tags to get plain content
        let tag_re = Regex::new(r"<[^>]+>[\s\S]*?</[^>]+>").ok()?;
        plain_text = tag_re.replace_all(&plain_text, "").to_string();

        // Also remove self-closing tags
        let self_closing_re = Regex::new(r"<[^>]+/>").ok()?;
        plain_text = self_closing_re.replace_all(&plain_text, "").to_string();

        plain_text = plain_text.trim().to_string();

        if !plain_text.is_empty() {
            delta["content"] = json!(plain_text);
            has_content = true;
        }

        if has_content || !delta.as_object()?.is_empty() {
            Some(delta)
        } else {
            None
        }
    }

    /// Parse a single tool_call XML block
    fn parse_tool_call(&self, tool_content: &str) -> Option<Value> {
        // Extract function name (first line or until first tag)
        let lines: Vec<&str> = tool_content.trim().lines().collect();
        if lines.is_empty() {
            return None;
        }

        let function_name = lines[0].trim();

        // Parse arguments
        let mut arguments = json!({});

        // Look for arg_key/arg_value pairs
        let arg_key_re = Regex::new(r"<arg_key>(.*?)</arg_key>").ok()?;
        let arg_value_re = Regex::new(r"<arg_value>([\s\S]*?)</arg_value>").ok()?;

        let keys: Vec<String> = arg_key_re
            .captures_iter(tool_content)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        let values: Vec<String> = arg_value_re
            .captures_iter(tool_content)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
            .collect();

        // Pair up keys and values
        for (key, value) in keys.iter().zip(values.iter()) {
            // Try to parse value as JSON, otherwise use as string
            if let Ok(json_value) = serde_json::from_str::<Value>(value) {
                arguments[key] = json_value;
            } else {
                arguments[key] = json!(value);
            }
        }

        // Generate a call ID
        let uuid_str = uuid::Uuid::new_v4().to_string().replace("-", "");
        let call_id = format!("call_{}", &uuid_str[..8]);

        Some(json!({
            "index": 0,
            "id": call_id,
            "type": "function",
            "function": {
                "name": function_name,
                "arguments": arguments.to_string()
            }
        }))
    }
}

/// Check if content contains XML-style tags
fn contains_xml_tags(content: &str) -> bool {
    content.contains("<think>")
        || content.contains("<tool_call>")
        || content.contains("<arg_key>")
        || content.contains("<function_call>")
}

/// Extract assistant content from a JSON chunk
fn extract_assistant_content(json_chunk: &Value) -> Option<String> {
    // Try streaming format
    if let Some(content) = json_chunk
        .get("choices")?
        .get(0)?
        .get("delta")?
        .get("content")?
        .as_str()
    {
        return Some(content.to_string());
    }

    // Try non-streaming format
    if let Some(content) = json_chunk
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()
    {
        return Some(content.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_think_tag() {
        let adapter = XmlResponseAdapter::new("glm-4.6".to_string());
        let xml = "<think>I'll analyze this problem step by step</think>";
        let result = adapter.parse_xml_content(xml).unwrap();

        assert_eq!(
            result["reasoning"]["text"],
            "I'll analyze this problem step by step"
        );
    }

    #[test]
    fn test_parse_tool_call() {
        let adapter = XmlResponseAdapter::new("glm-4.6".to_string());
        let xml = r#"<tool_call>update_plan
<arg_key>plan</arg_key>
<arg_value>[{"step": "test", "status": "pending"}]</arg_value>
</tool_call>"#;

        let result = adapter.parse_xml_response(xml).unwrap();
        let tool_calls = &result["choices"][0]["delta"]["tool_calls"];

        assert!(tool_calls.is_array());
        assert_eq!(tool_calls[0]["function"]["name"], "update_plan");
    }

    #[test]
    fn test_mixed_content() {
        let adapter = XmlResponseAdapter::new("glm-4.6".to_string());
        let xml = r#"<think>Planning the approach</think>
Let me help you with that.
<tool_call>search
<arg_key>query</arg_key>
<arg_value>rust programming</arg_value>
</tool_call>"#;

        let result = adapter.parse_xml_response(xml).unwrap();
        let delta = &result["choices"][0]["delta"];

        assert!(delta["reasoning"].is_object());
        assert!(delta["tool_calls"].is_array());
    }
}
