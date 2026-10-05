use super::*;
use crate::common::Reasoning;
use crate::common::TextControls;
use crate::common::TextFormat;
use crate::common::TextFormatType;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;

fn base_request(input: Vec<ResponseItem>, tools: Option<Vec<Value>>) -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "gemma-4-12b".to_string(),
        instructions: "You are Clanker.".to_string(),
        input,
        tools,
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        store: false,
        stream: true,
        stream_options: None,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
        client_metadata: None,
    }
}

fn build_default(request: &ResponsesApiRequest) -> ChatCompletionsRequest {
    build_chat_completions_request(request, &ChatRequestOptions::default())
}

fn with_effort(effort: ReasoningEffort) -> ResponsesApiRequest {
    let mut request = base_request(vec![text("user", "hi")], None);
    request.reasoning = Some(Reasoning {
        effort: Some(effort),
        summary: None,
        context: None,
    });
    request
}

fn reasoning(text: &str) -> ResponseItem {
    ResponseItem::Reasoning {
        id: None,
        summary: Vec::new(),
        content: Some(vec![ReasoningItemContent::ReasoningText {
            text: text.to_string(),
        }]),
        encrypted_content: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn message(role: &str, content: Vec<ContentItem>) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: role.to_string(),
        content,
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn text(role: &str, text: &str) -> ResponseItem {
    let part = if role == "assistant" {
        ContentItem::OutputText {
            text: text.to_string(),
        }
    } else {
        ContentItem::InputText {
            text: text.to_string(),
        }
    };
    message(role, vec![part])
}

fn function_call(call_id: &str, name: &str, arguments: &str) -> ResponseItem {
    ResponseItem::FunctionCall {
        id: None,
        name: name.to_string(),
        namespace: None,
        arguments: arguments.to_string(),
        call_id: call_id.to_string(),
        internal_chat_message_metadata_passthrough: None,
    }
}

fn function_output(call_id: &str, body: FunctionCallOutputBody) -> ResponseItem {
    ResponseItem::FunctionCallOutput {
        id: None,
        call_id: call_id.to_string(),
        output: FunctionCallOutputPayload {
            body,
            success: Some(true),
        },
        internal_chat_message_metadata_passthrough: None,
    }
}

/// A request advertising every tool shape the translator has to handle.
pub(crate) fn request_with_tools() -> ResponsesApiRequest {
    base_request(
        Vec::new(),
        Some(vec![
            json!({
                "type": "function",
                "name": "shell",
                "description": "Run a command",
                "strict": false,
                "parameters": {"type": "object", "properties": {"command": {"type": "array"}}}
            }),
            json!({
                "type": "custom",
                "name": "apply_patch",
                "description": "Apply a patch",
                "format": {"type": "grammar", "syntax": "lark", "definition": "start: patch"}
            }),
            json!({
                "type": "namespace",
                "name": "mcp__docs__",
                "description": "Docs tools",
                "tools": [{
                    "type": "function",
                    "name": "search",
                    "description": "Search docs",
                    "strict": false,
                    "parameters": {"type": "object", "properties": {"q": {"type": "string"}}}
                }]
            }),
            json!({"type": "web_search"}),
            json!({"type": "tool_search", "execution": "client", "description": "", "parameters": {}}),
        ]),
    )
}

#[test]
fn translates_tools_to_chat_functions() {
    let request = build_default(&request_with_tools());
    let tools = request.body["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["function"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["shell", "apply_patch", "mcp__docs__search"]);
    assert!(tools.iter().all(|tool| tool["type"] == "function"));
    assert_eq!(
        tools[0]["function"],
        json!({
            "name": "shell",
            "description": "Run a command",
            "parameters": {"type": "object", "properties": {"command": {"type": "array"}}}
        })
    );
    assert_eq!(
        tools[1]["function"]["parameters"]["required"],
        json!([FREEFORM_INPUT_ARGUMENT])
    );
    let description = tools[1]["function"]["description"].as_str().unwrap();
    assert!(description.contains("start: patch"), "{description}");

    assert_eq!(
        request.tool_names.resolve("apply_patch"),
        ChatTool::Freeform {
            name: "apply_patch".to_string()
        }
    );
    assert_eq!(
        request.tool_names.resolve("mcp__docs__search"),
        ChatTool::Function {
            namespace: Some("mcp__docs__".to_string()),
            name: "search".to_string()
        }
    );
    assert_eq!(request.body["tool_choice"], json!("auto"));
    assert_eq!(request.body["parallel_tool_calls"], json!(true));
    assert_eq!(request.body["stream"], json!(true));
    assert_eq!(
        request.body["stream_options"],
        json!({"include_usage": true})
    );
}

#[test]
fn omits_tool_fields_without_tools() {
    let request = build_default(&base_request(vec![text("user", "hi")], None));
    let body = request.body.as_object().unwrap();
    assert!(!body.contains_key("tools"));
    assert!(!body.contains_key("tool_choice"));
    assert!(!body.contains_key("parallel_tool_calls"));
}

#[test]
fn builds_conversation_with_tool_round_trip() {
    let input = vec![
        text("developer", "Sandbox: workspace-write"),
        text("user", "List files"),
        ResponseItem::Reasoning {
            id: None,
            summary: Vec::new(),
            content: Some(vec![ReasoningItemContent::ReasoningText {
                text: "I should run ls".to_string(),
            }]),
            encrypted_content: None,
            internal_chat_message_metadata_passthrough: None,
        },
        function_call("call_1", "shell", r#"{"command":["ls"]}"#),
        function_call("call_2", "shell", r#"{"command":["pwd"]}"#),
        function_output("call_1", FunctionCallOutputBody::Text("a.txt".to_string())),
        function_output("call_2", FunctionCallOutputBody::Text("/tmp".to_string())),
        ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "call_3".to_string(),
            name: "apply_patch".to_string(),
            namespace: None,
            input: "*** Begin Patch".to_string(),
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::CustomToolCallOutput {
            id: None,
            call_id: "call_3".to_string(),
            name: None,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text("Done".to_string()),
                success: Some(true),
            },
            internal_chat_message_metadata_passthrough: None,
        },
        text("assistant", "There is a.txt"),
    ];
    let request = build_default(&base_request(input, None));
    assert_eq!(
        request.body["messages"],
        json!([
            {"role": "system", "content": "You are Clanker.\n\nSandbox: workspace-write"},
            {"role": "user", "content": "List files"},
            {
                "role": "assistant",
                "content": null,
                "reasoning_content": "I should run ls",
                "tool_calls": [
                    {"id": "call_1", "type": "function", "function": {"name": "shell", "arguments": "{\"command\":[\"ls\"]}"}},
                    {"id": "call_2", "type": "function", "function": {"name": "shell", "arguments": "{\"command\":[\"pwd\"]}"}}
                ]
            },
            {"role": "tool", "tool_call_id": "call_1", "content": "a.txt"},
            {"role": "tool", "tool_call_id": "call_2", "content": "/tmp"},
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {"id": "call_3", "type": "function", "function": {"name": "apply_patch", "arguments": "{\"input\":\"*** Begin Patch\"}"}}
                ]
            },
            {"role": "tool", "tool_call_id": "call_3", "content": "Done"},
            {"role": "assistant", "content": "There is a.txt"}
        ])
    );
}

#[test]
fn assistant_text_and_tool_call_share_one_message() {
    let input = vec![
        text("user", "hi"),
        text("assistant", "Let me check."),
        function_call("call_1", "shell", "{}"),
        function_output("call_1", FunctionCallOutputBody::Text("ok".to_string())),
    ];
    let request = build_default(&base_request(input, None));
    assert_eq!(
        request.body["messages"][2],
        json!({
            "role": "assistant",
            "content": "Let me check.",
            "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "shell", "arguments": "{}"}}]
        })
    );
}

#[test]
fn namespaced_function_calls_are_flattened_in_history() {
    let input = vec![ResponseItem::FunctionCall {
        id: None,
        name: "search".to_string(),
        namespace: Some("mcp__docs__".to_string()),
        arguments: "{}".to_string(),
        call_id: "call_1".to_string(),
        internal_chat_message_metadata_passthrough: None,
    }];
    let request = build_default(&base_request(input, None));
    assert_eq!(
        request.body["messages"][1]["tool_calls"][0]["function"]["name"],
        json!("mcp__docs__search")
    );
}

#[test]
fn user_images_and_audio_become_media_parts() {
    let input = vec![message(
        "user",
        vec![
            ContentItem::InputText {
                text: "What is this?".to_string(),
            },
            ContentItem::InputImage {
                image_url: "data:image/png;base64,AAAA".to_string(),
                detail: None,
            },
            ContentItem::InputImage {
                image_url: "data:audio/wav;base64,UklGRg==".to_string(),
                detail: None,
            },
            ContentItem::InputImage {
                image_url: "data:audio/mpeg;base64,SUQz".to_string(),
                detail: None,
            },
        ],
    )];
    let request = build_default(&base_request(input, None));
    assert_eq!(
        request.body["messages"][1],
        json!({
            "role": "user",
            "content": [
                {"type": "text", "text": "What is this?"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
                {"type": "input_audio", "input_audio": {"data": "UklGRg==", "format": "wav"}},
                {"type": "input_audio", "input_audio": {"data": "SUQz", "format": "mp3"}}
            ]
        })
    );
}

#[test]
fn tool_output_images_are_replayed_after_tool_messages() {
    let input = vec![
        function_call("call_1", "view_image", "{}"),
        function_call("call_2", "shell", "{}"),
        function_output(
            "call_1",
            FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "screenshot".to_string(),
                },
                FunctionCallOutputContentItem::InputImage {
                    image_url: "data:image/png;base64,AAAA".to_string(),
                    detail: None,
                },
            ]),
        ),
        function_output("call_2", FunctionCallOutputBody::Text("ok".to_string())),
        text("user", "thanks"),
    ];
    let request = build_default(&base_request(input, None));
    let messages = request.body["messages"].as_array().unwrap();
    let roles: Vec<&str> = messages
        .iter()
        .map(|message| message["role"].as_str().unwrap())
        .collect();
    assert_eq!(
        roles,
        vec!["system", "assistant", "tool", "tool", "user", "user"]
    );
    assert_eq!(messages[2]["content"], json!("screenshot"));
    assert_eq!(
        messages[4]["content"][1],
        json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}})
    );
}

#[test]
fn forwards_reasoning_effort_and_output_schema() {
    let mut request = base_request(vec![text("user", "hi")], None);
    request.reasoning = Some(Reasoning {
        effort: Some(ReasoningEffort::High),
        summary: None,
        context: None,
    });
    request.text = Some(TextControls {
        verbosity: None,
        format: Some(TextFormat {
            r#type: TextFormatType::JsonSchema,
            strict: true,
            schema: json!({"type": "object"}),
            name: "codex_output_schema".to_string(),
        }),
    });
    let body = build_default(&request).body;
    assert_eq!(body["reasoning_effort"], json!("high"));
    assert_eq!(
        body["response_format"],
        json!({
            "type": "json_schema",
            "json_schema": {"name": "codex_output_schema", "schema": {"type": "object"}, "strict": true}
        })
    );
}

#[test]
fn disabling_reasoning_turns_off_template_thinking() {
    let options = ChatRequestOptions {
        thinking_template_kwargs: true,
        extra_body: None,
        max_audio_inputs: None,
    };
    for effort in [ReasoningEffort::None, ReasoningEffort::Minimal] {
        let body = build_chat_completions_request(&with_effort(effort), &options).body;
        assert_eq!(
            body["chat_template_kwargs"],
            json!({"enable_thinking": false})
        );
    }

    let body = build_chat_completions_request(&with_effort(ReasoningEffort::Low), &options).body;
    assert_eq!(body.get("chat_template_kwargs"), None);
    assert_eq!(body["reasoning_effort"], json!("low"));

    // Providers that reject unknown fields (OpenAI) only get `reasoning_effort`.
    let body = build_default(&with_effort(ReasoningEffort::None)).body;
    assert_eq!(body.get("chat_template_kwargs"), None);
    assert_eq!(body["reasoning_effort"], json!("none"));
}

#[test]
fn extra_body_is_deep_merged_last() {
    let options = ChatRequestOptions {
        thinking_template_kwargs: true,
        extra_body: Some(
            json!({
                "chat_template_kwargs": {"reasoning_budget": 128},
                "reasoning_effort": null,
                "stream_options": {"include_usage": false},
                "top_k": 40,
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        ),
        max_audio_inputs: None,
    };
    let body = build_chat_completions_request(&with_effort(ReasoningEffort::None), &options).body;
    assert_eq!(
        body["chat_template_kwargs"],
        json!({"enable_thinking": false, "reasoning_budget": 128})
    );
    assert_eq!(body.get("reasoning_effort"), None);
    assert_eq!(body["stream_options"], json!({"include_usage": false}));
    assert_eq!(body["top_k"], json!(40));
    assert_eq!(body["model"], json!("gemma-4-12b"));
}

#[test]
fn reasoning_from_an_interrupted_turn_is_not_replayed_later() {
    let input = vec![
        text("user", "first"),
        reasoning("abandoned thought"),
        text("user", "second"),
        text("assistant", "answer"),
    ];
    let messages = build_default(&base_request(input, None)).body["messages"].clone();
    assert_eq!(
        messages[3],
        json!({"role": "assistant", "content": "answer"})
    );
}

fn audio(data: &str) -> ContentItem {
    ContentItem::InputImage {
        image_url: format!("data:audio/wav;base64,{data}"),
        detail: None,
    }
}

#[test]
fn max_audio_inputs_keeps_only_the_newest_clips() {
    let input = vec![
        message("user", vec![audio("AAAA")]),
        text("assistant", "heard you"),
        message("user", vec![audio("BBBB"), audio("CCCC")]),
    ];
    let options = ChatRequestOptions {
        max_audio_inputs: Some(1),
        ..Default::default()
    };
    let body = build_chat_completions_request(&base_request(input, None), &options).body;
    let note = json!({"type": "text", "text": AUDIO_DROPPED_NOTE});
    assert_eq!(body["messages"][1]["content"], json!([note.clone()]));
    assert_eq!(
        body["messages"][3]["content"],
        json!([
            note,
            {"type": "input_audio", "input_audio": {"data": "CCCC", "format": "wav"}}
        ])
    );
}

#[test]
fn control_characters_are_scrubbed_from_text() {
    let input = vec![text("user", "png\u{0}\u{1}bytes\tok\nline")];
    let body = build_default(&base_request(input, None)).body;
    assert_eq!(body["messages"][1]["content"], json!("pngbytes\tok\nline"));
}
