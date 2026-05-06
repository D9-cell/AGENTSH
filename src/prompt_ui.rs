use std::io::{self, Write};

use anyhow::{Context, Result};
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;

use crate::safety::RiskLevel;
use crate::tools::PlannedCommand;

pub fn show_permission_panel(commands: &[PlannedCommand]) -> bool {
    try_show_permission_panel(commands).unwrap_or_else(|error| {
        print_error(&error.to_string());
        false
    })
}

pub fn show_explanation(text: &str) {
    if let Err(error) = try_show_explanation(text) {
        print_error(&error.to_string());
    }
}

pub fn print_error(msg: &str) {
    let mut stderr = io::stderr();
    let _ = execute!(
        stderr,
        SetForegroundColor(Color::Red),
        Print(format!("[agentsh error] {msg}\n")),
        ResetColor
    );
}

pub fn print_info(msg: &str) {
    let mut stdout = io::stdout();
    let _ = execute!(
        stdout,
        SetAttribute(Attribute::Dim),
        SetForegroundColor(Color::White),
        Print(format!("[agentsh] {msg}\n")),
        ResetColor,
        SetAttribute(Attribute::Reset)
    );
}

pub fn print_text(msg: &str) {
    println!("{msg}");
}

fn try_show_permission_panel(commands: &[PlannedCommand]) -> Result<bool> {
    let mut stdout = io::stdout();
    enable_raw_mode().context("failed to enable raw mode")?;
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to initialize terminal UI")?;

    let ui_result = (|| -> Result<bool> {
        terminal.draw(|frame| draw_permission_panel(frame, commands))?;

        loop {
            match event::read().context("failed to read terminal input")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                    _ => return Ok(false),
                },
                _ => {}
            }
        }
    })();

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show).ok();
    terminal.show_cursor().ok();

    ui_result
}

fn draw_permission_panel(frame: &mut ratatui::Frame<'_>, commands: &[PlannedCommand]) {
    let area = centered_rect(frame.size());
    let block = Block::default().title("AgentSH plan").borders(Borders::ALL);
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let available_width = inner.width.saturating_sub(2).max(20) as usize;
    let mut lines = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        lines.push(Line::from(vec![
            Span::raw(format!("[{}] ", index + 1)),
            Span::styled(
                format!("[{}] ", risk_label(command.risk)),
                Style::default()
                    .fg(risk_color(command.risk))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(truncate_text(&command.display_text, available_width.saturating_sub(12))),
        ]));

        if let Some(preview) = &command.preview {
            for preview_line in preview.lines().take(6) {
                lines.push(Line::from(Span::styled(
                    format!("    {}", truncate_text(preview_line, available_width.saturating_sub(4))),
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
        }
    }

    let commands_widget = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(commands_widget, chunks[0]);
    frame.render_widget(Paragraph::new("Approve all? [Y/n]"), chunks[1]);
}

fn centered_rect(area: Rect) -> Rect {
    if area.width < 40 || area.height < 8 {
        return area;
    }

    let width = area.width.min(100);
    let height = area.height.min(20);
    Rect {
        x: area.x + (area.width.saturating_sub(width) / 2),
        y: area.y + (area.height.saturating_sub(height) / 2),
        width,
        height,
    }
}

fn try_show_explanation(text: &str) -> Result<()> {
    let total_width = terminal::size().map(|(width, _)| width as usize).unwrap_or(80).max(20);
    let content_width = total_width.saturating_sub(4).max(16);
    let lines = wrap_text(text, content_width);
    let mut stdout = io::stdout();

    execute!(stdout, SetAttribute(Attribute::Dim)).context("failed to dim explanation output")?;
    writeln!(stdout, "┌ why {}┐", "─".repeat(content_width.saturating_sub(4)))?;
    for line in lines {
        writeln!(stdout, "│ {line:<width$} │", width = content_width.saturating_sub(2))?;
    }
    writeln!(stdout, "└{}┘", "─".repeat(content_width))?;
    execute!(stdout, SetAttribute(Attribute::Reset)).ok();
    Ok(())
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut wrapped = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.trim().is_empty() {
            wrapped.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let next_len = if current.is_empty() {
                word.len()
            } else {
                current.len() + 1 + word.len()
            };

            if next_len > width && !current.is_empty() {
                wrapped.push(current);
                current = word.to_string();
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            }
        }

        if !current.is_empty() {
            wrapped.push(current);
        }
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }

    wrapped
}

fn truncate_text(text: &str, max_width: usize) -> String {
    let count = text.chars().count();
    if count <= max_width {
        return text.to_string();
    }

    if max_width <= 1 {
        return "…".to_string();
    }

    let mut truncated = text.chars().take(max_width - 1).collect::<String>();
    truncated.push('…');
    truncated
}

fn risk_label(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Safe => "SAFE",
        RiskLevel::High => "HIGH",
        RiskLevel::Critical => "CRITICAL",
    }
}

fn risk_color(risk: RiskLevel) -> ratatui::style::Color {
    match risk {
        RiskLevel::Safe => ratatui::style::Color::Green,
        RiskLevel::High => ratatui::style::Color::Yellow,
        RiskLevel::Critical => ratatui::style::Color::Red,
    }
}