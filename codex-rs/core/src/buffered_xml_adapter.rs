//! Buffered XML Response Adapter for handling fragmented XML streaming
//!
//! This adapter handles models like glm-4.6 that stream XML tags in separate SSE chunks.
//! It buffers partial content and only transforms when complete XML structures are detected.

use regex_lite::Regex;
use serde_json::{Value, json};
use std::collections::VecDeque;

/// State machine for tracking XML parsing state
#[derive(Debug, Clone, PartialEq)]
enum XmlParseState {
    /// Looking for opening tags
    Searching,
    /// Inside a think tag
    InThinkTag { depth: usize },
    /// Inside a tool_call tag
    InToolCallTag { depth: usize },
    /// Accumulated complete blocks ready to process
    Complete,
}

/// Buffered XML adapter that handles fragmented streaming
pub struct BufferedXmlAdapter {
    /// Buffer for accumulating partial XML content
    buffer: String,
    /// Current parsing state
    state: XmlParseState,
    /// Queue of complete XML blocks ready to be processed
    complete_blocks: VecDeque<String>,
    /// Track if we're between chunks that might be XML
    partial_tag_buffer: String,
}

impl BufferedXmlAdapter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            state: XmlParseState::Searching,
            partial_tag_buffer: String::new(),
            complete_blocks: VecDeque::new(),
        }
    }

    /// Process a new SSE chunk, accumulating in buffer
    pub fn process_chunk(&mut self, raw_data: &str) -> Option<Value> {
        // First check if this is already JSON
        if raw_data.trim().starts_with('{')
            && let Ok(mut json_chunk) = serde_json::from_str::<Value>(raw_data)
        {
            // Check if there's content field with XML
            if let Some(content) = extract_content_from_json(&json_chunk) {
                // Check if content contains XML tags
                if content.contains("<think>")
                    || content.contains("</think>")
                    || content.contains("<tool_call>")
                    || content.contains("</tool_call>")
                    || content.contains("<arg_key>")
                    || content.contains("<arg_value>")
                    || content.contains("</arg_key>")
                    || content.contains("</arg_value>")
                    || content.contains("<function_call>")
                {
                    // Process the content to separate XML from plain text
                    let processed = self.process_mixed_content(&content);

                    // Build the response with processed content
                    if let Some(choices) = json_chunk
                        .get_mut("choices")
                        .and_then(|c| c.as_array_mut())
                        .and_then(|arr| arr.get_mut(0))
                    {
                        if let Some(delta) = choices.get_mut("delta") {
                            // Replace the content with our processed version
                            if let Some(delta_obj) = delta.as_object_mut() {
                                // Clear original content with XML
                                delta_obj.clear();

                                // Add processed fields
                                if let Some(obj) = processed.as_object() {
                                    for (key, value) in obj {
                                        delta_obj.insert(key.clone(), value.clone());
                                    }
                                }
                            }
                        }
                    }

                    // Return the cleaned chunk
                    return Some(json_chunk);
                }
            }
            // Return the JSON as-is if no XML transformation needed
            return Some(json_chunk);
        }

        // Handle pure text/XML chunks
        self.buffer.push_str(raw_data);

        // Check for complete XML structures
        self.extract_complete_blocks();

        // If we have complete blocks, create a response
        if !self.complete_blocks.is_empty() {
            return self.create_response_from_blocks();
        }

        None
    }

    /// Extract complete XML blocks from the buffer
    fn extract_complete_blocks(&mut self) {
        let mut search_from = 0;

        loop {
            // Look for complete think blocks
            if let Some((start, end)) = self.find_complete_tag(&self.buffer[search_from..], "think")
            {
                let adjusted_start = search_from + start;
                let adjusted_end = search_from + end;

                // Extract the complete block
                let block = self.buffer[adjusted_start..adjusted_end].to_string();
                self.complete_blocks.push_back(block);

                // Remove from buffer
                self.buffer.drain(adjusted_start..adjusted_end);
                // Reset search position since we modified the buffer
                search_from = adjusted_start;
                continue;
            }

            // Look for complete tool_call blocks
            if let Some((start, end)) =
                self.find_complete_tag(&self.buffer[search_from..], "tool_call")
            {
                let adjusted_start = search_from + start;
                let adjusted_end = search_from + end;

                // Extract the complete block
                let block = self.buffer[adjusted_start..adjusted_end].to_string();
                self.complete_blocks.push_back(block);

                // Remove from buffer
                self.buffer.drain(adjusted_start..adjusted_end);
                // Reset search position since we modified the buffer
                search_from = adjusted_start;
                continue;
            }

            // No more complete blocks found
            break;
        }
    }

    /// Find a complete XML tag pair in the text
    fn find_complete_tag(&self, text: &str, tag_name: &str) -> Option<(usize, usize)> {
        let open_tag = format!("<{tag_name}>");
        let close_tag = format!("</{tag_name}>");

        if let Some(start_pos) = text.find(&open_tag) {
            // Look for the matching close tag
            if let Some(close_pos) = text[start_pos..].find(&close_tag) {
                let end_pos = start_pos + close_pos + close_tag.len();
                return Some((start_pos, end_pos));
            }
        }

        None
    }

    /// Transform accumulated complete blocks
    fn transform_complete_blocks(&mut self, mut json_chunk: Value) -> Option<Value> {
        let mut delta = json!({});
        let mut has_content = false;

        // Process all complete blocks
        while let Some(block) = self.complete_blocks.pop_front() {
            if block.contains("<think>") {
                // Extract reasoning content
                if let Some(reasoning) = self.extract_reasoning(&block) {
                    delta["reasoning"] = json!({ "text": reasoning });
                    has_content = true;
                }
            } else if block.contains("<tool_call>") {
                // Extract tool call
                if let Some(tool_call) = self.extract_tool_call(&block) {
                    let tool_calls = delta
                        .get_mut("tool_calls")
                        .and_then(|v| v.as_array_mut())
                        .map(|a| {
                            a.push(tool_call.clone());
                            a.clone()
                        })
                        .unwrap_or_else(|| vec![tool_call]);

                    delta["tool_calls"] = json!(tool_calls);
                    has_content = true;
                }
            }
        }

        // Check for any plain text in buffer (non-XML content)
        let plain_text = self.extract_plain_text();
        if !plain_text.is_empty() {
            delta["content"] = json!(plain_text);
            has_content = true;
        }

        if !has_content {
            // Don't return empty chunks
            return None;
        }

        // Create a clean JSON response with the transformed content
        // Replace the entire delta to avoid duplicating XML content
        if let Some(choices) = json_chunk
            .get_mut("choices")
            .and_then(|c| c.as_array_mut())
            .and_then(|arr| arr.get_mut(0))
        {
            // Replace the entire delta with our cleaned version
            choices["delta"] = delta;
        }

        Some(json_chunk)
    }

    /// Create a response from complete blocks (for non-JSON input)
    fn create_response_from_blocks(&mut self) -> Option<Value> {
        let mut delta = json!({});
        let mut has_content = false;

        // Process all complete blocks
        while let Some(block) = self.complete_blocks.pop_front() {
            if block.contains("<think>") {
                if let Some(reasoning) = self.extract_reasoning(&block) {
                    delta["reasoning"] = json!({ "text": reasoning });
                    has_content = true;
                }
            } else if block.contains("<tool_call>") {
                if let Some(tool_call) = self.extract_tool_call(&block) {
                    let tool_calls = delta
                        .get_mut("tool_calls")
                        .and_then(|v| v.as_array_mut())
                        .map(|a| {
                            a.push(tool_call.clone());
                            a.clone()
                        })
                        .unwrap_or_else(|| vec![tool_call]);

                    delta["tool_calls"] = json!(tool_calls);
                    has_content = true;
                }
            }
        }

        // Check for plain text
        let plain_text = self.extract_plain_text();
        if !plain_text.is_empty() {
            delta["content"] = json!(plain_text);
            has_content = true;
        }

        if !has_content {
            return None;
        }

        // Wrap in OpenAI response structure
        Some(json!({
            "choices": [{
                "index": 0,
                "delta": delta,
                "finish_reason": null
            }]
        }))
    }

    /// Extract reasoning from a think block
    fn extract_reasoning(&self, block: &str) -> Option<String> {
        let think_re = Regex::new(r"<think>([\s\S]*?)</think>").ok()?;

        if let Some(cap) = think_re.captures(block) {
            let content = cap.get(1)?.as_str().trim();
            if !content.is_empty() {
                return Some(content.to_string());
            }
        }

        // Check for text immediately after empty think tags
        if block.contains("<think></think>") {
            let after_think_re = Regex::new(r"</think>\s*\n?([^\n<]+)").ok()?;
            if let Some(cap) = after_think_re.captures(block) {
                let content = cap.get(1)?.as_str().trim();
                if !content.is_empty() {
                    return Some(content.to_string());
                }
            }
        }

        None
    }

    /// Extract tool call from a tool_call block
    fn extract_tool_call(&self, block: &str) -> Option<Value> {
        let tool_call_re = Regex::new(r"<tool_call>([\s\S]*?)</tool_call>").ok()?;

        if let Some(cap) = tool_call_re.captures(block) {
            let content = cap.get(1)?.as_str();

            // Extract function name (first line)
            let lines: Vec<&str> = content.trim().lines().collect();
            if lines.is_empty() {
                return None;
            }

            let function_name = lines[0].trim();

            // Extract arguments
            let mut arguments = json!({});
            let arg_key_re = Regex::new(r"<arg_key>(.*?)</arg_key>").ok()?;
            let arg_value_re = Regex::new(r"<arg_value>([\s\S]*?)</arg_value>").ok()?;

            let keys: Vec<String> = arg_key_re
                .captures_iter(content)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .collect();

            let values: Vec<String> = arg_value_re
                .captures_iter(content)
                .filter_map(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
                .collect();

            for (key, value) in keys.iter().zip(values.iter()) {
                if let Ok(json_value) = serde_json::from_str::<Value>(value) {
                    arguments[key] = json_value;
                } else {
                    arguments[key] = json!(value);
                }
            }

            // Generate call ID
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
        } else {
            None
        }
    }

    /// Process mixed content containing both XML and plain text
    fn process_mixed_content(&mut self, content: &str) -> Value {
        let mut delta = json!({});

        // Store the original content to extract plain text from it later
        let original_content = content.to_string();

        // Add content to buffer for XML processing
        self.buffer.push_str(content);

        // Extract complete XML blocks (this removes them from the buffer)
        self.extract_complete_blocks();

        // Process any complete blocks
        let mut extracted_blocks = Vec::new();
        if !self.complete_blocks.is_empty() {
            let mut tool_calls = Vec::new();

            while let Some(block) = self.complete_blocks.pop_front() {
                extracted_blocks.push(block.clone());

                if block.contains("<think>") {
                    if let Some(reasoning) = self.extract_reasoning(&block) {
                        delta["reasoning"] = json!({ "text": reasoning });
                    }
                } else if block.contains("<tool_call>") {
                    if let Some(tool_call) = self.extract_tool_call(&block) {
                        tool_calls.push(tool_call);
                    }
                }
            }

            if !tool_calls.is_empty() {
                delta["tool_calls"] = json!(tool_calls);
            }
        }

        // Extract plain text by removing the extracted blocks from original content
        let mut cleaned_content = original_content.clone();

        // Remove all the blocks we extracted
        for block in &extracted_blocks {
            cleaned_content = cleaned_content.replace(block, "");
        }

        // Also remove any partial/incomplete XML tags
        let partial_tags = vec![
            r"</?think>",
            r"</?tool_call>",
            r"</?arg_key>",
            r"</?arg_value>",
            r"</?function_call>",
        ];

        for tag_pattern in partial_tags {
            if let Ok(re) = Regex::new(tag_pattern) {
                cleaned_content = re.replace_all(&cleaned_content, "").to_string();
            }
        }

        // Clean up whitespace
        let cleaned_content = cleaned_content.trim();

        if !cleaned_content.is_empty() {
            delta["content"] = json!(cleaned_content);
        }

        delta
    }

    /// Extract plain text from buffer (non-XML content)
    fn extract_plain_text(&mut self) -> String {
        // Remove all XML tags to get plain text
        let tag_re = Regex::new(r"<[^>]+>").unwrap();
        let plain = tag_re.replace_all(&self.buffer, "").trim().to_string();

        if !plain.is_empty() {
            // Clear the plain text from buffer
            self.buffer.clear();
        }

        plain
    }

    /// Check if we're still waiting for more content
    pub fn has_pending_content(&self) -> bool {
        !self.buffer.is_empty() || !self.complete_blocks.is_empty()
    }

    /// Flush any remaining content (call when stream ends)
    pub fn flush(&mut self) -> Option<Value> {
        if self.has_pending_content() {
            // Try to extract any remaining complete blocks
            self.extract_complete_blocks();

            // Process what we have
            if !self.complete_blocks.is_empty() {
                return self.create_response_from_blocks();
            }

            // If we have leftover buffer content, treat it as plain text
            if !self.buffer.is_empty() {
                let content = self.buffer.clone();
                self.buffer.clear();

                return Some(json!({
                    "choices": [{
                        "index": 0,
                        "delta": {
                            "content": content
                        },
                        "finish_reason": null
                    }]
                }));
            }
        }

        None
    }
}

/// Extract content from a JSON chunk
fn extract_content_from_json(json_chunk: &Value) -> Option<String> {
    // Try streaming format
    json_chunk
        .get("choices")?
        .get(0)?
        .get("delta")?
        .get("content")?
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            // Try non-streaming format
            json_chunk
                .get("choices")?
                .get(0)?
                .get("message")?
                .get("content")?
                .as_str()
                .map(|s| s.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fragmented_think_tags() {
        let mut adapter = BufferedXmlAdapter::new();

        // Simulate fragmented streaming
        let chunk1 = json!({
            "choices": [{
                "delta": { "content": "<think>" },
                "index": 0
            }]
        });

        let chunk2 = json!({
            "choices": [{
                "delta": { "content": "I'm thinking about this" },
                "index": 0
            }]
        });

        let chunk3 = json!({
            "choices": [{
                "delta": { "content": "</think>" },
                "index": 0
            }]
        });

        // Process chunks
        let result1 = adapter.process_chunk(&chunk1.to_string());
        assert!(result1.is_some()); // Returns the chunk as-is since no complete blocks yet

        let result2 = adapter.process_chunk(&chunk2.to_string());
        assert!(result2.is_some()); // Still accumulating

        let result3 = adapter.process_chunk(&chunk3.to_string());
        assert!(result3.is_some()); // Should now have complete block

        // Verify the final result has the reasoning
        if let Some(result) = result3 {
            let delta = &result["choices"][0]["delta"];
            assert!(delta["reasoning"].is_object());
            assert_eq!(
                delta["reasoning"]["text"].as_str().unwrap(),
                "I'm thinking about this"
            );
        }
    }

    #[test]
    fn test_fragmented_tool_call() {
        let mut adapter = BufferedXmlAdapter::new();

        // Simulate extremely fragmented tool call
        let chunks = vec![
            "<tool_call>",
            "search",
            "\n<arg_key>",
            "query",
            "</arg_key>",
            "\n<arg_value>",
            "rust programming",
            "</arg_value>",
            "\n</tool_call>",
        ];

        let mut last_result = None;
        for chunk_text in chunks {
            let chunk = json!({
                "choices": [{
                    "delta": { "content": chunk_text },
                    "index": 0
                }]
            });

            last_result = adapter.process_chunk(&chunk.to_string());
        }

        // The last chunk should trigger the complete tool call
        assert!(last_result.is_some());

        if let Some(result) = last_result {
            let delta = &result["choices"][0]["delta"];
            assert!(delta["tool_calls"].is_array());

            let tool_calls = delta["tool_calls"].as_array().unwrap();
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(
                tool_calls[0]["function"]["name"].as_str().unwrap(),
                "search"
            );
        }
    }
}
