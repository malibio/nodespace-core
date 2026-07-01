/// Tool-call parser for raw GGUF tool-call formats.
///
/// Handles two model families' wire formats, both of which use the
/// `[TOOL_CALLS]` sentinel but disagree on what follows it:
///
/// **Mistral / Ministral** -- function name precedes the JSON args:
///
/// ```text
/// [TOOL_CALLS]function_name[ARGS]{"param": "value"}   (Format A, legacy)
/// [TOOL_CALLS]function_name{"param": "value"}         (Format B, Ministral 2512+)
/// ```
///
/// Multiple calls chain naturally:
///
/// ```text
/// [TOOL_CALLS]search_nodes[ARGS]{"query": "embeddings"}[TOOL_CALLS]search_nodes[ARGS]{"query": "vector search"}
/// ```
///
/// **Gemma 4** -- function name lives inside the JSON object, keyed `tool_name`
/// and `tool_args` instead of Mistral's `name`/`arguments`:
///
/// ```text
/// [TOOL_CALLS]{"tool_name": "search_nodes", "tool_args": {"query": "embeddings"}}     (Format C, object)
/// [TOOL_CALLS][{"tool_name": "x", "tool_args": {...}}, {...}]                         (Format C, array)
/// ```
///
/// **Ornith / Qwen3.5** -- XML-like envelope defined by the model's embedded chat template:
///
/// ```text
/// <tool_call>
/// <function=search_nodes>
/// <parameter=query>
/// Rust programming
/// </parameter>
/// </function>
/// </tool_call>
/// ```
///
/// Multiple calls appear as consecutive `<tool_call>` blocks. Parameter values
/// are plain text (not JSON-encoded), supporting multi-line values naturally.
///
/// This module provides both a complete-text parser and a streaming parser that
/// handles partial sentinels split across token boundaries.
/// Sentinel markers shared by both formats.
const TOOL_CALLS_SENTINEL: &str = "[TOOL_CALLS]";
const ARGS_SENTINEL: &str = "[ARGS]";

/// Mistral-style tool call object: `{"name": ..., "arguments": ...}`.
#[derive(serde::Deserialize)]
struct MistralCallObject {
    name: String,
    arguments: serde_json::Value,
}

/// Gemma 4 tool call object: `{"tool_name": ..., "tool_args": ...}`.
#[derive(serde::Deserialize)]
struct GemmaCallObject {
    tool_name: String,
    tool_args: serde_json::Value,
}

/// Try to deserialize a JSON object as either Mistral or Gemma 4 tool call.
///
/// Both families wrap a single function invocation but use different field
/// names. We try Mistral first (the original format) and fall back to Gemma 4.
fn parse_tool_call_object(json_str: &str) -> Option<ParsedToolCall> {
    if let Ok(c) = serde_json::from_str::<MistralCallObject>(json_str) {
        return Some(ParsedToolCall {
            name: c.name,
            args: c.arguments,
        });
    }
    if let Ok(c) = serde_json::from_str::<GemmaCallObject>(json_str) {
        return Some(ParsedToolCall {
            name: c.tool_name,
            args: c.tool_args,
        });
    }
    None
}

/// A single parsed tool call extracted from model output.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedToolCall {
    /// Name of the tool the model wants to invoke.
    pub name: String,
    /// Parsed JSON arguments for the tool.
    pub args: serde_json::Value,
}

/// Result of attempting to parse tool calls from complete text.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseResult {
    /// No tool-call sentinel found; the text is plain assistant output.
    PlainText(String),
    /// One or more tool calls were found.
    ToolCalls(Vec<ParsedToolCall>),
    /// The sentinel was found but the format was invalid.
    Error(String),
}

/// Parse tool calls from a complete response string.
///
/// Supports two Mistral tool call formats:
///
/// **Format A** (legacy, with `[ARGS]` sentinel):
/// ```text
/// [TOOL_CALLS]function_name[ARGS]{"param": "value"}
/// ```
///
/// **Format B** (Ministral 2512+, no `[ARGS]` sentinel):
/// ```text
/// [TOOL_CALLS]function_name{"param": "value"}
/// ```
///
/// Returns `ParseResult::PlainText` if no `[TOOL_CALLS]` sentinel is found,
/// `ParseResult::ToolCalls` for successfully parsed calls, or
/// `ParseResult::Error` for malformed tool-call output.
pub fn parse_tool_calls(text: &str) -> ParseResult {
    if !text.contains(TOOL_CALLS_SENTINEL) {
        return ParseResult::PlainText(text.to_string());
    }

    let mut calls = Vec::new();
    let mut remaining = text;

    // Skip any text before the first [TOOL_CALLS] sentinel
    if let Some(idx) = remaining.find(TOOL_CALLS_SENTINEL) {
        remaining = &remaining[idx..];
    }

    while let Some(tc_start) = remaining.find(TOOL_CALLS_SENTINEL) {
        let after_sentinel = &remaining[tc_start + TOOL_CALLS_SENTINEL.len()..];

        // Format C (Gemma 4 / OpenAI-style): the function name lives inside the
        // JSON object as `tool_name` (Gemma) or `name` (OpenAI), so the JSON
        // appears directly after [TOOL_CALLS] with no name prefix. Detect by
        // looking at the first non-whitespace byte after the sentinel.
        let trimmed = after_sentinel.trim_start();
        let leading_ws = after_sentinel.len() - trimmed.len();
        match trimmed.as_bytes().first() {
            Some(b'{') => {
                let json_region = &after_sentinel[leading_ws..];
                let json_end = match find_balanced_brace(json_region) {
                    Some(e) => e,
                    None => {
                        return ParseResult::Error(
                            "Unbalanced braces in tool call object".to_string(),
                        );
                    }
                };
                let json_str = json_region[..json_end].trim();
                match parse_tool_call_object(json_str) {
                    Some(call) => calls.push(call),
                    None => {
                        return ParseResult::Error(format!(
                            "Tool call object did not match Mistral or Gemma 4 schema (raw: {:?})",
                            json_str
                        ));
                    }
                }
                let after_json = &json_region[json_end..];
                remaining = match after_json.find(TOOL_CALLS_SENTINEL) {
                    Some(idx) => &after_json[idx..],
                    None => "",
                };
                continue;
            }
            Some(b'[') => {
                let arr_region = &after_sentinel[leading_ws..];
                // [ARGS] is a sentinel, not a JSON array — fall through to
                // the legacy Format-A path so we report it as "empty function
                // name" rather than a misleading "invalid JSON array" error.
                if !arr_region.starts_with(ARGS_SENTINEL) {
                    let arr_end = match find_balanced_bracket(arr_region) {
                        Some(e) => e,
                        None => {
                            return ParseResult::Error(
                                "Unbalanced brackets after [TOOL_CALLS]".to_string(),
                            );
                        }
                    };
                    let arr_str = arr_region[..arr_end].trim();
                    let arr: Vec<serde_json::Value> = match serde_json::from_str(arr_str) {
                        Ok(v) => v,
                        Err(e) => {
                            return ParseResult::Error(format!(
                                "Invalid JSON array of tool calls: {} (raw: {:?})",
                                e, arr_str
                            ));
                        }
                    };
                    for elem in arr {
                        let elem_str = elem.to_string();
                        match parse_tool_call_object(&elem_str) {
                            Some(call) => calls.push(call),
                            None => {
                                return ParseResult::Error(format!(
                                    "Array element did not match Mistral or Gemma 4 schema (raw: {})",
                                    elem_str
                                ));
                            }
                        }
                    }
                    let after_arr = &arr_region[arr_end..];
                    remaining = match after_arr.find(TOOL_CALLS_SENTINEL) {
                        Some(idx) => &after_arr[idx..],
                        None => "",
                    };
                    continue;
                }
            }
            _ => {}
        }

        // Format A/B: function name precedes the JSON (Mistral / Ministral).
        let has_args_sentinel = after_sentinel.find(ARGS_SENTINEL);
        let has_json_start = after_sentinel.find('{');

        let (function_name, json_str, advance_to) = if let (Some(args_pos), Some(json_pos)) =
            (has_args_sentinel, has_json_start)
        {
            if args_pos < json_pos {
                // Format A: [TOOL_CALLS]name[ARGS]{json}
                let name = after_sentinel[..args_pos].trim().to_string();
                let after_args = &after_sentinel[args_pos + ARGS_SENTINEL.len()..];
                let json_end = after_args
                    .find(TOOL_CALLS_SENTINEL)
                    .unwrap_or(after_args.len());
                let json = after_args[..json_end].trim();
                (name, json, &after_args[json_end..])
            } else {
                // Format B: [TOOL_CALLS]name{json}
                match parse_format_b(after_sentinel) {
                    Ok(v) => v,
                    Err(msg) => return ParseResult::Error(msg),
                }
            }
        } else if has_json_start.is_some() {
            // No [ARGS] sentinel, but JSON found — Format B
            match parse_format_b(after_sentinel) {
                Ok(v) => v,
                Err(msg) => return ParseResult::Error(msg),
            }
        } else if has_args_sentinel.is_some() {
            // [ARGS] found but no JSON — malformed
            return ParseResult::Error("Found [ARGS] sentinel but no JSON arguments".to_string());
        } else {
            return ParseResult::Error("No arguments found after [TOOL_CALLS]".to_string());
        };

        if function_name.is_empty() {
            return ParseResult::Error("Empty function name after [TOOL_CALLS]".to_string());
        }

        if json_str.is_empty() {
            return ParseResult::Error(format!(
                "Empty arguments for tool call '{}'",
                function_name
            ));
        }

        match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(args) => {
                calls.push(ParsedToolCall {
                    name: function_name,
                    args,
                });
            }
            Err(e) => {
                return ParseResult::Error(format!(
                    "Invalid JSON for tool call '{}': {} (raw: {:?})",
                    function_name, e, json_str
                ));
            }
        }

        remaining = advance_to;
    }

    if calls.is_empty() {
        ParseResult::Error("Found [TOOL_CALLS] sentinel but parsed zero tool calls".to_string())
    } else {
        ParseResult::ToolCalls(calls)
    }
}

/// Parse Format B: `function_name{"param": "value"}` (no [ARGS] sentinel).
///
/// The function name is everything before the first `{`.
/// The JSON extends from `{` to the matching `}` (brace-balanced).
///
/// Returns `Ok((function_name, json_str, remaining))` on success. On failure
/// returns a plain error string; the caller wraps it into `ParseResult::Error`.
fn parse_format_b(after_sentinel: &str) -> std::result::Result<(String, &str, &str), String> {
    let json_start = after_sentinel
        .find('{')
        .ok_or_else(|| "No JSON found after function name".to_string())?;

    let function_name = after_sentinel[..json_start].trim().to_string();

    // Find the end of the JSON object by brace-balancing
    let json_region = &after_sentinel[json_start..];
    let json_end = find_balanced_brace(json_region).ok_or_else(|| {
        format!(
            "Unbalanced braces in tool call arguments for '{}'",
            function_name
        )
    })?;

    let json_str = json_region[..json_end].trim();
    let after_json = &json_region[json_end..];

    // Check for another [TOOL_CALLS] or end
    let advance_to = if let Some(next_tc) = after_json.find(TOOL_CALLS_SENTINEL) {
        &after_json[next_tc..]
    } else {
        // End of string
        &after_json[after_json.len()..]
    };

    Ok((function_name, json_str, advance_to))
}

/// Find the end of a delimiter-balanced span starting at `open` in `s`.
///
/// Returns the byte offset just past the matching `close`, or `None` if the
/// delimiters are unbalanced. Handles JSON strings: `open`/`close` chars that
/// appear inside `"..."` are ignored, and backslash-escapes inside strings
/// are honoured.
///
/// Caller is expected to position the slice so the first character is `open`;
/// the function does not validate that.
fn find_balanced(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Find the end of a bracket-balanced JSON array starting at `[`.
fn find_balanced_bracket(s: &str) -> Option<usize> {
    find_balanced(s, '[', ']')
}

/// Find the end of a brace-balanced JSON object starting at `{`.
fn find_balanced_brace(s: &str) -> Option<usize> {
    find_balanced(s, '{', '}')
}

// ---------------------------------------------------------------------------
// Streaming parser
// ---------------------------------------------------------------------------

/// State machine for parsing tool calls from a stream of tokens.
///
/// Tokens arrive one at a time and may split sentinels across boundaries.
/// The parser accumulates text and detects when the `[TOOL_CALLS]` sentinel
/// appears, switching into tool-call parsing mode.
#[derive(Debug)]
pub struct StreamingToolCallParser {
    /// Accumulated raw output from the model.
    buffer: String,
    /// Whether we have detected the `[TOOL_CALLS]` sentinel in the stream.
    in_tool_call_mode: bool,
    /// Text that was emitted as plain tokens before the sentinel was detected.
    /// If the sentinel never appears, all text is plain output.
    plain_prefix: String,
}

/// Events emitted by the streaming parser as tokens arrive.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// A plain text token that should be forwarded to the user.
    TextToken(String),
    /// A complete tool call was parsed from the accumulated buffer.
    ToolCall(ParsedToolCall),
    /// The buffer contains a partial sentinel that may complete with more tokens.
    /// The caller should NOT emit these characters as text yet.
    Buffering,
    /// Parsing completed (end of stream). Contains any remaining tool calls
    /// or an error if the buffer was malformed.
    Finished(ParseResult),
}

impl StreamingToolCallParser {
    /// Create a new streaming parser.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            in_tool_call_mode: false,
            plain_prefix: String::new(),
        }
    }

    /// Feed a token into the parser and get the resulting event.
    ///
    /// The caller should handle each `StreamEvent` appropriately:
    /// - `TextToken`: forward to the user as streaming text
    /// - `ToolCall`: a complete tool call was parsed
    /// - `Buffering`: the parser is accumulating a potential sentinel; hold output
    /// - `Finished`: end-of-stream finalization
    pub fn feed(&mut self, token: &str) -> StreamEvent {
        self.buffer.push_str(token);

        // If we're already in tool-call mode, just keep buffering.
        // Tool calls will be extracted on finish().
        if self.in_tool_call_mode {
            return StreamEvent::Buffering;
        }

        // Check if the buffer contains the complete sentinel
        if self.buffer.contains(TOOL_CALLS_SENTINEL) {
            self.in_tool_call_mode = true;
            // Extract any plain text before the sentinel
            if let Some(idx) = self.buffer.find(TOOL_CALLS_SENTINEL) {
                let prefix = self.buffer[..idx].to_string();
                if !prefix.is_empty() {
                    self.plain_prefix = prefix;
                }
            }
            return StreamEvent::Buffering;
        }

        // Check if the buffer ends with a partial sentinel prefix.
        // For example, if we've received "[TOOL" we should buffer rather than
        // emit those characters as text.
        if has_partial_sentinel_suffix(&self.buffer) {
            return StreamEvent::Buffering;
        }

        // No sentinel detected; emit everything in the buffer as text
        let text = self.buffer.clone();
        self.buffer.clear();
        StreamEvent::TextToken(text)
    }

    /// Signal end of stream and extract any remaining tool calls.
    pub fn finish(self) -> ParseResult {
        if !self.in_tool_call_mode {
            // No tool calls detected in the entire stream
            return ParseResult::PlainText(self.buffer);
        }

        // Parse the accumulated buffer for tool calls
        parse_tool_calls(&self.buffer)
    }
}

impl Default for StreamingToolCallParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if the string ends with a prefix of any sentinel marker.
///
/// This detects cases where a sentinel is split across token boundaries.
/// For example, if the buffer ends with "[TOOL" or "[TOOL_CA", we should
/// buffer rather than emit those characters as plain text.
fn has_partial_sentinel_suffix(text: &str) -> bool {
    // Check against both sentinels
    for sentinel in &[TOOL_CALLS_SENTINEL, ARGS_SENTINEL] {
        for prefix_len in 1..sentinel.len() {
            let sentinel_prefix = &sentinel[..prefix_len];
            if text.ends_with(sentinel_prefix) {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Format D: Ornith / Qwen3.5 XML tool-call parser
// ---------------------------------------------------------------------------

const XML_TOOL_CALL_OPEN: &str = "<tool_call>";
const XML_TOOL_CALL_CLOSE: &str = "</tool_call>";

/// Parse Ornith-style XML tool calls from a complete response string.
///
/// Handles the format emitted by Ornith 1.0 (Qwen3.5-based) when tools are
/// injected via the OAI-compat Jinja template path:
///
/// ```text
/// <tool_call>
/// <function=search_nodes>
/// <parameter=query>
/// Rust programming
/// </parameter>
/// </function>
/// </tool_call>
/// ```
///
/// Multiple `<tool_call>` blocks may appear consecutively. Parameter values are
/// plain text (potentially multi-line). Arguments are collected into a JSON
/// object `{"param": "value", ...}`.
///
/// Returns `ParseResult::PlainText` if no `<tool_call>` sentinel is present,
/// `ParseResult::ToolCalls` on success, or `ParseResult::Error` on malformed input.
pub fn parse_tool_calls_xml(text: &str) -> ParseResult {
    if !text.contains(XML_TOOL_CALL_OPEN) {
        return ParseResult::PlainText(text.to_string());
    }

    let mut calls = Vec::new();
    let mut search_from = 0;

    while let Some(open_pos) = text[search_from..].find(XML_TOOL_CALL_OPEN) {
        let abs_open = search_from + open_pos;
        let after_open = &text[abs_open + XML_TOOL_CALL_OPEN.len()..];

        let close_pos = match after_open.find(XML_TOOL_CALL_CLOSE) {
            Some(p) => p,
            None => {
                return ParseResult::Error("Unclosed <tool_call> block".to_string());
            }
        };

        let block = after_open[..close_pos].trim();

        match parse_xml_tool_call_block(block) {
            Ok(call) => calls.push(call),
            Err(msg) => return ParseResult::Error(msg),
        }

        search_from = abs_open + XML_TOOL_CALL_OPEN.len() + close_pos + XML_TOOL_CALL_CLOSE.len();
    }

    if calls.is_empty() {
        ParseResult::Error("Found <tool_call> sentinel but parsed zero tool calls".to_string())
    } else {
        ParseResult::ToolCalls(calls)
    }
}

/// Parse a single `<tool_call>` block's inner content.
///
/// Expected shape (whitespace-flexible):
/// ```text
/// <function=search_nodes>
/// <parameter=query>
/// value
/// </parameter>
/// </function>
/// ```
fn parse_xml_tool_call_block(block: &str) -> Result<ParsedToolCall, String> {
    let func_open = block
        .find("<function=")
        .ok_or_else(|| format!("No <function=...> in tool_call block: {:?}", block))?;
    let after_func_eq = &block[func_open + "<function=".len()..];
    let func_name_end = after_func_eq
        .find('>')
        .ok_or_else(|| "Unclosed <function=...> tag".to_string())?;
    let function_name = after_func_eq[..func_name_end].trim().to_string();

    if function_name.is_empty() {
        return Err("Empty function name in <function=> tag".to_string());
    }

    let func_body_start = func_open + "<function=".len() + func_name_end + 1;
    let func_body_end = block
        .find("</function>")
        .ok_or_else(|| format!("No </function> closing tag for '{}'", function_name))?;

    if func_body_end < func_body_start {
        return Err(format!(
            "</function> appeared before <function={}> body",
            function_name
        ));
    }

    let params_text = &block[func_body_start..func_body_end];

    let mut args = serde_json::Map::new();
    let mut param_search = 0;

    while let Some(p_open) = params_text[param_search..].find("<parameter=") {
        let abs_p_open = param_search + p_open;
        let after_p_eq = &params_text[abs_p_open + "<parameter=".len()..];
        let p_name_end = after_p_eq
            .find('>')
            .ok_or_else(|| "Unclosed <parameter=...> tag".to_string())?;
        let param_name = after_p_eq[..p_name_end].trim().to_string();

        let p_body_start = abs_p_open + "<parameter=".len() + p_name_end + 1;
        let p_close = params_text[p_body_start..]
            .find("</parameter>")
            .ok_or_else(|| format!("No </parameter> for parameter '{}'", param_name))?;
        let param_value = params_text[p_body_start..p_body_start + p_close]
            .trim()
            .to_string();

        args.insert(param_name, serde_json::Value::String(param_value));

        param_search = p_body_start + p_close + "</parameter>".len();
    }

    Ok(ParsedToolCall {
        name: function_name,
        args: serde_json::Value::Object(args),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // Complete-text parser tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_plain_text_no_tool_calls() {
        let result = parse_tool_calls("Hello, world! This is a plain response.");
        match result {
            ParseResult::PlainText(text) => {
                assert_eq!(text, "Hello, world! This is a plain response.");
            }
            other => panic!("Expected PlainText, got {:?}", other),
        }
    }

    #[test]
    fn test_single_tool_call() {
        let input = r#"[TOOL_CALLS]search_nodes[ARGS]{"query":"test"}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
                assert_eq!(calls[0].args, json!({"query": "test"}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_multiple_tool_calls() {
        let input = r#"[TOOL_CALLS]search_nodes[ARGS]{"query": "embeddings"}[TOOL_CALLS]search_nodes[ARGS]{"query": "vector search"}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].name, "search_nodes");
                assert_eq!(calls[0].args, json!({"query": "embeddings"}));
                assert_eq!(calls[1].name, "search_nodes");
                assert_eq!(calls[1].args, json!({"query": "vector search"}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_call_with_complex_args() {
        let input = r#"[TOOL_CALLS]create_node[ARGS]{"type":"task","title":"Buy groceries","priority":1,"tags":["food","errands"]}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "create_node");
                assert_eq!(
                    calls[0].args,
                    json!({
                        "type": "task",
                        "title": "Buy groceries",
                        "priority": 1,
                        "tags": ["food", "errands"]
                    })
                );
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_call_with_prefix_text() {
        let input = r#"Let me search for that. [TOOL_CALLS]search_nodes[ARGS]{"query":"test"}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_malformed_json() {
        let input = r#"[TOOL_CALLS]search_nodes[ARGS]{not valid json}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::Error(msg) => {
                assert!(msg.contains("Invalid JSON"), "Error was: {}", msg);
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_no_args_sentinel_parses_as_format_b() {
        // Without [ARGS], this is now valid Format B (Ministral 2512+)
        let input = "[TOOL_CALLS]search_nodes{\"query\":\"test\"}";
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
                assert_eq!(calls[0].args["query"], "test");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_empty_function_name() {
        let input = r#"[TOOL_CALLS][ARGS]{"query":"test"}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::Error(msg) => {
                assert!(msg.contains("Empty function name"), "Error was: {}", msg);
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_empty_args_with_sentinel() {
        // [ARGS] present but no JSON after it — [ARGS] comes before any `{`
        // so it's treated as Format A, but with empty args
        let input = "[TOOL_CALLS]search_nodes[ARGS]";
        let result = parse_tool_calls(input);
        match result {
            ParseResult::Error(msg) => {
                // Any error about missing/empty args is fine
                assert!(
                    msg.contains("arguments") || msg.contains("JSON"),
                    "Error was: {}",
                    msg
                );
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_empty_string() {
        let result = parse_tool_calls("");
        match result {
            ParseResult::PlainText(text) => {
                assert_eq!(text, "");
            }
            other => panic!("Expected PlainText, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_call_with_nested_json() {
        let input = r#"[TOOL_CALLS]update_node[ARGS]{"id":"node:123","changes":{"title":"Updated","metadata":{"priority":1,"nested":{"deep":true}}}}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "update_node");
                assert_eq!(
                    calls[0].args["changes"]["metadata"]["nested"]["deep"],
                    json!(true)
                );
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_call_with_whitespace() {
        let input = r#"[TOOL_CALLS] search_nodes [ARGS] {"query": "test"} "#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
                assert_eq!(calls[0].args, json!({"query": "test"}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_three_tool_calls() {
        let input =
            r#"[TOOL_CALLS]a[ARGS]{"x":1}[TOOL_CALLS]b[ARGS]{"y":2}[TOOL_CALLS]c[ARGS]{"z":3}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 3);
                assert_eq!(calls[0].name, "a");
                assert_eq!(calls[1].name, "b");
                assert_eq!(calls[2].name, "c");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Format C (Gemma 4 / OpenAI-style object after [TOOL_CALLS]) tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_gemma4_single_tool_call_object() {
        let input =
            r#"[TOOL_CALLS]{"tool_name":"search_nodes","tool_args":{"query":"embeddings"}}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
                assert_eq!(calls[0].args, json!({"query": "embeddings"}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_gemma4_object_with_leading_whitespace() {
        let input = "[TOOL_CALLS]   \n  {\"tool_name\":\"get_node\",\"tool_args\":{\"id\":\"n1\"}}";
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "get_node");
                assert_eq!(calls[0].args, json!({"id": "n1"}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_openai_style_object_after_sentinel() {
        // OpenAI-style payload: {"name": ..., "arguments": ...} as a bare object
        // following [TOOL_CALLS]. Format C dual-deserialization should accept it.
        let input = r#"[TOOL_CALLS]{"name":"create_node","arguments":{"type":"task","title":"X"}}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "create_node");
                assert_eq!(calls[0].args, json!({"type": "task", "title": "X"}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_gemma4_array_of_tool_calls() {
        let input = r#"[TOOL_CALLS][{"tool_name":"a","tool_args":{"x":1}},{"tool_name":"b","tool_args":{"y":2}}]"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].name, "a");
                assert_eq!(calls[0].args, json!({"x": 1}));
                assert_eq!(calls[1].name, "b");
                assert_eq!(calls[1].args, json!({"y": 2}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_gemma4_mixed_schema_array() {
        // Defensive: array contains one Mistral-style and one Gemma-style entry.
        let input = r#"[TOOL_CALLS][{"name":"a","arguments":{"x":1}},{"tool_name":"b","tool_args":{"y":2}}]"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].name, "a");
                assert_eq!(calls[1].name, "b");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_gemma4_object_with_nested_json_args() {
        let input = r#"[TOOL_CALLS]{"tool_name":"create_node","tool_args":{"type":"task","title":"Buy groceries","tags":["food","errands"]}}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "create_node");
                assert_eq!(
                    calls[0].args,
                    json!({
                        "type": "task",
                        "title": "Buy groceries",
                        "tags": ["food", "errands"]
                    })
                );
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_gemma4_object_followed_by_another_tool_call() {
        let input = r#"[TOOL_CALLS]{"tool_name":"a","tool_args":{"x":1}}[TOOL_CALLS]{"tool_name":"b","tool_args":{"y":2}}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].name, "a");
                assert_eq!(calls[1].name, "b");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_gemma4_object_unknown_schema_errors() {
        // Object after [TOOL_CALLS] that matches neither schema.
        let input = r#"[TOOL_CALLS]{"foo":"bar"}"#;
        let result = parse_tool_calls(input);
        assert!(matches!(result, ParseResult::Error(_)));
    }

    // -----------------------------------------------------------------------
    // Partial sentinel detection tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_partial_sentinel_suffix() {
        assert!(has_partial_sentinel_suffix("some text["));
        assert!(has_partial_sentinel_suffix("some text[T"));
        assert!(has_partial_sentinel_suffix("some text[TOOL"));
        assert!(has_partial_sentinel_suffix("some text[TOOL_"));
        assert!(has_partial_sentinel_suffix("some text[TOOL_CALLS"));
        // The full sentinel "[TOOL_CALLS]" is not a *partial* prefix — it's the
        // complete sentinel, handled by `contains(TOOL_CALLS_SENTINEL)` earlier
        // in the streaming parser.
        assert!(!has_partial_sentinel_suffix("some text[TOOL_CALLS]"));
        assert!(!has_partial_sentinel_suffix("some text"));
        assert!(!has_partial_sentinel_suffix("some text with brackets []"));
    }

    #[test]
    fn test_partial_args_sentinel_suffix() {
        assert!(has_partial_sentinel_suffix("fn_name[A"));
        assert!(has_partial_sentinel_suffix("fn_name[AR"));
        assert!(has_partial_sentinel_suffix("fn_name[ARG"));
        assert!(has_partial_sentinel_suffix("fn_name[ARGS"));
    }

    // -----------------------------------------------------------------------
    // Streaming parser tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_streaming_plain_text() {
        let mut parser = StreamingToolCallParser::new();

        let event1 = parser.feed("Hello");
        assert_eq!(event1, StreamEvent::TextToken("Hello".to_string()));

        let event2 = parser.feed(", world!");
        assert_eq!(event2, StreamEvent::TextToken(", world!".to_string()));

        let result = parser.finish();
        match result {
            ParseResult::PlainText(text) => {
                // After all tokens were emitted, remaining buffer is empty
                assert!(text.is_empty());
            }
            other => panic!("Expected PlainText, got {:?}", other),
        }
    }

    #[test]
    fn test_streaming_tool_call_single_token() {
        // Entire tool call arrives in one token
        let mut parser = StreamingToolCallParser::new();

        let event = parser.feed(r#"[TOOL_CALLS]search_nodes[ARGS]{"query":"test"}"#);
        assert_eq!(event, StreamEvent::Buffering);

        let result = parser.finish();
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
                assert_eq!(calls[0].args, json!({"query": "test"}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_streaming_tool_call_split_sentinel() {
        // The [TOOL_CALLS] sentinel is split across multiple tokens
        let mut parser = StreamingToolCallParser::new();

        let e1 = parser.feed("[TOOL");
        assert_eq!(e1, StreamEvent::Buffering, "Should buffer partial sentinel");

        let e2 = parser.feed("_CALLS]");
        assert_eq!(
            e2,
            StreamEvent::Buffering,
            "Should still buffer after sentinel completes"
        );

        let e3 = parser.feed("search_nodes");
        assert_eq!(e3, StreamEvent::Buffering);

        let e4 = parser.feed("[ARGS]");
        assert_eq!(e4, StreamEvent::Buffering);

        let e5 = parser.feed(r#"{"query":"test"}"#);
        assert_eq!(e5, StreamEvent::Buffering);

        let result = parser.finish();
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_streaming_text_then_tool_call() {
        let mut parser = StreamingToolCallParser::new();

        // Plain text first
        let e1 = parser.feed("Let me search");
        assert_eq!(e1, StreamEvent::TextToken("Let me search".to_string()));

        let e2 = parser.feed(" for that.");
        assert_eq!(e2, StreamEvent::TextToken(" for that.".to_string()));

        // Then tool call
        let e3 = parser.feed("[TOOL_CALLS]search_nodes[ARGS]");
        assert_eq!(e3, StreamEvent::Buffering);

        let e4 = parser.feed(r#"{"query":"test"}"#);
        assert_eq!(e4, StreamEvent::Buffering);

        let result = parser.finish();
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_streaming_partial_bracket_then_plain() {
        // Edge case: a "[" appears but it's not a sentinel
        let mut parser = StreamingToolCallParser::new();

        let e1 = parser.feed("array[0]");
        // The "[" at position 5 is a potential partial sentinel suffix for "[A"
        // but "array[0]" doesn't end with a sentinel prefix, so it should emit
        assert_eq!(e1, StreamEvent::TextToken("array[0]".to_string()));
    }

    #[test]
    fn test_streaming_partial_bracket_at_end() {
        let mut parser = StreamingToolCallParser::new();

        let e1 = parser.feed("some text[");
        assert_eq!(e1, StreamEvent::Buffering, "Trailing [ should buffer");

        // Next token completes a non-sentinel pattern
        let e2 = parser.feed("0]");
        // Now buffer is "some text[0]" which doesn't end with a sentinel prefix
        assert_eq!(e2, StreamEvent::TextToken("some text[0]".to_string()));
    }

    #[test]
    fn test_streaming_multiple_tool_calls() {
        let mut parser = StreamingToolCallParser::new();

        let input = r#"[TOOL_CALLS]a[ARGS]{"x":1}[TOOL_CALLS]b[ARGS]{"y":2}"#;
        let e1 = parser.feed(input);
        assert_eq!(e1, StreamEvent::Buffering);

        let result = parser.finish();
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].name, "a");
                assert_eq!(calls[1].name, "b");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_streaming_empty_stream() {
        let parser = StreamingToolCallParser::new();
        let result = parser.finish();
        match result {
            ParseResult::PlainText(text) => assert!(text.is_empty()),
            other => panic!("Expected PlainText, got {:?}", other),
        }
    }

    #[test]
    fn test_streaming_character_by_character() {
        // Feed the tool call character by character to stress-test boundary detection
        let mut parser = StreamingToolCallParser::new();
        let input = r#"[TOOL_CALLS]fn[ARGS]{"k":"v"}"#;

        let mut last_event = StreamEvent::Buffering;
        for ch in input.chars() {
            last_event = parser.feed(&ch.to_string());
        }

        // After feeding all characters, we should be in buffering mode
        assert_eq!(last_event, StreamEvent::Buffering);

        let result = parser.finish();
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "fn");
                assert_eq!(calls[0].args, json!({"k": "v"}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_call_with_string_containing_brackets() {
        // JSON args contain bracket characters that look like sentinels
        let input = r#"[TOOL_CALLS]search_nodes[ARGS]{"query":"array[0] and list[1]"}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].args["query"], "array[0] and list[1]");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_tool_call_with_empty_object_args() {
        let input = r#"[TOOL_CALLS]list_tools[ARGS]{}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "list_tools");
                assert_eq!(calls[0].args, json!({}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Format B tests (Ministral 2512+ — no [ARGS] sentinel)
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_b_real_model_output() {
        // Actual output from Ministral 3B/8B 2512 model
        let input = r#"[TOOL_CALLS]search_nodes{"query": "task", "node_type": "task"}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
                assert_eq!(calls[0].args["query"], "task");
                assert_eq!(calls[0].args["node_type"], "task");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_format_b_simple() {
        let input = r#"[TOOL_CALLS]get_node{"node_id": "abc-123"}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "get_node");
                assert_eq!(calls[0].args["node_id"], "abc-123");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_format_b_nested_json() {
        let input = r#"[TOOL_CALLS]create_node{"title": "Test", "properties": {"priority": 1, "tags": ["a", "b"]}}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "create_node");
                assert_eq!(calls[0].args["properties"]["priority"], 1);
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_format_b_with_prefix_text() {
        let input = r#"Let me search for that. [TOOL_CALLS]search_nodes{"query": "test"}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_format_b_empty_args() {
        let input = r#"[TOOL_CALLS]list_all{}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "list_all");
                assert_eq!(calls[0].args, json!({}));
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_format_b_json_with_braces_in_strings() {
        let input = r#"[TOOL_CALLS]search_nodes{"query": "find {important} nodes"}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].args["query"], "find {important} nodes");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn test_format_b_streaming() {
        // Simulate streaming: [TOOL_CALLS] control token injected, then name + json
        let mut parser = StreamingToolCallParser::new();

        let e1 = parser.feed("[TOOL_CALLS]");
        assert_eq!(e1, StreamEvent::Buffering);

        let e2 = parser.feed("search_nodes");
        assert_eq!(e2, StreamEvent::Buffering);

        let e3 = parser.feed(r#"{"query": "test"}"#);
        assert_eq!(e3, StreamEvent::Buffering);

        let result = parser.finish();
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
                assert_eq!(calls[0].args["query"], "test");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Brace balancing tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_balanced_brace_simple() {
        assert_eq!(find_balanced_brace(r#"{"a": 1}"#), Some(8));
    }

    #[test]
    fn test_balanced_brace_nested() {
        assert_eq!(find_balanced_brace(r#"{"a": {"b": 1}}"#), Some(15));
    }

    #[test]
    fn test_balanced_brace_with_string() {
        assert_eq!(find_balanced_brace(r#"{"a": "}"}"#), Some(10));
    }

    #[test]
    fn test_balanced_brace_unbalanced() {
        assert_eq!(find_balanced_brace(r#"{"a": 1"#), None);
    }

    // -----------------------------------------------------------------------
    // Regression fixtures — historical engine failures (issue #1355)
    //
    // These tests pin the two tool-call failures that were misattributed to
    // model capability but were caused by the old vendored llama.cpp engine
    // (pre-build-8660). The upgraded engine (llama-cpp-2 ≥ 0.1.146, vendored
    // at a commit 123 commits past b8660) produces correct output; the tests
    // confirm our parser handles both the broken and fixed shapes correctly.
    // -----------------------------------------------------------------------

    /// Ministral 8B regression: the old engine dropped the `type` key from a
    /// JSON object field, producing malformed JSON:
    ///   {"name":"amount_due","number"}
    /// — a bare string value after a comma is not valid JSON. The parser must
    /// reject this rather than silently swallowing it.
    ///
    /// With the upgraded engine the field is emitted correctly as:
    ///   {"name":"amount_due","type":"number"}
    #[test]
    fn regression_ministral8b_dropped_type_key_is_rejected() {
        // This is the exact malformed shape the old engine produced for a
        // create_schema field definition inside a [TOOL_CALLS] response.
        let broken = r#"[TOOL_CALLS]create_schema[ARGS]{"name":"Invoice","fields":[{"name":"amount_due","number"}]}"#;
        let result = parse_tool_calls(broken);
        assert!(
            matches!(result, ParseResult::Error(_)),
            "Malformed JSON with dropped 'type' key must be a parse Error, got: {:?}",
            result
        );
    }

    /// Ministral 8B regression (fixed): the upgraded engine now emits the
    /// complete, valid `create_schema` call with both `name` and `type` keys.
    /// This is the shape the fixed engine produces — the parser must accept it.
    #[test]
    fn regression_ministral8b_complete_create_schema_accepted() {
        let fixed = r#"[TOOL_CALLS]create_schema[ARGS]{"name":"Invoice","fields":[{"name":"amount_due","type":"number","description":"Total amount due"}]}"#;
        let result = parse_tool_calls(fixed);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "create_schema");
                assert_eq!(calls[0].args["name"], "Invoice");
                let fields = calls[0].args["fields"].as_array().expect("fields array");
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0]["name"], "amount_due");
                assert_eq!(fields[0]["type"], "number");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    /// Ministral 8B streaming regression: the dropped-type failure also
    /// occurred mid-stream. Verify the streaming parser surfaces a parse
    /// Error (not a crash) on the malformed shape.
    #[test]
    fn regression_ministral8b_streaming_broken_json_errors_cleanly() {
        let mut parser = StreamingToolCallParser::new();
        // Feed all at once — the sentinel is present so we enter tool-call mode.
        parser.feed(r#"[TOOL_CALLS]create_schema[ARGS]{"name":"Invoice","fields":[{"name":"amount_due","number"}]}"#);
        let result = parser.finish();
        assert!(
            matches!(result, ParseResult::Error(_)),
            "Streaming parser must surface Error for malformed JSON, got: {:?}",
            result
        );
    }

    /// Gemma 4 12B regression: the `[TOOL_CALLS]` Mistral-format parser must
    /// continue to handle Gemma 4 Format-C (JSON object after sentinel) exactly
    /// as it did before the engine upgrade. The upgrade must not break this path.
    #[test]
    fn regression_gemma4_mistral_sentinel_format_still_works() {
        // Gemma 4 can also emit via the [TOOL_CALLS] sentinel path (Format C).
        // Confirm parser still handles it correctly post-upgrade.
        let input = r#"[TOOL_CALLS]{"tool_name":"create_schema","tool_args":{"name":"Invoice","fields":[{"name":"amount_due","type":"number"}]}}"#;
        let result = parse_tool_calls(input);
        match result {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "create_schema");
                assert_eq!(calls[0].args["name"], "Invoice");
                let fields = calls[0].args["fields"].as_array().expect("fields array");
                assert_eq!(fields[0]["type"], "number");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Format D: Ornith / Qwen3.5 XML tool-call parser
    // -----------------------------------------------------------------------

    #[test]
    fn xml_single_call() {
        let input = "<tool_call>\n<function=search_nodes>\n<parameter=query>\nRust programming\n</parameter>\n</function>\n</tool_call>";
        match parse_tool_calls_xml(input) {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "search_nodes");
                assert_eq!(calls[0].args["query"], "Rust programming");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn xml_multiple_calls() {
        let input = "<tool_call>\n<function=get_node>\n<parameter=node_id>\nnode_001\n</parameter>\n</function>\n</tool_call><tool_call>\n<function=get_node>\n<parameter=node_id>\nnode_002\n</parameter>\n</function>\n</tool_call>";
        match parse_tool_calls_xml(input) {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].name, "get_node");
                assert_eq!(calls[0].args["node_id"], "node_001");
                assert_eq!(calls[1].name, "get_node");
                assert_eq!(calls[1].args["node_id"], "node_002");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn xml_missing_close_tool_call() {
        let input = "<tool_call>\n<function=search_nodes>\n<parameter=query>\nRust\n</parameter>\n</function>";
        let result = parse_tool_calls_xml(input);
        assert!(
            matches!(result, ParseResult::Error(_)),
            "Expected Error for unclosed <tool_call>, got: {:?}",
            result
        );
    }

    #[test]
    fn xml_missing_close_parameter() {
        let input = "<tool_call>\n<function=search_nodes>\n<parameter=query>\nRust programming\n</function>\n</tool_call>";
        let result = parse_tool_calls_xml(input);
        assert!(
            matches!(result, ParseResult::Error(_)),
            "Expected Error for unclosed <parameter>, got: {:?}",
            result
        );
    }

    #[test]
    fn xml_empty_parameter_body() {
        let input = "<tool_call>\n<function=get_children>\n<parameter=node_id>\n</parameter>\n</function>\n</tool_call>";
        match parse_tool_calls_xml(input) {
            ParseResult::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "get_children");
                assert_eq!(calls[0].args["node_id"], "");
            }
            other => panic!("Expected ToolCalls, got {:?}", other),
        }
    }

    #[test]
    fn xml_no_sentinel_is_plain_text() {
        let input = "This is a plain response with no tool call.";
        assert_eq!(
            parse_tool_calls_xml(input),
            ParseResult::PlainText(input.to_string())
        );
    }

    #[test]
    fn xml_no_function_tag_is_error() {
        let input = "<tool_call>\nnot a function block\n</tool_call>";
        let result = parse_tool_calls_xml(input);
        assert!(
            matches!(result, ParseResult::Error(_)),
            "Expected Error for missing <function=...>, got: {:?}",
            result
        );
    }
}
