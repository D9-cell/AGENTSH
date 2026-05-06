use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use agentsh::agent;
use agentsh::config::Config;
use agentsh::context::Context;
use agentsh::llm::LlmClient;
use mockito::{Matcher, Server};
use serde_json::json;

#[tokio::test]
async fn executes_safe_plan_and_records_explanation() {
    let mut server = Server::new_async().await;
    let plan_body = json!({
        "choices": [
            {
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "call_1",
                            "function": {
                                "name": "bash_exec",
                                "arguments": "{\"command\":\"echo hello\"}"
                            }
                        }
                    ],
                    "tool_call_id": null
                }
            }
        ]
    });
    let explain_body = json!({
        "choices": [
            {
                "message": {
                    "role": "assistant",
                    "content": "I ran a simple echo command so you could verify the terminal path works and see the output immediately.",
                    "tool_calls": null,
                    "tool_call_id": null
                }
            }
        ]
    });

    let plan_mock = server
        .mock("POST", "/v1/chat/completions")
        .match_body(Matcher::Regex("tool_choice".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(plan_body.to_string())
        .create_async()
        .await;

    let explain_mock = server
        .mock("POST", "/v1/chat/completions")
        .match_body(Matcher::Regex("Summarise what you just did".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(explain_body.to_string())
        .create_async()
        .await;

    let original_cwd = std::env::current_dir().unwrap();
    let temp_dir = unique_temp_dir("agent-integration");
    fs::create_dir_all(&temp_dir).unwrap();
    std::env::set_current_dir(&temp_dir).unwrap();

    let mut config = Config::default();
    config.llm.base_url = format!("{}/v1", server.url());
    config.safety.auto_approve_safe = true;

    let llm = LlmClient::new(&config.llm).unwrap();
    let mut context = Context::new(config.agent.context_lines, Vec::new()).unwrap();

    agent::handle("show me a quick greeting", &config, &mut context, &llm)
        .await
        .unwrap();

    assert_eq!(context.turn_history.len(), 1);
    let turn = &context.turn_history[0];
    assert!(turn.executed);
    assert_eq!(turn.planned_commands, vec!["echo hello".to_string()]);
    assert!(turn.explanation.contains("echo command"));

    plan_mock.assert_async().await;
    explain_mock.assert_async().await;

    std::env::set_current_dir(original_cwd).unwrap();
    let _ = fs::remove_dir_all(temp_dir);
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("agentsh-{prefix}-{nanos}"))
}