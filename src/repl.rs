use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context as AnyhowContext, Result};
use crossterm::cursor::{position, MoveTo};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use unicode_width::UnicodeWidthStr;

use crate::agent;
use crate::blocks;
use crate::config::Config;
use crate::context::{self, Context, PermissionMode, Turn};
use crate::history::HistoryDb;
use crate::llm::LlmClient;
use crate::parser::{self, InputKind};
use crate::prompt_ui;
use crate::suggest::Suggester;

pub async fn run(
    config: Config,
    history_db: HistoryDb,
    permission_mode: PermissionMode,
) -> Result<()> {
    let recent_turns = history_db.recent(5).unwrap_or_default();
    let mut context = Context::new(config.agent.context_lines, recent_turns)?;
    context.permission_mode = permission_mode;
    let llm = LlmClient::new(&config.llm)?;
    let runtime_name = runtime_name_from_base_url(&config.llm.base_url);

    loop {
        prompt_ui::render_status_bar(&config.llm.model, runtime_name, &context.cwd, &context.permission_mode)?;
        print_prompt(&context).await?;
        let suggester = Suggester::new(history_db.all_commands().unwrap_or_default());
        let input = match read_input_line(&suggester)? {
            ReadOutcome::Line(line) => line,
            ReadOutcome::Interrupt => continue,
            ReadOutcome::TogglePermissionMode => {
                toggle_permission_mode(&mut context);
                continue;
            }
            ReadOutcome::Eof => break,
        };

        if input.trim().is_empty() {
            continue;
        }

        if input.trim() == "--allow-all" {
            enable_auto_approve(&mut context);
            continue;
        }

        let previous_turn_count = context.turn_history.len();
        match parser::classify(&input) {
            InputKind::DirectCommand => {
                let started = Instant::now();
                match context::run_passthrough(&input, &mut context).await {
                    Ok(result) => {
                        if !result.interactive {
                            blocks::print_command_block(
                                &input,
                                &result.output,
                                result.exit_code,
                                started.elapsed(),
                            )?;
                        }
                    }
                    Err(error) => {
                        prompt_ui::print_error(&error.to_string());
                        continue;
                    }
                }

                let turn = Turn {
                    user_input: input.clone(),
                    planned_commands: vec![input],
                    executed: true,
                    explanation: String::new(),
                };
                context.record_turn(turn.clone());
                if let Err(error) = history_db.insert_turn(&turn) {
                    prompt_ui::print_error(&format!("history error: {error}"));
                }
            }
            InputKind::NaturalLanguage => {
                if let Err(error) = agent::handle(&input, &config, &mut context, &llm).await {
                    prompt_ui::print_error(&error.to_string());
                }

                if context.turn_history.len() > previous_turn_count {
                    if let Some(turn) = context.turn_history.last() {
                        if let Err(error) = history_db.insert_turn(turn) {
                            prompt_ui::print_error(&format!("history error: {error}"));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

enum ReadOutcome {
    Line(String),
    Interrupt,
    TogglePermissionMode,
    Eof,
}

async fn print_prompt(context: &Context) -> Result<()> {
    let mut stdout = io::stdout();
    let path_segment = shorten_cwd(&context.cwd);
    let git_segment = git_prompt_segment(&context.cwd).await;

    execute!(stdout, SetForegroundColor(Color::Rgb { r: 71, g: 85, b: 105 }), Print("\n• "))?;

    execute!(
        stdout,
        SetBackgroundColor(Color::Rgb { r: 30, g: 41, b: 59 }),
        SetForegroundColor(Color::Rgb { r: 241, g: 245, b: 249 }),
        Print(format!(" {path_segment} ")),
        SetForegroundColor(Color::Rgb { r: 30, g: 41, b: 59 }),
        Print(""),
        ResetColor,
    )?;

    if let Some((branch, dirty)) = git_segment {
        execute!(
            stdout,
            SetBackgroundColor(if dirty {
                Color::Rgb { r: 217, g: 119, b: 6 }
            } else {
                Color::Rgb { r: 5, g: 150, b: 105 }
            }),
            SetForegroundColor(Color::Black),
            Print(format!(" {branch} {} ", if dirty { "±" } else { "✓" })),
            SetForegroundColor(if dirty {
                Color::Rgb { r: 217, g: 119, b: 6 }
            } else {
                Color::Rgb { r: 5, g: 150, b: 105 }
            }),
            Print(""),
            ResetColor,
        )?;
    }

    if matches!(context.permission_mode, PermissionMode::AutoApprove { .. }) {
        execute!(
            stdout,
            SetBackgroundColor(Color::Rgb { r: 251, g: 191, b: 36 }),
            SetForegroundColor(Color::Black),
            Print(" [AUTO] "),
            SetForegroundColor(Color::Rgb { r: 251, g: 191, b: 36 }),
            Print(""),
            ResetColor,
        )?;
    }

    execute!(
        stdout,
        SetForegroundColor(Color::Rgb { r: 96, g: 165, b: 250 }),
        SetAttribute(Attribute::Bold),
        Print(" ❯  "),
        ResetColor
    )?;
    stdout.flush().context("failed to flush prompt")?;
    Ok(())
}

fn read_input_line(suggester: &Suggester) -> Result<ReadOutcome> {
    let mut stdout = io::stdout();
    enable_raw_mode().context("failed to enable raw mode")?;

    let result = (|| -> Result<ReadOutcome> {
        let mut input = String::new();
        let (start_col, start_row) = position().context("failed to read cursor position")?;
        let mut suppress_suggestion = false;

        render_input(&mut stdout, start_col, start_row, &input, None)?;

        loop {
            match event::read().context("failed to read terminal input")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter => {
                        render_input(&mut stdout, start_col, start_row, &input, None)?;
                        writeln!(stdout)?;
                        return Ok(ReadOutcome::Line(input));
                    }
                    KeyCode::Backspace if input.pop().is_some() => {
                        suppress_suggestion = false;
                        render_input(
                            &mut stdout,
                            start_col,
                            start_row,
                            &input,
                            visible_suggestion(suggester, &input, suppress_suggestion).as_deref(),
                        )?;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        render_input(&mut stdout, start_col, start_row, &input, None)?;
                        writeln!(stdout)?;
                        return Ok(ReadOutcome::Interrupt);
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        render_input(&mut stdout, start_col, start_row, &input, None)?;
                        writeln!(stdout)?;
                        if input.is_empty() {
                            return Ok(ReadOutcome::Eof);
                        }
                        return Ok(ReadOutcome::Line(input));
                    }
                    KeyCode::F(2) => {
                        render_input(&mut stdout, start_col, start_row, &input, None)?;
                        writeln!(stdout)?;
                        return Ok(ReadOutcome::TogglePermissionMode);
                    }
                    KeyCode::Char(character) => {
                        if character == '\t' {
                            if let Some(suggestion) = visible_suggestion(suggester, &input, suppress_suggestion) {
                                input = suggestion;
                                suppress_suggestion = false;
                                render_input(&mut stdout, start_col, start_row, &input, None)?;
                            }
                            continue;
                        }

                        input.push(character);
                        suppress_suggestion = false;
                        render_input(
                            &mut stdout,
                            start_col,
                            start_row,
                            &input,
                            visible_suggestion(suggester, &input, suppress_suggestion).as_deref(),
                        )?;
                    }
                    KeyCode::Tab | KeyCode::Right | KeyCode::End => {
                        if let Some(suggestion) = visible_suggestion(suggester, &input, suppress_suggestion) {
                            input = suggestion;
                            suppress_suggestion = false;
                            render_input(&mut stdout, start_col, start_row, &input, None)?;
                        }
                    }
                    KeyCode::Esc => {
                        suppress_suggestion = true;
                        render_input(&mut stdout, start_col, start_row, &input, None)?;
                    }
                    _ => {
                        suppress_suggestion = true;
                        render_input(&mut stdout, start_col, start_row, &input, None)?;
                    }
                },
                _ => {}
            }
        }
    })();

    disable_raw_mode().ok();
    result
}

fn render_input(
    stdout: &mut io::Stdout,
    start_col: u16,
    start_row: u16,
    input: &str,
    suggestion: Option<&str>,
) -> Result<()> {
    let ghost = suggestion
        .and_then(|full| full.strip_prefix(input))
        .unwrap_or_default();
    let cursor_col = (usize::from(start_col) + display_width(input)).min(u16::MAX as usize) as u16;

    execute!(
        stdout,
        MoveTo(start_col, start_row),
        Clear(ClearType::UntilNewLine),
        SetForegroundColor(Color::White),
        Print(input),
        SetAttribute(Attribute::Dim),
        SetForegroundColor(Color::DarkGrey),
        Print(ghost),
        ResetColor,
        SetAttribute(Attribute::Reset),
        MoveTo(cursor_col, start_row),
    )?;
    stdout.flush().context("failed to flush input line")?;
    Ok(())
}

fn visible_suggestion(suggester: &Suggester, input: &str, suppress: bool) -> Option<String> {
    if suppress {
        return None;
    }

    suggester.suggest(input)
}

fn toggle_permission_mode(context: &mut Context) {
    context.permission_mode = match context.permission_mode {
        PermissionMode::PerPlan => {
            prompt_ui::print_info("Full-permission mode enabled for this session.");
            PermissionMode::AutoApprove { countdown_secs: 2 }
        }
        PermissionMode::AutoApprove { .. } => {
            prompt_ui::print_info("Full-permission mode disabled.");
            PermissionMode::PerPlan
        }
    };
}

fn enable_auto_approve(context: &mut Context) {
    if matches!(context.permission_mode, PermissionMode::AutoApprove { .. }) {
        prompt_ui::print_info("Full-permission mode is already enabled.");
        return;
    }

    context.permission_mode = PermissionMode::AutoApprove { countdown_secs: 2 };
    prompt_ui::print_info("Full-permission mode enabled for this session.");
}

async fn git_prompt_segment(cwd: &Path) -> Option<(String, bool)> {
    let branch = git_stdout(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    let status = git_stdout(cwd, &["status", "--porcelain"]).await;
    Some((branch, status.map(|output| !output.is_empty()).unwrap_or(false)))
}

async fn git_stdout(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = timeout(
        Duration::from_millis(300),
        Command::new("git")
            .args(args)
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!stdout.is_empty()).then_some(stdout)
}

fn runtime_name_from_base_url(base_url: &str) -> &'static str {
    match base_url {
        "http://localhost:11434/v1" => "Ollama ✓",
        "http://localhost:1234/v1" => "LM Studio ✓",
        "http://localhost:8080/v1" => "Local runtime ✓",
        "http://localhost:1337/v1" => "Jan ✓",
        _ => "Runtime ?",
    }
}

fn shorten_cwd(cwd: &Path) -> String {
    let display = if let Some(home) = dirs::home_dir() {
        if let Ok(relative) = cwd.strip_prefix(&home) {
            if relative.as_os_str().is_empty() {
                "~".to_string()
            } else {
                format!("~/{}", relative.display())
            }
        } else {
            cwd.display().to_string()
        }
    } else {
        cwd.display().to_string()
    };

    if display_width(&display) <= 40 {
        return display;
    }

    let mut parts = display.split('/').collect::<Vec<_>>();
    if parts.len() >= 3 {
        let tail = parts.split_off(parts.len().saturating_sub(2));
        if display.starts_with("~/") {
            return format!("~/…/{}/{}", tail[0], tail[1]);
        }
        return format!("…/{}/{}", tail[0], tail[1]);
    }

    display
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}