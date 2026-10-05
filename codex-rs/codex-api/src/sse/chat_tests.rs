use super::*;
use crate::requests::chat::build_chat_completions_request;
use assert_matches::assert_matches;
use bytes::Bytes;
use futures::stream;
use pretty_assertions::assert_eq;
use serde_json::json;

fn sse_body(chunks: &[Value]) -> String {
    let mut body = String::new();
    for chunk in chunks {
        body.push_str(&format!("data: {chunk}\n\n"));
    }
    body.push_str("data: [DONE]\n\n");
    body
}

async fn collect(body: String, tool_names: ChatToolNames) -> Vec<Result<ResponseEvent, ApiError>> {
    // Keep the connection open after the body to prove `[DONE]` terminates the stream.
    let stream = stream::iter(vec![Ok(Bytes::from(body))]).chain(stream::pending());
    let stream: ByteStream = Box::pin(stream);
    let (tx, mut rx) = mpsc::channel(64);
    tokio::spawn(process_chat_sse(
        stream,
        tx,
        Duration::from_secs(5),
        /*telemetry*/ None,
        tool_names,
    ));
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    })
    .await
    .expect("stream should finish")
}

fn delta(delta: Value) -> Value {
    json!({"id": "chatcmpl-1", "choices": [{"index": 0, "delta": delta, "finish_reason": null}]})
}

fn finish(reason: &str) -> Value {
    json!({"id": "chatcmpl-1", "choices": [{"index": 0, "delta": {}, "finish_reason": reason}]})
}

#[tokio::test]
async fn streams_reasoning_then_text_and_reports_usage() {
    let body = sse_body(&[
        delta(json!({"role": "assistant", "content": null})),
        delta(json!({"reasoning_content": "think"})),
        delta(json!({"reasoning_content": "ing"})),
        delta(json!({"content": "Hi "})),
        delta(json!({"content": "there"})),
        finish("stop"),
        json!({"id": "chatcmpl-1", "choices": [], "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15,
            "prompt_tokens_details": {"cached_tokens": 4},
            "completion_tokens_details": {"reasoning_tokens": 2}
        }}),
    ]);
    let events = collect(body, ChatToolNames::default()).await;
    let events: Vec<ResponseEvent> = events.into_iter().map(Result::unwrap).collect();

    assert_matches!(events[0], ResponseEvent::Created);
    assert_matches!(
        &events[1],
        ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. })
    );
    assert_matches!(&events[2], ResponseEvent::ReasoningContentDelta { delta, .. } if delta == "think");
    assert_matches!(&events[3], ResponseEvent::ReasoningContentDelta { delta, .. } if delta == "ing");
    let ResponseEvent::OutputItemDone(ResponseItem::Reasoning { content, .. }) = &events[4] else {
        panic!("expected reasoning done, got {:?}", events[4]);
    };
    assert_eq!(
        content,
        &Some(vec![ReasoningItemContent::ReasoningText {
            text: "thinking".to_string()
        }])
    );
    assert_matches!(
        &events[5],
        ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
    );
    assert_matches!(&events[6], ResponseEvent::OutputTextDelta(text) if text == "Hi ");
    assert_matches!(&events[7], ResponseEvent::OutputTextDelta(text) if text == "there");
    let ResponseEvent::OutputItemDone(ResponseItem::Message { role, content, .. }) = &events[8]
    else {
        panic!("expected message done, got {:?}", events[8]);
    };
    assert_eq!(role, "assistant");
    assert_eq!(
        content,
        &vec![ContentItem::OutputText {
            text: "Hi there".to_string()
        }]
    );
    let ResponseEvent::Completed {
        response_id,
        token_usage,
        ..
    } = &events[9]
    else {
        panic!("expected completed, got {:?}", events[9]);
    };
    assert_eq!(response_id, "chatcmpl-1");
    assert_eq!(
        token_usage,
        &Some(TokenUsage {
            input_tokens: 10,
            cached_input_tokens: 4,
            cache_write_input_tokens: 0,
            output_tokens: 5,
            reasoning_output_tokens: 2,
            total_tokens: 15,
        })
    );
    assert_eq!(events.len(), 10);
}

#[tokio::test]
async fn accepts_reasoning_field_alias() {
    let body = sse_body(&[delta(json!({"reasoning": "hmm"})), finish("stop")]);
    let events = collect(body, ChatToolNames::default()).await;
    assert!(events.iter().any(|event| matches!(
        event,
        Ok(ResponseEvent::ReasoningContentDelta { delta, .. }) if delta == "hmm"
    )));
}

#[tokio::test]
async fn assembles_streamed_tool_calls_and_maps_tool_kinds() {
    let request = crate::requests::chat::tests::request_with_tools();
    let tool_names = build_chat_completions_request(&request, &Default::default()).tool_names;

    let body = sse_body(&[
        delta(json!({"tool_calls": [
            {"index": 0, "id": "call_a", "type": "function", "function": {"name": "shell", "arguments": ""}}
        ]})),
        delta(json!({"tool_calls": [{"index": 0, "function": {"arguments": "{\"command\":"}}]})),
        delta(json!({"tool_calls": [{"index": 0, "function": {"arguments": "[\"ls\"]}"}}]})),
        delta(json!({"tool_calls": [
            {"index": 1, "id": "call_b", "type": "function", "function": {
                "name": "apply_patch",
                "arguments": "{\"input\":\"*** Begin Patch\\n*** End Patch\"}"
            }}
        ]})),
        delta(json!({"tool_calls": [
            {"index": 2, "id": "call_c", "type": "function", "function": {
                "name": "mcp__docs__search", "arguments": "{\"q\":\"x\"}"
            }}
        ]})),
        finish("tool_calls"),
    ]);
    let events: Vec<ResponseEvent> = collect(body, tool_names)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let items: Vec<&ResponseItem> = events
        .iter()
        .filter_map(|event| match event {
            ResponseEvent::OutputItemDone(item) => Some(item),
            _ => None,
        })
        .collect();

    assert_eq!(
        items,
        vec![
            &ResponseItem::FunctionCall {
                id: None,
                name: "shell".to_string(),
                namespace: None,
                arguments: "{\"command\":[\"ls\"]}".to_string(),
                call_id: "call_a".to_string(),
                internal_chat_message_metadata_passthrough: None,
            },
            &ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id: "call_b".to_string(),
                name: "apply_patch".to_string(),
                namespace: None,
                input: "*** Begin Patch\n*** End Patch".to_string(),
                internal_chat_message_metadata_passthrough: None,
            },
            &ResponseItem::FunctionCall {
                id: None,
                name: "search".to_string(),
                namespace: Some("mcp__docs__".to_string()),
                arguments: "{\"q\":\"x\"}".to_string(),
                call_id: "call_c".to_string(),
                internal_chat_message_metadata_passthrough: None,
            },
        ]
    );
    assert_matches!(events.last(), Some(ResponseEvent::Completed { .. }));
}

#[tokio::test]
async fn completes_on_eof_without_done_sentinel() {
    let body = format!("data: {}\n\n", delta(json!({"content": "hi"})));
    let stream = stream::iter(vec![Ok(Bytes::from(body))]);
    let stream: ByteStream = Box::pin(stream);
    let (tx, mut rx) = mpsc::channel(16);
    process_chat_sse(
        stream,
        tx,
        Duration::from_secs(5),
        None,
        ChatToolNames::default(),
    )
    .await;
    let mut last = None;
    while let Ok(event) = rx.try_recv() {
        last = Some(event);
    }
    assert_matches!(last, Some(Ok(ResponseEvent::Completed { .. })));
}

#[tokio::test]
async fn surfaces_inline_errors() {
    let body = sse_body(&[json!({"error": {"message": "model exploded", "type": "server_error"}})]);
    let events = collect(body, ChatToolNames::default()).await;
    assert_matches!(events.last(), Some(Err(ApiError::Stream(message))) if message == "model exploded");
}

#[tokio::test]
async fn length_finish_reason_is_context_window_exceeded() {
    let body = sse_body(&[delta(json!({"content": "a"})), finish("length")]);
    let events = collect(body, ChatToolNames::default()).await;
    assert_matches!(events.last(), Some(Err(ApiError::ContextWindowExceeded)));
}
