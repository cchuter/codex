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
        debug!("=== XML Adapter: transform_chunk called ===");
        debug!("Raw data: {}", raw_data);

        // First check if this already looks like JSON
        if raw_data.trim().starts_with('{') {
            debug!("Data starts with '{{' - attempting JSON parse");
            // If it's already JSON, check if it needs transformation
            if let Ok(json_chunk) = serde_json::from_str::<Value>(raw_data) {
                debug!("Successfully parsed as JSON");
                // Check if there's text content that might contain XML
                if let Some(content) = extract_assistant_content(&json_chunk) {
                    debug!("Extracted assistant content: {}", content);
                    if contains_xml_tags(&content) {
                        debug!("✓ XML tags detected in content field!");
                        debug!("Found XML tags in JSON content field: {}", content);
                        let result = self.transform_xml_in_json(json_chunk, &content);
                        if let Some(ref transformed) = result {
                            debug!(
                                "Transformed result: {}",
                                serde_json::to_string_pretty(transformed).unwrap_or_default()
                            );
                        }
                        return result;
                    } else {
                        debug!("✗ No XML tags found in content");
                    }
                } else {
                    debug!("✗ No assistant content found in JSON chunk");
                }
                return Some(json_chunk);
            } else {
                debug!("Failed to parse as JSON");
            }
        } else {
            debug!("Data does not start with '{{' - checking for pure XML");
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
            // Merge the parsed content into the existing delta/message
            if let Some(choices) = json_chunk
                .get_mut("choices")
                .and_then(|c| c.as_array_mut())
                .and_then(|arr| arr.get_mut(0))
            {
                // Handle different response formats
                if let Some(delta) = choices.get_mut("delta") {
                    // For streaming responses - replace delta content with parsed content
                    if let Some(delta_obj) = delta.as_object_mut()
                        && let Some(parsed_obj) = parsed.as_object()
                    {
                        // Clear the original content field with XML tags
                        delta_obj.remove("content");
                        // Merge all fields from parsed into delta
                        for (key, value) in parsed_obj {
                            delta_obj.insert(key.clone(), value.clone());
                        }
                    }
                } else if let Some(message) = choices.get_mut("message") {
                    // For non-streaming responses - replace message content with parsed content
                    if let Some(message_obj) = message.as_object_mut()
                        && let Some(parsed_obj) = parsed.as_object()
                    {
                        // Clear the original content field with XML tags
                        message_obj.remove("content");
                        // Merge all fields from parsed into message
                        for (key, value) in parsed_obj {
                            message_obj.insert(key.clone(), value.clone());
                        }
                    }
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
        debug!("Parsing XML content: {}", content);
        let mut delta = json!({});
        let mut has_content = false;
        let mut tool_calls = Vec::new();
        let mut reasoning_text = String::new();

        // Extract and handle <think> tags (reasoning)
        // Handle both formats: <think>content</think> and <think></think>\ncontent
        let think_re = Regex::new(r"<think>([\s\S]*?)</think>").ok()?;

        if let Some(cap) = think_re.captures(content) {
            // Check if think tags have content inside
            let inner_text = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            if !inner_text.is_empty() {
                reasoning_text = inner_text.to_string();
                debug!("Found reasoning text inside think tags: {}", reasoning_text);
            } else {
                // Think tags are empty, check for text immediately after </think>
                let after_think_re = Regex::new(r"</think>\s*\n?([^\n<]+)").ok()?;
                if let Some(after_cap) = after_think_re.captures(content)
                    && let Some(text_after) = after_cap.get(1) {
                        reasoning_text = text_after.as_str().trim().to_string();
                        debug!(
                            "Found reasoning text after empty think tags: {}",
                            reasoning_text
                        );
                    }
            }

            if !reasoning_text.is_empty() {
                delta["reasoning"] = json!({
                    "text": reasoning_text
                });
                has_content = true;
            }
        }

        // Extract and handle <tool_call> tags
        let tool_call_re = Regex::new(r"<tool_call>([\s\S]*?)</tool_call>").ok()?;
        for cap in tool_call_re.captures_iter(content) {
            if let Some(tool_content) = cap.get(1)
                && let Some(tool_call) = self.parse_tool_call(tool_content.as_str())
            {
                debug!("Found tool call: {:?}", tool_call);
                tool_calls.push(tool_call);
                has_content = true;
            }
        }

        // If we found tool calls, add them to delta
        if !tool_calls.is_empty() {
            debug!("Adding {} tool calls to delta", tool_calls.len());
            delta["tool_calls"] = json!(tool_calls);
            has_content = true;
        }

        // Extract plain text (text that's not inside XML tags and not reasoning)
        // First, remove the XML blocks we've already processed
        let mut plain_text = content.to_string();

        // Remove think tags and any reasoning text we captured
        // First remove non-empty think tags and their content
        let think_with_content_re = Regex::new(r"<think>[\s\S]*?</think>").ok()?;
        plain_text = think_with_content_re.replace_all(&plain_text, "").to_string();

        // Then clean up any text that immediately followed empty think tags (already in reasoning)
        if !reasoning_text.is_empty() {
            plain_text = plain_text.replace(&reasoning_text, "");
        }

        // Remove tool_call blocks completely
        let tool_call_block_re = Regex::new(r"<tool_call>[\s\S]*?</tool_call>").ok()?;
        plain_text = tool_call_block_re.replace_all(&plain_text, "").to_string();

        // Clean up any remaining XML tags
        let remaining_tags_re = Regex::new(r"<[^>]+>").ok()?;
        plain_text = remaining_tags_re.replace_all(&plain_text, "").to_string();

        // Clean up extra whitespace
        plain_text = plain_text.trim().to_string();

        // Only add content if we have plain text that's not already in reasoning
        if !plain_text.is_empty() && plain_text != reasoning_text {
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
    debug!("Extracting assistant content from JSON chunk");

    // Try streaming format
    if let Some(choices) = json_chunk.get("choices") {
        debug!("Found 'choices' field");
        if let Some(choice) = choices.get(0) {
            debug!("Found first choice");
            if let Some(delta) = choice.get("delta") {
                debug!("Found 'delta' field (streaming format)");
                if let Some(content) = delta.get("content") {
                    debug!("Found 'content' in delta");
                    if let Some(content_str) = content.as_str() {
                        debug!("Content as string: {}", content_str);
                        return Some(content_str.to_string());
                    } else {
                        debug!("Content is not a string");
                    }
                } else {
                    debug!("No 'content' field in delta");
                }
            } else if let Some(message) = choice.get("message") {
                debug!("Found 'message' field (non-streaming format)");
                if let Some(content) = message.get("content") {
                    debug!("Found 'content' in message");
                    if let Some(content_str) = content.as_str() {
                        debug!("Content as string: {}", content_str);
                        return Some(content_str.to_string());
                    }
                }
            } else {
                debug!("No 'delta' or 'message' field in choice");
            }
        } else {
            debug!("No first choice in choices array");
        }
    } else {
        debug!("No 'choices' field in JSON chunk");
    }

    debug!("Failed to extract assistant content");
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

    #[test]
    fn test_empty_think_tags_with_text_after() {
        let adapter = XmlResponseAdapter::new("glm-4.6".to_string());

        // This is the ACTUAL format the user is seeing with empty think tags
        let xml = r#"<think></think>
I'll create a gSwap trading bot that alternates between buying and selling a fixed amount every minute. Let me start by exploring the existing codebase structure to understand how gSwap works.
<tool_call>update_plan
<arg_key>plan</arg_key>
<arg_value>[{"step": "Explore existing gSwap codebase structure", "status": "in_progress"}, {"step": "Create trading bot with alternating buy/sell logic", "status": "pending"}]</arg_value>
</tool_call>"#;

        let result = adapter.parse_xml_content(xml).unwrap();

        // Check reasoning was extracted from after the empty think tags
        assert!(result["reasoning"].is_object(), "Should have reasoning");
        assert!(
            result["reasoning"]["text"]
                .as_str()
                .unwrap()
                .contains("gSwap trading bot")
        );

        // Check tool calls were extracted
        assert!(result["tool_calls"].is_array(), "Should have tool_calls");
        assert_eq!(result["tool_calls"][0]["function"]["name"], "update_plan");

        // Check no XML tags remain
        if let Some(content) = result.get("content") {
            let content_str = content.as_str().unwrap();
            assert!(!content_str.contains("<think>"));
            assert!(!content_str.contains("</think>"));
            assert!(!content_str.contains("<tool_call>"));
        }
    }

    #[test]
    fn test_user_reported_xml() {
        let adapter = XmlResponseAdapter::new("glm-4.6".to_string());

        // This is the exact content the user is seeing
        let xml = r#"<think>
I'll create a gSwap trading bot that alternates between buying and selling a fixed amount every minute. Let me start by exploring the codebase structure to understand how to integrate with gSwap.
</think>
<tool_call>update_plan
<arg_key>plan</arg_key>
<arg_value>[{"step": "Explore codex-rs structure and gSwap integration", "status": "in_progress"}, {"step": "Create trading bot with alternating buy/sell logic", "status": "pending"}]</arg_value>
</tool_call>"#;

        // Test with JSON-wrapped content (as it comes from the API)
        let json_chunk = serde_json::json!({
            "choices": [{
                "index": 0,
                "delta": {
                    "content": xml
                },
                "finish_reason": null
            }]
        });

        // Transform the chunk
        let result = adapter.transform_chunk(&json_chunk.to_string()).unwrap();
        let delta = &result["choices"][0]["delta"];

        // Verify the transformation worked
        assert!(
            delta["reasoning"].is_object(),
            "Should have reasoning field"
        );
        assert_eq!(
            delta["reasoning"]["text"].as_str().unwrap().trim(),
            "I'll create a gSwap trading bot that alternates between buying and selling a fixed amount every minute. Let me start by exploring the codebase structure to understand how to integrate with gSwap."
        );

        assert!(
            delta["tool_calls"].is_array(),
            "Should have tool_calls field"
        );
        let tool_calls = delta["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1, "Should have one tool call");

        let tool_call = &tool_calls[0];
        assert_eq!(
            tool_call["function"]["name"].as_str().unwrap(),
            "update_plan"
        );

        // Verify no XML tags remain in content
        if let Some(content) = delta.get("content") {
            let content_str = content.as_str().unwrap();
            println!("Content field after transformation: '{}'", content_str);
            assert!(
                !content_str.contains("<think>"),
                "Should not contain <think> tags, but got: {}",
                content_str
            );
            assert!(
                !content_str.contains("<tool_call>"),
                "Should not contain <tool_call> tags"
            );
            assert!(
                !content_str.contains("<arg_key>"),
                "Should not contain <arg_key> tags"
            );
            assert!(
                !content_str.contains("<arg_value>"),
                "Should not contain <arg_value> tags"
            );
        }
    }
}
