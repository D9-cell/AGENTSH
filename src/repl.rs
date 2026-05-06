use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context as AnyhowContext, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::agent;
use crate::config::Config;
use crate::context::{self, Context, Turn};
use crate::history::HistoryDb;
use crate::llm::LlmClient;
use crate::parser::{self, InputKind};
use crate::prompt_ui;

pub async fn run(config: Config, history_db: HistoryDb) -> Result<()> {
    let recent_turns = history_db.recent(5).unwrap_or_default();
    let mut context = Context::new(config.agent.context_lines, recent_turns)?;
    let llm = LlmClient::new(&config.llm)?;

    loop {
        print_prompt(&context.cwd)?;
        let input = match read_input_line()? {
            ReadOutcome::Line(line) => line,
            ReadOutcome::Interrupt => continue,
            ReadOutcome::Eof => break,
        };

        if input.trim().is_empty() {
            continue;
        }

        let previous_turn_count = context.turn_history.len();
        match parser::classify(&input) {
            InputKind::DirectCommand => {
                if let Err(error) = context::run_passthrough(&input, &mut context).await {
                    prompt_ui::print_error(&error.to_string());
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
    Eof,
}

fn print_prompt(cwd: &Path) -> Result<()> {
    let mut stdout = io::stdout();
    execute!(
        stdout,
        SetForegroundColor(Color::Green),
        Print(cwd.display().to_string()),
        ResetColor,
        SetForegroundColor(Color::Blue),
        Print(" ❯ "),
        ResetColor
    )?;
    stdout.flush().context("failed to flush prompt")?;
    Ok(())
}

fn read_input_line() -> Result<ReadOutcome> {
    let mut stdout = io::stdout();
    enable_raw_mode().context("failed to enable raw mode")?;

    let result = (|| -> Result<ReadOutcome> {
        let mut input = String::new();

        loop {
            match event::read().context("failed to read terminal input")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter => {
                        writeln!(stdout)?;
                        return Ok(ReadOutcome::Line(input));
                    }
                    KeyCode::Backspace if input.pop().is_some() => {
                        execute!(stdout, Print("\u{8} \u{8}"))?;
                        stdout.flush()?;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        writeln!(stdout)?;
                        return Ok(ReadOutcome::Interrupt);
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        writeln!(stdout)?;
                        if input.is_empty() {
                            return Ok(ReadOutcome::Eof);
                        }
                        return Ok(ReadOutcome::Line(input));
                    }
                    KeyCode::Char(character) => {
                        input.push(character);
                        write!(stdout, "{character}")?;
                        stdout.flush()?;
                    }
                    KeyCode::Tab => {
                        input.push('\t');
                        write!(stdout, "\t")?;
                        stdout.flush()?;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    })();

    disable_raw_mode().ok();
    result
}