use std::time::Instant;

use anyhow::{Context as AnyhowContext, Result};
use serde_json::Value;

use crate::blocks;
use crate::config::Config;
use crate::context::{Context, PermissionMode, Turn};
use crate::llm::{ChatMessage, LlmClient};
use crate::prompt_ui;
use crate::safety::RiskLevel;
use crate::spinner::Spinner;
use crate::tools::{self, PlannedCommand};

#[derive(Debug, Clone)]
struct ExecutionResult {
    command: PlannedCommand,
    output: String,
    exit_code: Option<i32>,
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

    let Some(tool_calls) = plan_message.tool_calls.as_ref() else {
        if let Some(content) = plan_message.content.as_deref() {
            prompt_ui::print_text(content);
            context.record_turn(Turn {
                user_input: user_input.to_string(),
                planned_commands: Vec::new(),
                executed: false,
                explanation: content.to_string(),
            });
        }
        return Ok(());
    };

    if tool_calls.is_empty() {
        if let Some(content) = plan_message.content.as_deref() {
            prompt_ui::print_text(content);
            context.record_turn(Turn {
                user_input: user_input.to_string(),
                planned_commands: Vec::new(),
                executed: false,
                explanation: content.to_string(),
            });
        }
        return Ok(());
    }

    let max_commands = config.agent.max_commands_per_turn.max(1);
    let selected_tool_calls = tool_calls.iter().take(max_commands).collect::<Vec<_>>();
    if tool_calls.len() > selected_tool_calls.len() {
        prompt_ui::print_info(&format!(
            "Limiting the plan to the first {} commands.",
            selected_tool_calls.len()
        ));
    }

    let planned_commands = build_planned_commands(&selected_tool_calls, context).await?;
    let approved = should_auto_approve(config, &planned_commands)
        || (!config.safety.require_confirm)
        || prompt_ui::show_permission_panel(&planned_commands, &context.permission_mode);

    if !approved {
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
        Err(error) => {
            let message = format!("[LLM error] {error}");
            prompt_ui::print_text(&message);
            message
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