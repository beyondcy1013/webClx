use super::*;

#[test]
fn converts_responses_input_to_chat_messages() {
    let payload = json!({
        "model": "codex-MiniMax-M2.7",
        "input": [
            {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "rules"}]},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hello"}]},
            {"type": "function_call_output", "call_id": "call_1", "output": "done"}
        ],
        "tools": [{"type": "function", "name": "exec_command", "description": "run", "parameters": {"type": "object"}}],
        "stream": true,
        "max_output_tokens": 128
    });

    let chat = responses_request_to_chat_request(&payload).expect("request should convert");
    assert_eq!(chat["stream"], false);
    assert_eq!(chat["max_completion_tokens"], 128);
    assert_eq!(chat["messages"][0]["role"], "system");
    assert_eq!(chat["messages"][1]["content"], "hello");
    assert_eq!(chat["messages"][2]["role"], "tool");
    assert_eq!(chat["tools"][0]["function"]["name"], "exec_command");
}

#[test]
fn wraps_chat_response_as_responses_sse() {
    let chat = json!({
        "id": "chat_1",
        "created": 1777560000,
        "model": "MiniMax-M2.7",
        "choices": [{
            "message": {"role": "assistant", "content": "OK"}
        }],
        "usage": {"prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3}
    });

    let response = chat_response_to_responses_payload(&chat, "codex-MiniMax-M2.7");
    assert_eq!(response["output"][0]["content"][0]["text"], "OK");
    let sse = response_payload_to_sse_chunks(&response).join("");
    assert!(sse.contains("response.output_text.delta"));
    assert!(sse.contains("response.completed"));
    assert!(sse.contains("data: [DONE]"));
}

#[test]
fn wraps_deepseek_reasoning_as_opaque_responses_item() {
    let chat = json!({
        "id": "chat_1",
        "model": "deepseek-v4-pro",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "OK",
                "reasoning_content": "Need answer briefly."
            }
        }]
    });

    let response = chat_response_to_responses_payload(&chat, "deepseek-v4-pro");

    assert_eq!(response["output"][0]["type"], "reasoning");
    assert_eq!(
        decode_reasoning_carrier(response["output"][0]["encrypted_content"].as_str().unwrap()),
        Some("Need answer briefly.".to_string())
    );
    assert_eq!(response["output"][1]["content"][0]["text"], "OK");
}

#[test]
fn responses_reasoning_item_round_trips_to_assistant_tool_call() {
    let payload = json!({
        "model": "deepseek-v4-pro",
        "input": [
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "list files"}]},
            {
                "type": "reasoning",
                "summary": [],
                "content": null,
                "encrypted_content": encode_reasoning_carrier("Need to inspect the directory.")
            },
            {"type": "function_call", "call_id": "call_1", "name": "exec_command", "arguments": "{\"cmd\":\"ls\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "Cargo.toml\n"}
        ]
    });

    let chat = responses_request_to_chat_request(&payload).expect("request should convert");

    assert_eq!(chat["messages"][1]["role"], "assistant");
    assert_eq!(chat["messages"][1]["reasoning_content"], "Need to inspect the directory.");
    assert_eq!(chat["messages"][1]["tool_calls"][0]["id"], "call_1");
}

#[test]
fn deepseek_sanitizer_backfills_missing_assistant_reasoning() {
    let request = json!({
        "model": "deepseek-v4-pro",
        "messages": [
            {"role": "user", "content": "list files"},
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "exec_command", "arguments": "{\"cmd\":\"ls\"}"}
                }]
            },
            {"role": "tool", "tool_call_id": "call_1", "content": "Cargo.toml\n"}
        ]
    });

    let request = sanitize_chat_request_for_deepseek(request);

    assert_eq!(request["messages"][1]["reasoning_content"], DEEPSEEK_REASONING_PLACEHOLDER);
}

#[test]
fn deepseek_sanitizer_degrades_unmatched_assistant_tool_calls() {
    let request = json!({
        "model": "deepseek-v4-pro",
        "messages": [
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "exec_command",
                        "arguments": "{\"cmd\":\"pwd\"}"
                    }
                }]
            },
            {"role": "user", "content": "continue"}
        ]
    });

    let request = sanitize_chat_request_for_deepseek(request);
    let messages = request["messages"]
        .as_array()
        .expect("messages should be an array");

    assert_eq!(messages[0]["role"], "assistant");
    assert!(messages[0].get("tool_calls").is_none());
    assert!(messages[0]["content"].as_str().unwrap().contains("call_1"));
    assert_eq!(messages[1]["role"], "user");
}

#[test]
fn strips_minimax_think_blocks_from_response_text() {
    let chat = json!({
        "id": "chat_1",
        "model": "MiniMax-M2.7",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "<think>\ninternal reasoning\n</think>\n\nOK"
            }
        }]
    });

    let response = chat_response_to_responses_payload(&chat, "codex-MiniMax-M2.7");
    assert_eq!(response["output"][0]["content"][0]["text"], "OK");
}

#[test]
fn previous_response_id_restores_tool_call_history() {
    let history = CodexProxyHistory::new();
    let first_payload = json!({
        "model": "codex-MiniMax-M2.7",
        "input": [
            {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "rules"}]},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "run pwd"}]}
        ]
    });
    let first_request =
        responses_request_to_chat_request(&first_payload).expect("first request converts");
    let first_chat = json!({
        "id": "chat_1",
        "model": "MiniMax-M2.7",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "exec_command",
                        "arguments": "{\"cmd\":\"pwd\"}"
                    }
                }]
            }
        }]
    });
    let first_response = chat_response_to_responses_payload(&first_chat, "codex-MiniMax-M2.7");
    history.record_response(&first_response, &first_request, &first_chat);

    let second_payload = json!({
        "model": "codex-MiniMax-M2.7",
        "previous_response_id": "resp_chat_1",
        "input": [
            {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "rules"}]},
            {"type": "function_call_output", "call_id": "call_1", "output": "/home/codes/webClx\n"}
        ]
    });
    let second_request =
        responses_request_to_chat_request(&second_payload).expect("second request converts");
    let second_request =
        history.chat_request_with_previous_response(&second_payload, second_request);
    let second_request = sanitize_chat_request_for_minimax(second_request);
    let messages = second_request["messages"]
        .as_array()
        .expect("messages should be an array");

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(messages[2]["tool_calls"][0]["id"], "call_1");
    assert_eq!(messages[3]["role"], "tool");
    assert_eq!(messages[3]["tool_call_id"], "call_1");
}

#[test]
fn missing_previous_response_id_converts_orphan_tool_result() {
    let history = CodexProxyHistory::new();
    let payload = json!({
        "model": "codex-MiniMax-M2.7",
        "previous_response_id": "resp_missing",
        "input": [
            {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "rules"}]},
            {"type": "function_call_output", "call_id": "call_1", "output": "done"}
        ]
    });

    let request = responses_request_to_chat_request(&payload).expect("request converts");
    let request = history.chat_request_with_previous_response(&payload, request);
    let request = sanitize_chat_request_for_minimax(request);
    let messages = request["messages"]
        .as_array()
        .expect("messages should be an array");

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
    assert!(messages[1]["content"].as_str().unwrap().contains("call_1"));
    assert!(messages[1]["content"].as_str().unwrap().contains("done"));
}

#[test]
fn missing_previous_response_id_degrades_historical_tool_calls() {
    let payload = json!({
        "model": "codex-MiniMax-M2.7",
        "previous_response_id": "resp_missing",
        "input": [
            {"type": "function_call", "call_id": "call_1", "name": "exec_command", "arguments": "{\"cmd\":\"pwd\"}"},
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "continue"}]},
            {"type": "function_call_output", "call_id": "call_1", "output": "/tmp\n"}
        ]
    });

    let request = responses_request_to_chat_request(&payload).expect("request converts");
    let request = degrade_resume_chat_request_for_minimax(request);
    let messages = request["messages"]
        .as_array()
        .expect("messages should be an array");

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "assistant");
    assert!(messages[0].get("tool_calls").is_none());
    assert!(messages[0]["content"].as_str().unwrap().contains("call_1"));
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(messages[2]["role"], "user");
    assert!(messages[2]["content"].as_str().unwrap().contains("/tmp"));
}

#[test]
fn sanitizer_degrades_unmatched_assistant_tool_calls() {
    let messages = vec![
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "exec_command",
                    "arguments": "{\"cmd\":\"pwd\"}"
                }
            }]
        }),
        json!({"role": "user", "content": "continue"}),
        json!({"role": "tool", "tool_call_id": "call_other", "content": "/tmp\n"}),
    ];

    let messages = sanitize_chat_messages_for_minimax(messages);

    assert_eq!(messages[0]["role"], "assistant");
    assert!(messages[0].get("tool_calls").is_none());
    assert!(messages[0]["content"].as_str().unwrap().contains("call_1"));
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(messages[2]["role"], "user");
    assert!(
        messages[2]["content"]
            .as_str()
            .unwrap()
            .contains("call_other")
    );
    assert!(messages[2]["content"].as_str().unwrap().contains("/tmp"));
}

#[test]
fn sanitizer_reorders_non_adjacent_tool_outputs() {
    let messages = vec![
        json!({"role": "user", "content": "run checks"}),
        json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [
                {
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "exec_command",
                        "arguments": "{\"cmd\":\"pwd\"}"
                    }
                },
                {
                    "id": "call_2",
                    "type": "function",
                    "function": {
                        "name": "exec_command",
                        "arguments": "{\"cmd\":\"ls\"}"
                    }
                }
            ]
        }),
        json!({"role": "user", "content": "continue"}),
        json!({"role": "tool", "tool_call_id": "call_2", "content": "Cargo.toml\n"}),
        json!({"role": "tool", "tool_call_id": "call_1", "content": "/tmp\n"}),
    ];

    let messages = sanitize_chat_messages_for_minimax(messages);

    assert_eq!(messages.len(), 5);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[2]["role"], "tool");
    assert_eq!(messages[2]["tool_call_id"], "call_1");
    assert_eq!(messages[3]["role"], "tool");
    assert_eq!(messages[3]["tool_call_id"], "call_2");
    assert_eq!(messages[4]["role"], "user");
}

#[test]
fn converts_chat_request_to_anthropic_messages_text_only() {
    let chat = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {"role": "system", "content": "be helpful"},
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello"},
            {"role": "user", "content": "what's up?"}
        ],
        "max_completion_tokens": 256,
        "temperature": 0.2,
        "top_p": 0.9
    });

    let payload = chat_request_to_anthropic_messages(&chat).expect("chat should convert");
    assert_eq!(payload["model"], "claude-3-5-sonnet-20241022");
    assert_eq!(payload["system"], "be helpful");
    assert_eq!(payload["max_tokens"], 256);
    assert_eq!(payload["temperature"], 0.2);
    assert_eq!(payload["top_p"], 0.9);
    assert_eq!(payload["stream"], false);

    let messages = payload["messages"].as_array().expect("messages is array");
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"][0]["type"], "text");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"][0]["text"], "hello");
    assert_eq!(messages[2]["role"], "user");
    assert_eq!(messages[2]["content"][0]["text"], "what's up?");
}

#[test]
fn converts_chat_request_to_anthropic_messages_with_tool_calls() {
    let chat = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {"role": "system", "content": "you have tools"},
            {"role": "user", "content": "list files"},
            {"role": "assistant", "content": "", "tool_calls": [
                {"id": "toolu_1", "type": "function", "function": {"name": "exec_command", "arguments": "{\"cmd\":\"ls\"}"}}
            ]},
            {"role": "tool", "tool_call_id": "toolu_1", "content": "Cargo.toml\n"}
        ],
        "tools": [
            {"type": "function", "function": {"name": "exec_command", "description": "run a command", "parameters": {"type": "object"}}}
        ],
        "tool_choice": "auto"
    });

    let payload = chat_request_to_anthropic_messages(&chat).expect("chat should convert");
    assert_eq!(payload["tools"][0]["name"], "exec_command");
    assert_eq!(payload["tools"][0]["input_schema"]["type"], "object");
    assert_eq!(payload["tool_choice"], "auto");

    let messages = payload["messages"].as_array().expect("messages is array");
    // system is hoisted, so the remaining messages are: user, assistant, user(tool_result)
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[1]["role"], "assistant");
    let assistant_blocks = messages[1]["content"]
        .as_array()
        .expect("assistant content is array");
    assert_eq!(assistant_blocks[0]["type"], "tool_use");
    assert_eq!(assistant_blocks[0]["id"], "toolu_1");
    assert_eq!(assistant_blocks[0]["name"], "exec_command");
    assert_eq!(assistant_blocks[0]["input"]["cmd"], "ls");
    assert_eq!(messages[2]["role"], "user");
    let tool_result = &messages[2]["content"][0];
    assert_eq!(tool_result["type"], "tool_result");
    assert_eq!(tool_result["tool_use_id"], "toolu_1");
}

#[test]
fn converts_chat_request_to_anthropic_messages_collapses_reasoning() {
    let chat = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {"role": "user", "content": "explain"},
            {"role": "assistant", "reasoning_content": "thinking hard", "content": "answer"}
        ]
    });

    let payload = chat_request_to_anthropic_messages(&chat).expect("chat should convert");
    let messages = payload["messages"].as_array().expect("messages");
    let assistant = &messages[1];
    let blocks = assistant["content"].as_array().expect("blocks");
    assert_eq!(blocks[0]["type"], "thinking");
    assert_eq!(blocks[0]["thinking"], "thinking hard");
    assert_eq!(blocks[1]["type"], "text");
    assert_eq!(blocks[1]["text"], "answer");
}

#[test]
fn converts_anthropic_messages_response_to_chat_response() {
    let response = json!({
        "id": "msg_01",
        "type": "message",
        "role": "assistant",
        "model": "claude-3-5-sonnet-20241022",
        "content": [
            {"type": "text", "text": "Hello there!"}
        ],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 12, "output_tokens": 4}
    });

    let chat =
        anthropic_messages_response_to_chat_response(&response, "claude-3-5-sonnet-20241022")
            .expect("response converts");
    assert_eq!(chat["object"], "chat.completion");
    assert_eq!(chat["model"], "claude-3-5-sonnet-20241022");
    assert_eq!(chat["choices"][0]["message"]["role"], "assistant");
    assert_eq!(chat["choices"][0]["message"]["content"], "Hello there!");
    assert_eq!(chat["choices"][0]["finish_reason"], "stop");
    assert_eq!(chat["usage"]["prompt_tokens"], 12);
    assert_eq!(chat["usage"]["completion_tokens"], 4);
    assert_eq!(chat["usage"]["total_tokens"], 16);
}

#[test]
fn converts_anthropic_messages_response_with_tool_use_and_thinking() {
    let response = json!({
        "id": "msg_02",
        "role": "assistant",
        "model": "claude-3-5-sonnet-20241022",
        "content": [
            {"type": "thinking", "thinking": "first I reason"},
            {"type": "text", "text": "Calling the tool."},
            {"type": "tool_use", "id": "toolu_42", "name": "exec_command", "input": {"cmd": "pwd"}}
        ],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 3, "output_tokens": 7}
    });

    let chat =
        anthropic_messages_response_to_chat_response(&response, "claude-3-5-sonnet-20241022")
            .expect("response converts");
    let message = &chat["choices"][0]["message"];
    assert_eq!(message["reasoning_content"], "first I reason");
    assert_eq!(message["content"], "Calling the tool.");
    let tool_calls = message["tool_calls"].as_array().expect("tool_calls array");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["id"], "toolu_42");
    assert_eq!(tool_calls[0]["function"]["name"], "exec_command");
    assert!(
        tool_calls[0]["function"]["arguments"]
            .as_str()
            .unwrap()
            .contains("pwd")
    );
    assert_eq!(chat["choices"][0]["finish_reason"], "tool_calls");
}

#[test]
fn anthropic_request_to_responses_payload_round_trip() {
    // After converting a Codex Responses request to a Chat request, then to
    // Anthropic messages, then forwarding a fake anthropic response back, we
    // should be able to wrap it as a Responses payload that the existing
    // history mechanism can record.
    let payload = json!({
        "model": "claude-3-5-sonnet-20241022",
        "input": "hi"
    });
    let chat = responses_request_to_chat_request(&payload).expect("responses converts");
    let anthropic = chat_request_to_anthropic_messages(&chat).expect("chat converts");
    assert_eq!(anthropic["messages"][0]["content"][0]["text"], "hi");

    let response = json!({
        "id": "msg_03",
        "role": "assistant",
        "model": "claude-3-5-sonnet-20241022",
        "content": [{"type": "text", "text": "hello back"}],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 1, "output_tokens": 2}
    });
    let chat_response =
        anthropic_messages_response_to_chat_response(&response, "claude-3-5-sonnet-20241022")
            .expect("response converts");
    let responses =
        chat_response_to_responses_payload(&chat_response, "claude-3-5-sonnet-20241022");
    assert_eq!(responses["output"][0]["content"][0]["text"], "hello back");
}
