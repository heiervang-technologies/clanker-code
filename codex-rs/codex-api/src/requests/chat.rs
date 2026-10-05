//! Translation from a Responses API request into a legacy Chat Completions
//! request (`POST /chat/completions`).
//!
//! The rest of the client builds a [`ResponsesApiRequest`] regardless of the
//! provider wire format. Providers configured with `wire_api = "chat"` reuse
//! that request and translate it here, which keeps prompt assembly, tool
//! selection and history management in a single place.
//!
//! Chat Completions only understands function tools, so Responses-only tool
//! shapes are adapted:
//!
//! - namespaced function tools are flattened into a single function name;
//! - freeform (`custom`) tools such as grammar-based `apply_patch` become a
//!   function with a single string `input` argument;
//! - hosted tools (`web_search`, `tool_search`, `image_generation`, ...) are
//!   dropped because a generic Chat Completions server cannot execute them.
//!
//! [`ChatToolNames`] records how each advertised function name maps back to the
//! original tool so streamed tool calls can be turned into the right
//! [`ResponseItem`] variant.

use crate::common::ResponsesApiRequest;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;
use codex_protocol::models::plaintext_agent_message_content;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;

/// Name of the single string argument used when exposing a freeform tool as a
/// Chat Completions function.
pub(crate) const FREEFORM_INPUT_ARGUMENT: &str = "input";

/// How a Chat Completions function name maps back to a Codex tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChatTool {
    Function {
        namespace: Option<String>,
        name: String,
    },
    Freeform {
        name: String,
    },
}

/// Lookup table from advertised Chat Completions function names to tools.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ChatToolNames {
    by_wire_name: HashMap<String, ChatTool>,
}

impl ChatToolNames {
    pub(crate) fn resolve(&self, wire_name: &str) -> ChatTool {
        self.by_wire_name
            .get(wire_name)
            .cloned()
            .unwrap_or_else(|| ChatTool::Function {
                namespace: None,
                name: wire_name.to_string(),
            })
    }

    fn insert(&mut self, wire_name: String, tool: ChatTool) {
        self.by_wire_name.insert(wire_name, tool);
    }
}

/// Provider-specific knobs for building a Chat Completions request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatRequestOptions {
    /// When the requested reasoning effort is `none` or `minimal`, also send
    /// `chat_template_kwargs: {"enable_thinking": false}`. Open-weight servers
    /// (llama.cpp, vLLM, SGLang) toggle thinking through the chat template and
    /// ignore `reasoning_effort`; OpenAI rejects the unknown field.
    pub thinking_template_kwargs: bool,
    /// Extra top-level body fields, deep-merged into the request last. A
    /// `null` value removes the field.
    pub extra_body: Option<Map<String, Value>>,
    /// Keep the audio of only the newest N `input_audio` parts; older ones
    /// become a short text note. `None` keeps all.
    pub max_audio_inputs: Option<usize>,
}

/// A Chat Completions request body plus the tool name mapping needed to decode
/// the streamed response.
#[derive(Debug, Clone)]
pub(crate) struct ChatCompletionsRequest {
    pub body: Value,
    pub tool_names: ChatToolNames,
}

/// Builds the Chat Completions body for `request`.
pub(crate) fn build_chat_completions_request(
    request: &ResponsesApiRequest,
    options: &ChatRequestOptions,
) -> ChatCompletionsRequest {
    let mut tool_names = ChatToolNames::default();
    let mut tools = Vec::new();
    for tool in request.tools.iter().flatten() {
        push_chat_tools(tool, &mut tools, &mut tool_names);
    }
    for item in &request.input {
        if let ResponseItem::AdditionalTools {
            tools: extra_tools, ..
        } = item
        {
            for tool in extra_tools {
                push_chat_tools(tool, &mut tools, &mut tool_names);
            }
        }
    }

    let mut messages = build_messages(&request.instructions, &request.input);
    scrub_control_chars(&mut messages);
    if let Some(keep) = options.max_audio_inputs {
        keep_newest_audio(&mut messages, keep);
    }

    let mut body = Map::new();
    body.insert("model".to_string(), json!(request.model));
    body.insert("messages".to_string(), Value::Array(messages));
    body.insert("stream".to_string(), json!(true));
    body.insert("stream_options".to_string(), json!({"include_usage": true}));
    if !tools.is_empty() {
        body.insert("tools".to_string(), Value::Array(tools));
        body.insert("tool_choice".to_string(), json!(request.tool_choice));
        body.insert(
            "parallel_tool_calls".to_string(),
            json!(request.parallel_tool_calls),
        );
    }
    if let Some(effort) = request
        .reasoning
        .as_ref()
        .and_then(|reasoning| reasoning.effort.as_ref())
        && let Ok(Value::String(effort)) = serde_json::to_value(effort)
    {
        if options.thinking_template_kwargs && matches!(effort.as_str(), "none" | "minimal") {
            body.insert(
                "chat_template_kwargs".to_string(),
                json!({"enable_thinking": false}),
            );
        }
        body.insert("reasoning_effort".to_string(), json!(effort));
    }
    if let Some(format) = request.text.as_ref().and_then(|text| text.format.as_ref()) {
        body.insert(
            "response_format".to_string(),
            json!({
                "type": "json_schema",
                "json_schema": {
                    "name": format.name,
                    "schema": format.schema,
                    "strict": format.strict,
                }
            }),
        );
    }
    if let Some(verbosity) = request
        .text
        .as_ref()
        .and_then(|text| text.verbosity.as_ref())
        && let Ok(verbosity) = serde_json::to_value(verbosity)
    {
        body.insert("verbosity".to_string(), verbosity);
    }
    if let Some(service_tier) = &request.service_tier {
        body.insert("service_tier".to_string(), json!(service_tier));
    }
    if let Some(prompt_cache_key) = &request.prompt_cache_key {
        body.insert("prompt_cache_key".to_string(), json!(prompt_cache_key));
    }

    if let Some(extra_body) = &options.extra_body {
        merge_json_object(&mut body, extra_body);
    }

    ChatCompletionsRequest {
        body: Value::Object(body),
        tool_names,
    }
}

/// Text shown to the model in place of audio dropped by `max_audio_inputs`.
pub(crate) const AUDIO_DROPPED_NOTE: &str = "(an earlier voice message; its audio is no longer attached)";

/// Replaces all but the newest `keep` `input_audio` parts with a text note.
fn keep_newest_audio(messages: &mut [Value], keep: usize) {
    let mut seen = 0usize;
    for message in messages.iter_mut().rev() {
        let Some(parts) = message.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        for part in parts.iter_mut().rev() {
            if part.get("type").and_then(Value::as_str) != Some("input_audio") {
                continue;
            }
            seen += 1;
            if seen > keep {
                *part = json!({"type": "text", "text": AUDIO_DROPPED_NOTE});
            }
        }
    }
}

/// Removes C0 control characters other than tab, newline and carriage return
/// from message text. Binary tool output (for example `head` on a PNG) carries
/// NULs that are useless to the model, and llama.cpp's multimodal tokenizer
/// rejects a prompt containing them once audio or images are attached.
fn scrub_control_chars(messages: &mut [Value]) {
    fn clean(text: &mut String) {
        if text
            .chars()
            .any(|c| c.is_ascii_control() && !matches!(c, '\t' | '\n' | '\r' | '\u{7f}'))
        {
            text.retain(|c| !c.is_ascii_control() || matches!(c, '\t' | '\n' | '\r' | '\u{7f}'));
        }
    }
    for message in messages.iter_mut() {
        match message.get_mut("content") {
            Some(Value::String(text)) => clean(text),
            Some(Value::Array(parts)) => {
                for part in parts.iter_mut() {
                    if let Some(Value::String(text)) = part.get_mut("text") {
                        clean(text);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Deep-merges `overlay` into `target`: nested objects merge, `null` removes a
/// key, and any other value replaces the existing one.
fn merge_json_object(target: &mut Map<String, Value>, overlay: &Map<String, Value>) {
    for (key, value) in overlay {
        match (target.get_mut(key), value) {
            (_, Value::Null) => {
                target.remove(key);
            }
            (Some(Value::Object(existing)), Value::Object(nested)) => {
                merge_json_object(existing, nested);
            }
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Function name advertised for a tool that lives in a Responses namespace.
fn flattened_function_name(namespace: &str, name: &str) -> String {
    if namespace.ends_with('_') {
        format!("{namespace}{name}")
    } else {
        format!("{namespace}__{name}")
    }
}

fn wire_function_name(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(namespace) if !namespace.is_empty() => flattened_function_name(namespace, name),
        _ => name.to_string(),
    }
}

fn push_chat_tools(tool: &Value, out: &mut Vec<Value>, names: &mut ChatToolNames) {
    let Some(tool_type) = tool.get("type").and_then(Value::as_str) else {
        return;
    };
    match tool_type {
        "function" => {
            if let Some((wire_name, function)) = chat_function_tool(tool, /*namespace*/ None) {
                names.insert(
                    wire_name,
                    ChatTool::Function {
                        namespace: None,
                        name: function_name(tool).to_string(),
                    },
                );
                out.push(function);
            }
        }
        "namespace" => {
            let namespace = tool.get("name").and_then(Value::as_str).unwrap_or_default();
            for nested in tool
                .get("tools")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if nested.get("type").and_then(Value::as_str) != Some("function") {
                    continue;
                }
                if let Some((wire_name, function)) = chat_function_tool(nested, Some(namespace)) {
                    names.insert(
                        wire_name,
                        ChatTool::Function {
                            namespace: Some(namespace.to_string()),
                            name: function_name(nested).to_string(),
                        },
                    );
                    out.push(function);
                }
            }
        }
        "custom" => {
            let Some(name) = tool.get("name").and_then(Value::as_str) else {
                return;
            };
            let mut description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Some(format) = tool.get("format") {
                let syntax = format
                    .get("syntax")
                    .and_then(Value::as_str)
                    .unwrap_or("text");
                if let Some(definition) = format.get("definition").and_then(Value::as_str) {
                    description.push_str(&format!(
                        "\n\nPass the raw tool input as the `{FREEFORM_INPUT_ARGUMENT}` string argument. \
                         It must conform to this {syntax} grammar:\n{definition}"
                    ));
                }
            }
            names.insert(
                name.to_string(),
                ChatTool::Freeform {
                    name: name.to_string(),
                },
            );
            out.push(json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": description,
                    "parameters": {
                        "type": "object",
                        "properties": {
                            FREEFORM_INPUT_ARGUMENT: {
                                "type": "string",
                                "description": "Raw freeform input for the tool."
                            }
                        },
                        "required": [FREEFORM_INPUT_ARGUMENT],
                        "additionalProperties": false
                    }
                }
            }));
        }
        // Hosted tools (web_search, tool_search, image_generation, local_shell,
        // ...) have no Chat Completions equivalent.
        _ => {}
    }
}

fn function_name(tool: &Value) -> &str {
    tool.get("name").and_then(Value::as_str).unwrap_or_default()
}

fn chat_function_tool(tool: &Value, namespace: Option<&str>) -> Option<(String, Value)> {
    let name = tool.get("name").and_then(Value::as_str)?;
    let wire_name = wire_function_name(namespace, name);
    let mut function = Map::new();
    function.insert("name".to_string(), json!(wire_name));
    if let Some(description) = tool.get("description") {
        function.insert("description".to_string(), description.clone());
    }
    function.insert(
        "parameters".to_string(),
        tool.get("parameters")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
    );
    if tool.get("strict").and_then(Value::as_bool) == Some(true) {
        function.insert("strict".to_string(), json!(true));
    }
    Some((
        wire_name,
        json!({"type": "function", "function": Value::Object(function)}),
    ))
}

/// Accumulates Chat Completions messages, merging adjacent assistant tool calls
/// and deferring images returned by tools until the tool results are complete.
#[derive(Default)]
struct MessageBuilder {
    messages: Vec<Value>,
    /// Reasoning text that should be attached to the next assistant message.
    pending_reasoning: String,
    /// Image parts returned by tool calls. Chat Completions only allows text in
    /// `tool` messages, so these are replayed as a user message once the tool
    /// results for the current assistant turn have been emitted.
    pending_tool_images: Vec<Value>,
}

impl MessageBuilder {
    fn flush_tool_images(&mut self) {
        if self.pending_tool_images.is_empty() {
            return;
        }
        let mut content = vec![json!({
            "type": "text",
            "text": "Images returned by the preceding tool calls:"
        })];
        content.append(&mut self.pending_tool_images);
        self.messages
            .push(json!({"role": "user", "content": content}));
    }

    fn take_reasoning(&mut self) -> Option<String> {
        if self.pending_reasoning.trim().is_empty() {
            self.pending_reasoning.clear();
            None
        } else {
            Some(std::mem::take(&mut self.pending_reasoning))
        }
    }

    fn push_message(&mut self, role: &str, content: Value) {
        self.flush_tool_images();
        let mut message = json!({"role": role, "content": content});
        // Reasoning only belongs to the assistant message it preceded; drop
        // reasoning from an interrupted turn instead of attaching it to a later
        // answer.
        if let Some(reasoning) = self.take_reasoning()
            && role == "assistant"
        {
            message["reasoning_content"] = json!(reasoning);
        }
        self.messages.push(message);
    }

    fn push_tool_call(&mut self, id: &str, name: &str, arguments: &str) {
        let tool_call = json!({
            "id": id,
            "type": "function",
            "function": {"name": name, "arguments": arguments},
        });
        let reasoning = self.take_reasoning();
        if self.pending_tool_images.is_empty()
            && let Some(Value::Object(last)) = self.messages.last_mut()
            && last.get("role").and_then(Value::as_str) == Some("assistant")
        {
            if let Some(tool_calls) = last.get_mut("tool_calls").and_then(Value::as_array_mut) {
                tool_calls.push(tool_call);
            } else {
                last.insert("tool_calls".to_string(), json!([tool_call]));
            }
            if let Some(reasoning) = reasoning {
                match last.get_mut("reasoning_content") {
                    Some(Value::String(existing)) => {
                        if !existing.is_empty() {
                            existing.push('\n');
                        }
                        existing.push_str(&reasoning);
                    }
                    _ => {
                        last.insert("reasoning_content".to_string(), json!(reasoning));
                    }
                }
            }
            return;
        }

        self.flush_tool_images();
        let mut message = json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [tool_call],
        });
        if let Some(reasoning) = reasoning {
            message["reasoning_content"] = json!(reasoning);
        }
        self.messages.push(message);
    }

    fn push_tool_output(&mut self, call_id: &str, output: &FunctionCallOutputPayload) {
        let content = match &output.body {
            FunctionCallOutputBody::Text(text) => text.clone(),
            FunctionCallOutputBody::ContentItems(items) => {
                let mut texts = Vec::new();
                for item in items {
                    match item {
                        FunctionCallOutputContentItem::InputText { text } => {
                            texts.push(text.as_str())
                        }
                        FunctionCallOutputContentItem::InputImage { image_url, .. } => {
                            self.pending_tool_images.push(media_part(image_url));
                        }
                        FunctionCallOutputContentItem::EncryptedContent { .. } => {}
                    }
                }
                texts.join("\n")
            }
        };
        self.messages.push(json!({
            "role": "tool",
            "tool_call_id": call_id,
            "content": content,
        }));
    }

    fn finish(mut self) -> Vec<Value> {
        self.flush_tool_images();
        self.messages
    }
}

/// Converts an image URL into a Chat Completions content part.
///
/// Audio carried as a `data:audio/<format>;base64,...` URL is sent as an
/// `input_audio` part so audio-capable models (for example Gemma) can hear it.
fn media_part(url: &str) -> Value {
    if let Some((format, data)) = parse_audio_data_url(url) {
        return json!({
            "type": "input_audio",
            "input_audio": {"data": data, "format": format},
        });
    }
    json!({"type": "image_url", "image_url": {"url": url}})
}

fn parse_audio_data_url(url: &str) -> Option<(String, &str)> {
    let rest = url.strip_prefix("data:audio/")?;
    let (mime_params, data) = rest.split_once(',')?;
    let mut params = mime_params.split(';');
    let subtype = params.next()?.to_ascii_lowercase();
    if !params.any(|param| param.eq_ignore_ascii_case("base64")) {
        return None;
    }
    let format = match subtype.as_str() {
        "mpeg" | "mp3" => "mp3",
        "wav" | "wave" | "x-wav" | "vnd.wave" => "wav",
        other => other,
    };
    Some((format.to_string(), data))
}

fn build_messages(instructions: &str, input: &[ResponseItem]) -> Vec<Value> {
    let mut builder = MessageBuilder::default();

    // Chat templates for many open models only accept a single leading system
    // message, so fold the base instructions and any leading developer/system
    // messages together.
    let mut system_parts = Vec::new();
    if !instructions.trim().is_empty() {
        system_parts.push(instructions.to_string());
    }
    let mut start = 0;
    for item in input {
        match item {
            ResponseItem::Message { role, content, .. }
                if role == "system" || role == "developer" =>
            {
                let text = message_text(content);
                if !text.is_empty() {
                    system_parts.push(text);
                }
                start += 1;
            }
            ResponseItem::AdditionalTools { .. } => start += 1,
            _ => break,
        }
    }
    if !system_parts.is_empty() {
        builder
            .messages
            .push(json!({"role": "system", "content": system_parts.join("\n\n")}));
    }

    for item in &input[start..] {
        match item {
            ResponseItem::Message { role, content, .. } => {
                let role = match role.as_str() {
                    // Mid-conversation developer messages are not universally
                    // supported; deliver them as system messages.
                    "developer" => "system",
                    other => other,
                };
                if role == "assistant" {
                    builder.push_message(role, json!(message_text(content)));
                } else if content
                    .iter()
                    .any(|part| matches!(part, ContentItem::InputImage { .. }))
                {
                    let parts = content
                        .iter()
                        .map(|part| match part {
                            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                                json!({"type": "text", "text": text})
                            }
                            ContentItem::InputImage { image_url, .. } => media_part(image_url),
                        })
                        .collect::<Vec<_>>();
                    builder.push_message(role, Value::Array(parts));
                } else {
                    builder.push_message(role, json!(message_text(content)));
                }
            }
            ResponseItem::AgentMessage {
                author, content, ..
            } => {
                if let Some(text) = plaintext_agent_message_content(content) {
                    builder.push_message("user", json!(format!("[message from {author}]\n{text}")));
                }
            }
            ResponseItem::Reasoning { content, .. } => {
                for part in content.iter().flatten() {
                    match part {
                        ReasoningItemContent::ReasoningText { text }
                        | ReasoningItemContent::Text { text } => {
                            builder.pending_reasoning.push_str(text);
                        }
                    }
                }
            }
            ResponseItem::FunctionCall {
                name,
                namespace,
                arguments,
                call_id,
                ..
            } => {
                let wire_name = wire_function_name(namespace.as_deref(), name);
                builder.push_tool_call(call_id, &wire_name, arguments);
            }
            ResponseItem::CustomToolCall {
                call_id,
                name,
                input,
                ..
            } => {
                let arguments = json!({ FREEFORM_INPUT_ARGUMENT: input }).to_string();
                builder.push_tool_call(call_id, name, &arguments);
            }
            ResponseItem::LocalShellCall {
                id,
                call_id,
                action,
                ..
            } => {
                let call_id = call_id
                    .clone()
                    .or_else(|| id.as_ref().map(ToString::to_string))
                    .unwrap_or_default();
                let arguments = serde_json::to_string(action).unwrap_or_default();
                builder.push_tool_call(&call_id, "local_shell", &arguments);
            }
            ResponseItem::FunctionCallOutput {
                call_id, output, ..
            }
            | ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => builder.push_tool_output(call_id, output),
            ResponseItem::AdditionalTools { .. }
            | ResponseItem::ToolSearchCall { .. }
            | ResponseItem::ToolSearchOutput { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::ImageGenerationCall { .. }
            | ResponseItem::Compaction { .. }
            | ResponseItem::CompactionTrigger { .. }
            | ResponseItem::ContextCompaction { .. }
            | ResponseItem::Other => {}
        }
    }

    builder.finish()
}

fn message_text(content: &[ContentItem]) -> String {
    content
        .iter()
        .filter_map(|part| match part {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                Some(text.as_str())
            }
            ContentItem::InputImage { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
#[path = "chat_tests.rs"]
pub(crate) mod tests;
