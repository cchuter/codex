//! Buffered XML Response Adapter for handling fragmented XML streaming
//!
//! This adapter handles models like glm-4.6 that stream XML tags in separate SSE chunks.
//! It buffers partial content and only transforms when complete XML structures are detected.

use regex_lite::Regex;
use serde_json::{Value, json};
use std::collections::VecDeque;

/// Buffered XML adapter that handles fragmented streaming
pub struct BufferedXmlAdapter {
    /// Buffer for accumulating partial XML content
    buffer: String,
    /// Queue of complete XML blocks ready to be processed
    complete_blocks: VecDeque<String>,
    /// Track if we've already sent tool_calls to prevent duplicate finish_reason
    sent_tool_calls: bool,
}

impl BufferedXmlAdapter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            complete_blocks: VecDeque::new(),
            sent_tool_calls: false,
        }
    }

    /// Process a new SSE chunk, accumulating in buffer
    pub fn process_chunk(&mut self, raw_data: &str) -> Option<Value> {
        // First check if this is already JSON
        if raw_data.trim().starts_with('{')
            && let Ok(mut json_chunk) = serde_json::from_str::<Value>(raw_data)
        {
            // Check for finish_reason to detect end of stream
            if let Some(finish_reason_val) = json_chunk
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|choice| choice.get("finish_reason"))
            {
                // Check if it's not null (could be a string or null)
                if !finish_reason_val.is_null() {
                    // If finish_reason is "stop" and we've already sent tool_calls, suppress this chunk
                    if finish_reason_val.as_str() == Some("stop") && self.sent_tool_calls {
                        return None;
                    }

                    // If we have an incomplete tool_call in buffer when stream ends,
                    // treat it as complete (glm-4.6 doesn't send closing tags)
                    if !self.buffer.is_empty() && self.buffer.contains("<tool_call>") {
                        // Add a closing tag to make it complete
                        self.buffer.push_str("</tool_call>");

                        // Extract the now-complete block
                        self.extract_complete_blocks();

                        // Process any complete blocks
                        if !self.complete_blocks.is_empty() {
                            let processed = self.create_delta_from_buffer();

                            if let Some(obj) = processed.as_object()
                                && !obj.is_empty()
                            {
                                if let Some(choices) = json_chunk
                                    .get_mut("choices")
                                    .and_then(|c| c.as_array_mut())
                                    .and_then(|arr| arr.get_mut(0))
                                    && let Some(delta_obj) =
                                        choices.get_mut("delta").and_then(|d| d.as_object_mut())
                                {
                                    delta_obj.clear();
                                    for (key, value) in obj {
                                        delta_obj.insert(key.clone(), value.clone());
                                    }

                                    // Set finish_reason to tool_calls if we have them
                                    if delta_obj.contains_key("tool_calls")
                                        && let Some(choice) = json_chunk
                                            .get_mut("choices")
                                            .and_then(|c| c.as_array_mut())
                                            .and_then(|arr| arr.get_mut(0))
                                            .and_then(|c| c.as_object_mut())
                                    {
                                        choice.insert(
                                            "finish_reason".to_string(),
                                            json!("tool_calls"),
                                        );
                                    }
                                }
                                return Some(json_chunk);
                            }
                        }
                    }
                }
            }

            // Check if there's content field with XML
            if let Some(content) = extract_content_from_json(&json_chunk) {
                // Check if content contains XML tags or we have buffered content
                if !self.buffer.is_empty()
                    || content.contains("<think>")
                    || content.contains("</think>")
                    || content.contains("<tool_call>")
                    || content.contains("</tool_call>")
                    || content.contains("<arg_key>")
                    || content.contains("<arg_value>")
                    || content.contains("</arg_key>")
                    || content.contains("</arg_value>")
                    || content.contains("<function_call>")
                {
                    // Add content to buffer for accumulation
                    self.buffer.push_str(&content);

                    // Try to extract complete blocks
                    self.extract_complete_blocks();

                    // Process what we have
                    let processed = self.create_delta_from_buffer();

                    // Only return a chunk if we have something meaningful to send
                    if let Some(obj) = processed.as_object()
                        && !obj.is_empty()
                    {
                        // Build the response with processed content
                        if let Some(choices) = json_chunk
                            .get_mut("choices")
                            .and_then(|c| c.as_array_mut())
                            .and_then(|arr| arr.get_mut(0))
                            && let Some(delta_obj) =
                                choices.get_mut("delta").and_then(|d| d.as_object_mut())
                        {
                            // Clear original content with XML
                            delta_obj.clear();

                            // Add processed fields
                            for (key, value) in obj {
                                delta_obj.insert(key.clone(), value.clone());
                            }

                            // Set finish_reason to "tool_calls" when we have tool_calls
                            if delta_obj.contains_key("tool_calls")
                                && let Some(choice) = json_chunk
                                    .get_mut("choices")
                                    .and_then(|c| c.as_array_mut())
                                    .and_then(|arr| arr.get_mut(0))
                                    .and_then(|c| c.as_object_mut())
                            {
                                choice.insert("finish_reason".to_string(), json!("tool_calls"));
                                self.sent_tool_calls = true;
                            }
                        }
                        return Some(json_chunk);
                    }

                    // If we're buffering incomplete XML, don't return anything yet
                    return None;
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

    /// Create a response from complete blocks (for non-JSON input)
    fn create_response_from_blocks(&mut self) -> Option<Value> {
        let delta = self.create_delta_from_buffer();

        // Only return a response if we have something to send
        if delta.as_object()?.is_empty() {
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

    /// Create delta from current buffer and complete blocks state
    fn create_delta_from_buffer(&mut self) -> Value {
        let mut delta = json!({});

        // Process complete blocks
        if !self.complete_blocks.is_empty() {
            let mut tool_calls = Vec::new();

            while let Some(block) = self.complete_blocks.pop_front() {
                if block.contains("<think>")
                    && let Some(reasoning) = self.extract_reasoning(&block)
                {
                    delta["reasoning"] = json!({ "text": reasoning });
                } else if block.contains("<tool_call>")
                    && let Some(tool_call) = self.extract_tool_call(&block)
                {
                    tool_calls.push(tool_call);
                }
            }

            if !tool_calls.is_empty() {
                delta["tool_calls"] = json!(tool_calls);
            }
        }

        // Don't output any partial/incomplete XML content from the buffer
        // The buffer is only for accumulating incomplete XML blocks
        // We should only output content when we have complete blocks

        delta
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
        .map(ToString::to_string)
        .or_else(|| {
            // Try non-streaming format
            json_chunk
                .get("choices")?
                .get(0)?
                .get("message")?
                .get("content")?
                .as_str()
                .map(ToString::to_string)
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
