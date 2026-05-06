use std::time::Instant;

use anyhow::{Context as AnyhowContext, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::blocks;
use crate::config::Config;
use crate::context::{Context, PermissionMode, Turn};
use crate::llm::{ChatMessage, LlmClient};
use crate::parser;
use crate::prompt_ui::{self, PermissionDecision};
use crate::safety::RiskLevel;
use crate::spinner::Spinner;
use crate::tools::{self, PlannedCommand};

#[derive(Debug, Clone)]
struct ExecutionResult {
    command: PlannedCommand,
    output: String,
    exit_code: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct ContentToolCall {
    name: String,
    #[serde(default)]
    parameters: Value,
    #[serde(default)]
    arguments: Value,
    command: Option<String>,
}

pub async fn handle(
    user_input: &str,
    config: &Config,
    context: &mut Context,
    llm: &LlmClient,
) -> Result<()> {
    let messages = build_messages(user_input, config, context);
    let spinner = Spinner::start("Planning...");
    let plan_message = match llm.plan(messages.clone(), &tools::all_schemas()).await {
        Ok(message) => {
            spinner.stop();
            message
        }
        Err(error) => {
            spinner.stop();
            prompt_ui::print_text(&format!("[LLM error] {error}"));
            return Ok(());
        }
    };

    let tool_calls = if let Some(tool_calls) = plan_message.tool_calls.clone().filter(|calls| !calls.is_empty()) {
        tool_calls
    } else if let Some(content) = plan_message.content.as_deref() {
        if let Some(fallback_calls) = fallback_tool_calls_from_content(content) {
            fallback_calls
        } else {
            prompt_ui::print_text(content);
            context.record_turn(Turn {
                user_input: user_input.to_string(),
                planned_commands: Vec::new(),
                executed: false,
                explanation: content.to_string(),
            });
            return Ok(());
        }
    } else {
        return Ok(());
    };

    let max_commands = config.agent.max_commands_per_turn.max(1);
    let selected_tool_calls = tool_calls.iter().take(max_commands).collect::<Vec<_>>();
    if tool_calls.len() > selected_tool_calls.len() {
        prompt_ui::print_info(&format!(
            "Limiting the plan to the first {} commands.",
            selected_tool_calls.len()
        ));
    }

    let planned_commands = build_planned_commands(&selected_tool_calls, context).await?;
    if !should_auto_approve(config, &planned_commands) && config.safety.require_confirm {
        match prompt_ui::show_permission_panel(&planned_commands, &context.permission_mode) {
            PermissionDecision::Approve => {}
            PermissionDecision::EnableAutoApprove => {
                context.permission_mode = PermissionMode::AutoApprove { countdown_secs: 2 };
                prompt_ui::print_info("Full-permission mode enabled for this session.");
            }
            PermissionDecision::Cancel => {
                prompt_ui::print_text("Cancelled.");
                context.record_turn(Turn {
                    user_input: user_input.to_string(),
                    planned_commands: planned_commands
                        .iter()
                        .map(|command| command.display_text.clone())
                        .collect(),
                    executed: false,
                    explanation: "Cancelled.".to_string(),
                });
                return Ok(());
            }
        }
    }

    let mut execution_results = Vec::new();
    for command in &planned_commands {
        let started = Instant::now();
        let result = match tools::execute_in_dir(&command.tool_name, &command.args, Some(&context.cwd)).await {
            Ok(result) => result,
            Err(error) => {
                let message = format!("[tool error] {error}");
                prompt_ui::print_error(&message);
                execution_results.push(ExecutionResult {
                    command: command.clone(),
                    output: message.clone(),
                    exit_code: None,
                });
                context.scrollback.push_line(message);
                continue;
            }
        };

        if let Some(updated_cwd) = result.updated_cwd.as_ref() {
            if updated_cwd.exists() {
                std::env::set_current_dir(updated_cwd).with_context(|| {
                    format!("failed to change directory to {}", updated_cwd.display())
                })?;
                context.cwd = std::env::current_dir().context("failed to sync current directory")?;
            }
        }
        context.scrollback.push_text(&result.output);
        blocks::print_command_block(
            &command.display_text,
            &result.output,
            result.exit_code,
            started.elapsed(),
        )?;

        if result.interrupted {
            prompt_ui::print_text("Mission cancelled.");
            context.permission_mode = PermissionMode::PerPlan;
            context.record_turn(Turn {
                user_input: user_input.to_string(),
                planned_commands: planned_commands
                    .iter()
                    .map(|planned| planned.display_text.clone())
                    .collect(),
                executed: false,
                explanation: "Mission cancelled.".to_string(),
            });
            return Ok(());
        }

        execution_results.push(ExecutionResult {
            command: command.clone(),
            output: result.output,
            exit_code: result.exit_code,
        });
    }

    let mut explanation_messages = messages;
    explanation_messages.push(plan_message.clone());
    for (tool_call, execution_result) in selected_tool_calls.iter().zip(execution_results.iter()) {
        explanation_messages.push(ChatMessage::tool(
            tool_call.id.clone(),
            format_tool_result(execution_result),
        ));
    }
    explanation_messages.push(ChatMessage::system(
        "Summarise what you just did and why in 2-3 plain sentences. Do not mention tool names or JSON. Speak to the user directly.",
    ));

    let explanation = match llm.explain(explanation_messages).await {
        Ok(text) => {
            prompt_ui::show_explanation(&text);
            text
        }
        Err(_) => {
            let fallback = fallback_explanation(&execution_results);
            prompt_ui::show_explanation(&fallback);
            fallback
        }
    };

    context.record_turn(Turn {
        user_input: user_input.to_string(),
        planned_commands: planned_commands
            .iter()
            .map(|command| command.display_text.clone())
            .collect(),
        executed: true,
        explanation,
    });

    Ok(())
}

fn fallback_tool_calls_from_content(content: &str) -> Option<Vec<crate::llm::ToolCall>> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return fallback_tool_calls_from_json(value);
    }

    if matches!(parser::classify(trimmed), parser::InputKind::DirectCommand) {
        return Some(vec![shell_tool_call(trimmed, 0)]);
    }

    None
}

fn fallback_tool_calls_from_json(value: Value) -> Option<Vec<crate::llm::ToolCall>> {
    match value {
        Value::Array(items) => {
            let tool_calls = items
                .into_iter()
                .enumerate()
                .map(|(index, item)| fallback_tool_call_from_value(item, index))
                .collect::<Option<Vec<_>>>()?;
            (!tool_calls.is_empty()).then_some(tool_calls)
        }
        other => fallback_tool_call_from_value(other, 0).map(|tool_call| vec![tool_call]),
    }
}

fn fallback_tool_call_from_value(value: Value, index: usize) -> Option<crate::llm::ToolCall> {
    let parsed = serde_json::from_value::<ContentToolCall>(value).ok()?;
    let arguments = if !parsed.arguments.is_null() {
        parsed.arguments
    } else {
        parsed.parameters
    };

    if matches!(parsed.name.as_str(), "bash_exec" | "file_read" | "file_write" | "git_status") {
        return Some(crate::llm::ToolCall {
            id: format!("fallback_call_{index}"),
            function: crate::llm::FunctionCall {
                name: parsed.name,
                arguments: serde_json::to_string(&arguments).ok()?,
            },
        });
    }

    if let Some(command) = parsed.command.or_else(|| fallback_shell_command(&parsed.name, &arguments)) {
        return Some(shell_tool_call(&command, index));
    }

    None
}

fn fallback_shell_command(name: &str, arguments: &Value) -> Option<String> {
    match arguments {
        Value::Null => Some(name.to_string()),
        Value::Object(map) if map.is_empty() => Some(name.to_string()),
        Value::Object(map) => map
            .get("command")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        Value::String(command) => Some(command.clone()),
        _ => None,
    }
}

fn shell_tool_call(command: &str, index: usize) -> crate::llm::ToolCall {
    crate::llm::ToolCall {
        id: format!("fallback_shell_{index}"),
        function: crate::llm::FunctionCall {
            name: "bash_exec".to_string(),
            arguments: serde_json::json!({ "command": command }).to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::fallback_tool_calls_from_content;

    #[test]
    fn parses_shell_like_json_content_into_tool_call() {
        let content = r#"{"name":"pwd","parameters":{}}"#;
        let tool_calls = fallback_tool_calls_from_content(content).expect("fallback tool calls");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "bash_exec");
        assert!(tool_calls[0].function.arguments.contains("pwd"));
    }

    #[test]
    fn parses_direct_command_content_into_tool_call() {
        let tool_calls = fallback_tool_calls_from_content("pwd").expect("fallback tool calls");
        assert_eq!(tool_calls[0].function.name, "bash_exec");
        assert!(tool_calls[0].function.arguments.contains("pwd"));
    }
}

fn fallback_explanation(results: &[ExecutionResult]) -> String {
    if results.is_empty() {
        return "No commands were executed.".to_string();
    }

    let total = results.len();
    let failures = results
        .iter()
        .filter(|result| result.exit_code.unwrap_or(1) != 0)
        .count();
    let last = results.last().expect("results checked as non-empty");

    if failures == 0 {
        return format!(
            "Completed {} command{}. The last step ran `{}` successfully.",
            total,
            if total == 1 { "" } else { "s" },
            last.command.display_text,
        );
    }

    format!(
        "Ran {} command{}. {} step{} failed. The last command `{}` exited with {}.",
        total,
        if total == 1 { "" } else { "s" },
        failures,
        if failures == 1 { "" } else { "s" },
        last.command.display_text,
        last.exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "an unknown status".to_string()),
    )
}

fn build_messages(user_input: &str, config: &Config, context: &Context) -> Vec<ChatMessage> {
    let mut messages = Vec::new();
    messages.push(ChatMessage::system(system_prompt(config, context)));

    let prior_turns = context
        .turn_history
        .iter()
        .rev()
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    for turn in prior_turns.into_iter().rev() {
        messages.push(ChatMessage::user(turn.user_input));

        let assistant_summary = if turn.planned_commands.is_empty() {
            turn.explanation
        } else {
            format!(
                "Planned commands: {}\nExplanation: {}",
                turn.planned_commands.join("; "),
                turn.explanation
            )
        };
        messages.push(ChatMessage::assistant(assistant_summary));
    }

    messages.push(ChatMessage::user(user_input.to_string()));
    messages
}

async fn build_planned_commands(
    tool_calls: &[&crate::llm::ToolCall],
    context: &Context,
) -> Result<Vec<PlannedCommand>> {
    let mut planned = Vec::with_capacity(tool_calls.len());
    for tool_call in tool_calls {
        let args: Value = serde_json::from_str(&tool_call.function.arguments)
            .with_context(|| format!("invalid tool arguments for {}", tool_call.function.name))?;
        let display_text = tools::describe_tool_call(&tool_call.function.name, &args)?;
        let risk = tools::risk_for_plan(&tool_call.function.name, &args)?;
        let preview = tools::preview_tool_call(&tool_call.function.name, &args, &context.cwd).await?;
        planned.push(PlannedCommand {
            tool_name: tool_call.function.name.clone(),
            args,
            risk,
            display_text,
            preview,
        });
    }

    Ok(planned)
}

fn should_auto_approve(config: &Config, planned_commands: &[PlannedCommand]) -> bool {
    config.safety.auto_approve_safe
        && !planned_commands.is_empty()
        && planned_commands
            .iter()
            .all(|command| command.risk == RiskLevel::Safe)
}

fn system_prompt(config: &Config, context: &Context) -> String {
    format!(
        "You are AgentSH, a terminal agent running on {}.\nCurrent directory: {}\nShell: {}\nRecent terminal output (last {} lines):\n---\n{}\n---\n\nYour job is to fulfil the user's request by calling shell tools.\nRules you must follow without exception:\n1. Never explain your reasoning or thinking. Just call the tools.\n2. Prefer the minimum number of commands needed to complete the task.\n3. Never run destructive commands (rm -rf, dd, mkfs, format) unless the user's request makes it unambiguously clear they intend destruction.\n4. If the request is ambiguous, call zero tools and reply with a single clarifying question.\n5. If the request cannot be done with shell commands, reply with a single sentence explaining why.\n6. Do not produce markdown. Plain text only.",
        std::env::consts::OS,
        context.cwd.display(),
        context.shell(),
        config.agent.context_lines,
        context.scrollback.render()
    )
}

fn format_tool_result(result: &ExecutionResult) -> String {
    let exit_code = result
        .exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "none".to_string());
    let output = if result.output.trim().is_empty() {
        "(no output)".to_string()
    } else {
        result.output.trim().to_string()
    };

    format!(
        "Command: {}\nExit code: {}\nOutput:\n{}",
        result.command.display_text, exit_code, output
    )
}