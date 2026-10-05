//! End-to-end coverage for providers configured with `wire_api = "chat"`.

use codex_model_provider_info::ModelProviderInfo;
use codex_model_provider_info::WireApi;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::user_input::UserInput;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use wiremock::Mock;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

fn chat_sse(chunks: &[Value]) -> String {
    let mut body = String::new();
    for chunk in chunks {
        body.push_str(&format!("data: {chunk}\n\n"));
    }
    body.push_str("data: [DONE]\n\n");
    body
}

struct ChatSequence {
    calls: AtomicUsize,
    bodies: Vec<String>,
}

impl Respond for ChatSequence {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(self.bodies[call].clone())
    }
}

fn chat_provider(base_url: String) -> ModelProviderInfo {
    ModelProviderInfo {
        name: "chat-mock".into(),
        base_url: Some(base_url),
        // Any variable that exists in the test environment works as a key.
        env_key: Some("PATH".into()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        aws: None,
        wire_api: WireApi::Chat,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
        developer_role_name: None,
        extra_body: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_wire_runs_tool_call_round_trip() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "call_plan_1";
    let plan_args = json!({
        "explanation": "chat wire check",
        "plan": [{"step": "Say hi", "status": "in_progress"}],
    })
    .to_string();
    let (args_head, args_tail) = plan_args.split_at(10);

    let first = chat_sse(&[
        json!({"id": "chatcmpl-1", "choices": [{"index": 0, "delta": {"role": "assistant", "reasoning_content": "Planning first."}}]}),
        json!({"id": "chatcmpl-1", "choices": [{"index": 0, "delta": {"tool_calls": [
            {"index": 0, "id": call_id, "type": "function", "function": {"name": "update_plan", "arguments": args_head}}
        ]}}]}),
        json!({"id": "chatcmpl-1", "choices": [{"index": 0, "delta": {"tool_calls": [
            {"index": 0, "function": {"arguments": args_tail}}
        ]}}]}),
        json!({"id": "chatcmpl-1", "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
        json!({"id": "chatcmpl-1", "choices": [], "usage": {"prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 120}}),
    ]);
    let second = chat_sse(&[
        json!({"id": "chatcmpl-2", "choices": [{"index": 0, "delta": {"role": "assistant", "content": "all "}}]}),
        json!({"id": "chatcmpl-2", "choices": [{"index": 0, "delta": {"content": "done"}}]}),
        json!({"id": "chatcmpl-2", "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}),
    ]);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ChatSequence {
            calls: AtomicUsize::new(0),
            bodies: vec![first, second],
        })
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;

    let provider = chat_provider(format!("{}/v1", server.uri()));
    let TestCodex { codex, .. } = test_codex()
        .with_config(move |config| {
            config.model_provider = provider;
        })
        .build(&server)
        .await?;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "make a plan".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: Default::default(),
            thread_settings: Default::default(),
        })
        .await?;

    let mut saw_plan_update = false;
    let mut last_agent_message = None;
    wait_for_event(&codex, |event| match event {
        EventMsg::PlanUpdate(update) => {
            saw_plan_update = update.explanation.as_deref() == Some("chat wire check");
            false
        }
        EventMsg::TurnComplete(complete) => {
            last_agent_message = complete.last_agent_message.clone();
            true
        }
        _ => false,
    })
    .await;
    assert!(saw_plan_update, "update_plan tool call was not executed");
    assert_eq!(last_agent_message.as_deref(), Some("all done"));

    let requests = server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|request| request.url.path() == "/v1/chat/completions")
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);

    let first_body: Value = requests[0].body_json()?;
    assert_eq!(first_body["stream"], json!(true));
    assert!(
        first_body.get("input").is_none(),
        "sent a Responses payload"
    );
    assert_eq!(first_body["messages"][0]["role"], json!("system"));
    let tools = first_body["tools"].as_array().expect("tools advertised");
    assert!(
        tools
            .iter()
            .all(|tool| tool["type"] == "function" && tool["function"]["name"].is_string())
    );
    assert!(
        tools
            .iter()
            .any(|tool| tool["function"]["name"] == "update_plan")
    );

    let second_body: Value = requests[1].body_json()?;
    let messages = second_body["messages"].as_array().expect("messages");
    let assistant = messages
        .iter()
        .find(|message| message.get("tool_calls").is_some())
        .expect("assistant tool call replayed");
    assert_eq!(assistant["reasoning_content"], json!("Planning first."));
    assert_eq!(assistant["tool_calls"][0]["id"], json!(call_id));
    assert_eq!(
        assistant["tool_calls"][0]["function"],
        json!({"name": "update_plan", "arguments": plan_args})
    );
    let tool_message = messages.last().expect("tool output message");
    assert_eq!(tool_message["role"], json!("tool"));
    assert_eq!(tool_message["tool_call_id"], json!(call_id));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chat_wire_disables_thinking_and_merges_extra_body() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ChatSequence {
            calls: AtomicUsize::new(0),
            bodies: vec![chat_sse(&[
                json!({"id": "chatcmpl-1", "choices": [{"index": 0, "delta": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}]}),
            ])],
        })
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    let mut provider = chat_provider(format!("{}/v1", server.uri()));
    provider.extra_body = json!({"chat_template_kwargs": {"keep": true}, "top_k": 40})
        .as_object()
        .cloned();
    let TestCodex { codex, .. } = test_codex()
        .with_config(move |config| {
            config.model_provider = provider;
            config.model_reasoning_effort = Some(ReasoningEffort::None);
        })
        .build(&server)
        .await?;

    codex
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "hi".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            responsesapi_client_metadata: None,
            additional_context: Default::default(),
            thread_settings: Default::default(),
        })
        .await?;
    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let requests = server.received_requests().await.unwrap_or_default();
    let body: Value = requests
        .iter()
        .find(|request| request.url.path() == "/v1/chat/completions")
        .expect("chat request")
        .body_json()?;
    assert_eq!(body["reasoning_effort"], json!("none"));
    assert_eq!(
        body["chat_template_kwargs"],
        json!({"enable_thinking": false, "keep": true})
    );
    assert_eq!(body["top_k"], json!(40));

    Ok(())
}
