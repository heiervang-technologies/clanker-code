//! Streaming parser for the legacy Chat Completions API.
//!
//! Chat Completions streams `chat.completion.chunk` objects whose `choices[0].delta`
//! carries assistant text, optional reasoning text (`reasoning_content` or
//! `reasoning`, depending on the server) and incremental tool calls. This module
//! converts those chunks into the same [`ResponseEvent`]s produced by the
//! Responses API parser so the rest of the client is wire-agnostic.

use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::rate_limits::parse_all_rate_limits;
use crate::requests::chat::ChatTool;
use crate::requests::chat::ChatToolNames;
use crate::requests::chat::FREEFORM_INPUT_ARGUMENT;
use crate::telemetry::SseTelemetry;
use codex_client::ByteStream;
use codex_client::StreamResponse;
use codex_protocol::ResponseItemId;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ReasoningItemContent;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;

use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

const REQUEST_ID_HEADER: &str = "x-request-id";

pub(crate) fn spawn_chat_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    tool_names: ChatToolNames,
) -> ResponseStream {
    let rate_limit_snapshots = parse_all_rate_limits(&stream_response.headers);
    let upstream_request_id = stream_response
        .headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(async move {
        for snapshot in rate_limit_snapshots {
            let _ = tx_event.send(Ok(ResponseEvent::RateLimits(snapshot))).await;
        }
        process_chat_sse(
            stream_response.bytes,
            tx_event,
            idle_timeout,
            telemetry,
            tool_names,
        )
        .await;
    });
    ResponseStream {
        rx_event,
        upstream_request_id,
    }
}

#[derive(Debug, Default)]
struct ToolCallState {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

/// Incremental state for one streamed Chat Completions response.
struct ChatStreamState {
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    tool_names: ChatToolNames,
    response_id: String,
    created_sent: bool,
    /// Reasoning item currently streaming (id, accumulated text).
    reasoning: Option<(ResponseItemId, String)>,
    /// Assistant message currently streaming (id, accumulated text).
    message: Option<(ResponseItemId, String)>,
    /// Tool calls keyed by their `index` in the chunk stream.
    tool_calls: BTreeMap<u64, ToolCallState>,
    token_usage: Option<TokenUsage>,
}

impl ChatStreamState {
    async fn send(&self, event: ResponseEvent) -> bool {
        self.tx_event.send(Ok(event)).await.is_ok()
    }

    async fn ensure_created(&mut self) {
        if !self.created_sent {
            self.created_sent = true;
            self.send(ResponseEvent::Created).await;
        }
    }

    async fn append_reasoning(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        // The core turn loop tracks a single active streamed item, so close the
        // assistant message before starting a new reasoning item.
        self.finish_message().await;
        if self.reasoning.is_none() {
            let id = ResponseItemId::new("rs");
            self.send(ResponseEvent::OutputItemAdded(reasoning_item(
                id.clone(),
                String::new(),
            )))
            .await;
            self.reasoning = Some((id, String::new()));
        }
        if let Some((_, accumulated)) = self.reasoning.as_mut() {
            accumulated.push_str(text);
        }
        self.send(ResponseEvent::ReasoningContentDelta {
            delta: text.to_string(),
            content_index: 0,
        })
        .await;
    }

    async fn append_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.finish_reasoning().await;
        if self.message.is_none() {
            let id = ResponseItemId::new("msg");
            self.send(ResponseEvent::OutputItemAdded(assistant_message(
                id.clone(),
                String::new(),
            )))
            .await;
            self.message = Some((id, String::new()));
        }
        if let Some((_, accumulated)) = self.message.as_mut() {
            accumulated.push_str(text);
        }
        self.send(ResponseEvent::OutputTextDelta(text.to_string()))
            .await;
    }

    fn append_tool_call(&mut self, tool_call: &Value, position: usize) {
        let index = tool_call
            .get("index")
            .and_then(Value::as_u64)
            .or_else(|| {
                // Some servers omit `index`; fall back to matching by id, then
                // to the chunk position.
                let id = tool_call.get("id").and_then(Value::as_str)?;
                self.tool_calls
                    .iter()
                    .find(|(_, state)| state.id.as_deref() == Some(id))
                    .map(|(index, _)| *index)
            })
            .unwrap_or(position as u64);
        let state = self.tool_calls.entry(index).or_default();
        if let Some(id) = tool_call.get("id").and_then(Value::as_str)
            && !id.is_empty()
        {
            state.id.get_or_insert_with(|| id.to_string());
        }
        if let Some(function) = tool_call.get("function") {
            if let Some(name) = function.get("name").and_then(Value::as_str)
                && !name.is_empty()
            {
                state.name.get_or_insert_with(|| name.to_string());
            }
            match function.get("arguments") {
                Some(Value::String(arguments)) => state.arguments.push_str(arguments),
                // A few servers send already-parsed JSON arguments.
                Some(arguments @ Value::Object(_)) => {
                    state.arguments.push_str(&arguments.to_string())
                }
                _ => {}
            }
        }
    }

    async fn finish_reasoning(&mut self) {
        if let Some((id, text)) = self.reasoning.take() {
            self.send(ResponseEvent::OutputItemDone(reasoning_item(id, text)))
                .await;
        }
    }

    async fn finish_message(&mut self) {
        if let Some((id, text)) = self.message.take() {
            self.send(ResponseEvent::OutputItemDone(assistant_message(id, text)))
                .await;
        }
    }

    async fn finish_tool_calls(&mut self) {
        let tool_calls = std::mem::take(&mut self.tool_calls);
        for (index, state) in tool_calls {
            let Some(wire_name) = state.name else {
                debug!("skipping chat tool call at index {index} without a name");
                continue;
            };
            let call_id = state
                .id
                .unwrap_or_else(|| format!("call_{}", uuid::Uuid::now_v7().simple()));
            let arguments = if state.arguments.trim().is_empty() {
                "{}".to_string()
            } else {
                state.arguments
            };
            let item = match self.tool_names.resolve(&wire_name) {
                ChatTool::Function { namespace, name } => ResponseItem::FunctionCall {
                    id: None,
                    name,
                    namespace,
                    arguments,
                    call_id,
                    internal_chat_message_metadata_passthrough: None,
                },
                ChatTool::Freeform { name } => ResponseItem::CustomToolCall {
                    id: None,
                    status: None,
                    call_id,
                    name,
                    namespace: None,
                    input: freeform_input(&arguments),
                    internal_chat_message_metadata_passthrough: None,
                },
            };
            self.send(ResponseEvent::OutputItemDone(item)).await;
        }
    }

    /// Flushes all in-progress items. Called on `finish_reason` and at stream end.
    async fn finish_items(&mut self) {
        self.finish_reasoning().await;
        self.finish_message().await;
        self.finish_tool_calls().await;
    }

    async fn complete(mut self) {
        self.ensure_created().await;
        self.finish_items().await;
        let response_id = std::mem::take(&mut self.response_id);
        let token_usage = self.token_usage.take();
        self.send(ResponseEvent::Completed {
            response_id,
            token_usage,
            end_turn: None,
        })
        .await;
    }

    /// Handles one decoded chunk. Returns `Err` for terminal errors.
    async fn handle_chunk(&mut self, chunk: &Value) -> Result<(), ApiError> {
        if let Some(error) = chunk.get("error") {
            return Err(stream_error(error));
        }
        self.ensure_created().await;
        if self.response_id.is_empty()
            && let Some(id) = chunk.get("id").and_then(Value::as_str)
        {
            self.response_id = id.to_string();
        }
        if let Some(usage) = chunk.get("usage").filter(|usage| !usage.is_null()) {
            self.token_usage = Some(token_usage_from_chat(usage));
        }

        let Some(choices) = chunk.get("choices").and_then(Value::as_array) else {
            return Ok(());
        };
        for choice in choices {
            // `delta` for streamed chunks; `message` for servers that ignore
            // `stream: true` on some code paths and send a single completion.
            let delta = choice.get("delta").or_else(|| choice.get("message"));
            if let Some(delta) = delta {
                for key in ["reasoning_content", "reasoning"] {
                    if let Some(text) = reasoning_text(delta.get(key)) {
                        self.append_reasoning(&text).await;
                        break;
                    }
                }
                match delta.get("content") {
                    Some(Value::String(text)) => self.append_text(text).await,
                    Some(Value::Array(parts)) => {
                        for part in parts {
                            if let Some(text) = part.get("text").and_then(Value::as_str) {
                                self.append_text(text).await;
                            }
                        }
                    }
                    _ => {}
                }
                if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                    for (position, tool_call) in tool_calls.iter().enumerate() {
                        self.append_tool_call(tool_call, position);
                    }
                }
            }

            match choice.get("finish_reason").and_then(Value::as_str) {
                Some("length") => return Err(ApiError::ContextWindowExceeded),
                Some(_) => self.finish_items().await,
                None => {}
            }
        }
        Ok(())
    }
}

/// Processes Server-Sent Events from the Chat Completions streaming API.
///
/// The stream is terminated by a `data: [DONE]` sentinel. Usage arrives in a
/// final chunk (with empty `choices`) after `finish_reason` when
/// `stream_options.include_usage` is set, so completion is only reported once
/// the sentinel is seen or the connection closes.
pub(crate) async fn process_chat_sse(
    stream: ByteStream,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    tool_names: ChatToolNames,
) {
    let mut stream = stream.eventsource();
    let mut state = ChatStreamState {
        tx_event,
        tool_names,
        response_id: String::new(),
        created_sent: false,
        reasoning: None,
        message: None,
        tool_calls: BTreeMap::new(),
        token_usage: None,
    };
    let mut saw_chunk = false;

    loop {
        let start = Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        if let Some(t) = telemetry.as_ref() {
            t.on_sse_poll(&response, start.elapsed());
        }
        let sse = match response {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(e))) => {
                debug!("SSE Error: {e:#}");
                let _ = state
                    .tx_event
                    .send(Err(ApiError::Stream(e.to_string())))
                    .await;
                return;
            }
            Ok(None) => {
                if saw_chunk {
                    state.complete().await;
                } else {
                    let _ = state
                        .tx_event
                        .send(Err(ApiError::Stream(
                            "chat completions stream closed before any chunk".into(),
                        )))
                        .await;
                }
                return;
            }
            Err(_) => {
                let _ = state
                    .tx_event
                    .send(Err(ApiError::Stream("idle timeout waiting for SSE".into())))
                    .await;
                return;
            }
        };

        trace!("Chat SSE event: {}", sse.data);
        let data = sse.data.trim();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" || data == "DONE" {
            state.complete().await;
            return;
        }
        let chunk: Value = match serde_json::from_str(data) {
            Ok(chunk) => chunk,
            Err(err) => {
                debug!("Failed to parse Chat Completions SSE event: {err}, data: {data}");
                continue;
            }
        };
        saw_chunk = true;
        if let Err(err) = state.handle_chunk(&chunk).await {
            let _ = state.tx_event.send(Err(err)).await;
            return;
        }
        if state.tx_event.is_closed() {
            return;
        }
    }
}

fn reasoning_item(id: ResponseItemId, text: String) -> ResponseItem {
    ResponseItem::Reasoning {
        id: Some(id),
        summary: Vec::new(),
        content: Some(if text.is_empty() {
            Vec::new()
        } else {
            vec![ReasoningItemContent::ReasoningText { text }]
        }),
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn assistant_message(id: ResponseItemId, text: String) -> ResponseItem {
    ResponseItem::Message {
        id: Some(id),
        role: "assistant".to_string(),
        content: if text.is_empty() {
            Vec::new()
        } else {
            vec![ContentItem::OutputText { text }]
        },
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn reasoning_text(value: Option<&Value>) -> Option<String> {
    let text = match value? {
        Value::String(text) => text.clone(),
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .and_then(Value::as_str)?
            .to_string(),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

/// Recovers the raw freeform input from function-call arguments. Falls back to
/// the raw argument string if the model did not produce the expected object.
fn freeform_input(arguments: &str) -> String {
    match serde_json::from_str::<Value>(arguments) {
        Ok(Value::Object(object)) => match object.get(FREEFORM_INPUT_ARGUMENT) {
            Some(Value::String(input)) => input.clone(),
            Some(other) => other.to_string(),
            None => arguments.to_string(),
        },
        Ok(Value::String(input)) => input,
        _ => arguments.to_string(),
    }
}

fn token_usage_from_chat(usage: &Value) -> TokenUsage {
    let int = |value: Option<&Value>| value.and_then(Value::as_i64).unwrap_or(0);
    let input_tokens = int(usage.get("prompt_tokens"));
    let output_tokens = int(usage.get("completion_tokens"));
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(input_tokens + output_tokens);
    TokenUsage {
        input_tokens,
        cached_input_tokens: int(usage.pointer("/prompt_tokens_details/cached_tokens")),
        cache_write_input_tokens: 0,
        output_tokens,
        reasoning_output_tokens: int(usage.pointer("/completion_tokens_details/reasoning_tokens")),
        total_tokens,
    }
}

fn stream_error(error: &Value) -> ApiError {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| error.to_string());
    let code = error
        .get("code")
        .or_else(|| error.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if code == "context_length_exceeded" {
        ApiError::ContextWindowExceeded
    } else {
        ApiError::Stream(message)
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
