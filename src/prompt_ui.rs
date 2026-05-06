use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::cursor::{MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use unicode_width::UnicodeWidthStr;

use crate::context::PermissionMode;
use crate::safety::RiskLevel;
use crate::tools::PlannedCommand;

pub fn show_permission_panel(commands: &[PlannedCommand], mode: &PermissionMode) -> bool {
    try_show_permission_panel(commands, mode).unwrap_or_else(|error| {
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

pub fn render_status_bar(
    model: &str,
    runtime_name: &str,
    cwd: &Path,
    mode: &PermissionMode,
) -> Result<()> {
    let (width, height) = terminal::size().unwrap_or((80, 24));
    if height <= 24 {
        return Ok(());
    }

    let mode_label = match mode {
        PermissionMode::PerPlan => "[NORMAL]",
        PermissionMode::AutoApprove { .. } => "[AUTO]",
    };
    let bar_text = format!(
        " agentsh  v{}   {}   {}   {} ",
        env!("CARGO_PKG_VERSION"),
        model,
        runtime_name,
        cwd.display(),
    );
    let available_width = usize::from(width).saturating_sub(display_width(mode_label));
    let fitted = fit_to_width(&bar_text, available_width);

    let mut stdout = io::stdout();
    execute!(
        stdout,
        MoveTo(0, height.saturating_sub(1)),
        SetBackgroundColor(Color::Rgb { r: 30, g: 33, b: 39 }),
        SetForegroundColor(Color::White),
        Clear(ClearType::CurrentLine),
        Print(fitted),
        SetForegroundColor(match mode {
            PermissionMode::PerPlan => Color::Grey,
            PermissionMode::AutoApprove { .. } => Color::Yellow,
        }),
        Print(mode_label),
        ResetColor
    )?;
    Ok(())
}

fn try_show_permission_panel(commands: &[PlannedCommand], mode: &PermissionMode) -> Result<bool> {
    let mut stdout = io::stdout();
    enable_raw_mode().context("failed to enable raw mode")?;
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to initialize terminal UI")?;
    let has_critical = commands.iter().any(|command| command.risk == RiskLevel::Critical);
    let mut countdown = match mode {
        PermissionMode::AutoApprove { countdown_secs } if !has_critical => Some(*countdown_secs),
        _ => None,
    };
    let mut last_tick = Instant::now();

    let ui_result = (|| -> Result<bool> {
        loop {
            terminal.draw(|frame| draw_permission_panel(frame, commands, mode, countdown, has_critical))?;

            if let Some(remaining) = countdown {
                let timeout = Duration::from_millis(100);
                if event::poll(timeout).context("failed to poll terminal input")? {
                    match event::read().context("failed to read terminal input")? {
                        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => return Ok(false),
                            _ => {}
                        },
                        _ => {}
                    }
                }

                if last_tick.elapsed() >= Duration::from_secs(1) {
                    if remaining == 0 {
                        return Ok(true);
                    }

                    countdown = Some(remaining.saturating_sub(1));
                    last_tick = Instant::now();
                    if countdown == Some(0) {
                        return Ok(true);
                    }
                }
                continue;
            }

            match event::read().context("failed to read terminal input")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => return Ok(false),
                    _ => {}
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

fn draw_permission_panel(
    frame: &mut ratatui::Frame<'_>,
    commands: &[PlannedCommand],
    mode: &PermissionMode,
    countdown: Option<u8>,
    has_critical: bool,
) {
    let area = centered_rect(frame.size());
    let block = Block::default()
        .title(Span::styled(
            " AgentSH Plan ",
            Style::default().fg(ratatui::style::Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(ratatui::style::Color::DarkGray))
        .borders(Borders::ALL);
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(3), Constraint::Length(1)])
        .split(inner);

    let available_width = inner.width.saturating_sub(4).max(28) as usize;
    let command_width = available_width.saturating_sub(16).max(12);
    let mut lines = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        let row_style = if command.risk == RiskLevel::Critical {
            Style::default().bg(ratatui::style::Color::Rgb(60, 0, 0))
        } else {
            Style::default()
        };
        let display_text = pad_to_width(&truncate_text(&command.display_text, command_width), command_width);
        lines.push(Line::from(vec![
            Span::styled(format!("  {:>2}   ", index + 1), row_style),
            Span::styled(display_text, row_style),
            Span::styled(
                "● ",
                row_style
                    .fg(risk_color(command.risk))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                risk_label(command.risk),
                row_style
                    .fg(risk_color(command.risk))
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        if let Some(preview) = &command.preview {
            for preview_line in preview.lines().take(6) {
                lines.push(Line::from(Span::styled(
                    format!("      {}", truncate_text(preview_line, available_width.saturating_sub(6))),
                    Style::default().add_modifier(Modifier::DIM),
                )));
            }
        }
    }

    let commands_widget = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(commands_widget, chunks[0]);

    let estimate_secs = 2 + commands.len() as u64;
    let summary_lines = vec![
        Line::from(format!("  {} commands · estimated ~{}s", commands.len(), estimate_secs)),
        Line::from(permission_hint(mode, countdown, has_critical)),
        Line::from(match mode {
            PermissionMode::PerPlan => "  [Y] Approve all    [N] Cancel    [F2] Allow all".to_string(),
            PermissionMode::AutoApprove { .. } if has_critical => {
                "  [Y] Approve critical plan    [N] Cancel".to_string()
            }
            PermissionMode::AutoApprove { .. } => {
                "  [Y] Run now    [N] Cancel auto-approve".to_string()
            }
        }),
    ];
    frame.render_widget(Paragraph::new(summary_lines), chunks[1]);
    frame.render_widget(
        Paragraph::new(" ").style(Style::default().fg(ratatui::style::Color::DarkGray)),
        chunks[2],
    );
}

fn centered_rect(area: Rect) -> Rect {
    if area.width < 40 || area.height < 8 {
        return area;
    }

    let width = area.width.min(90);
    let height = area.height.min(18);
    Rect {
        x: area.x + (area.width.saturating_sub(width) / 2),
        y: area.y + (area.height.saturating_sub(height) / 2),
        width,
        height,
    }
}

fn try_show_explanation(text: &str) -> Result<()> {
    let total_width = terminal::size().map(|(width, _)| width as usize).unwrap_or(80).max(20);
    let content_width = total_width.saturating_sub(6).max(16);
    let lines = wrap_text(text, content_width.saturating_sub(2));
    let mut stdout = io::stdout();

    let top = format!("╭─ why {}╮", "─".repeat(content_width.saturating_sub(6)));
    let bottom = format!("╰{}╯", "─".repeat(content_width));

    execute!(
        stdout,
        SetForegroundColor(Color::DarkMagenta),
        SetAttribute(Attribute::Dim),
        Print(top),
        ResetColor,
        SetAttribute(Attribute::Reset),
        Print("\n")
    )
    .context("failed to render explanation header")?;

    for (index, line) in lines.iter().enumerate() {
        let prefix = if index == 0 { "  💡 " } else { "    " };
        let rendered = pad_to_width(&truncate_text(line, content_width.saturating_sub(4)), content_width.saturating_sub(4));
        writeln!(stdout, "│{prefix}{rendered}│")?;
    }

    execute!(
        stdout,
        SetForegroundColor(Color::DarkMagenta),
        SetAttribute(Attribute::Dim),
        Print(bottom),
        ResetColor,
        SetAttribute(Attribute::Reset),
        Print("\n")
    )?;
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
    if display_width(text) <= max_width {
        return text.to_string();
    }

    if max_width <= 1 {
        return "…".to_string();
    }

    let mut width = 0usize;
    let mut truncated = String::new();
    for character in text.chars() {
        let char_width = UnicodeWidthStr::width(character.encode_utf8(&mut [0; 4]));
        if width + char_width >= max_width {
            break;
        }
        width += char_width;
        truncated.push(character);
    }
    truncated.push('…');
    truncated
}

fn pad_to_width(text: &str, width: usize) -> String {
    let padding = width.saturating_sub(display_width(text));
    format!("{text}{}", " ".repeat(padding))
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn fit_to_width(text: &str, width: usize) -> String {
    let truncated = truncate_text(text, width);
    pad_to_width(&truncated, width)
}

fn permission_hint(mode: &PermissionMode, countdown: Option<u8>, has_critical: bool) -> String {
    match mode {
        PermissionMode::PerPlan => "  Review the plan before anything runs.".to_string(),
        PermissionMode::AutoApprove { .. } if has_critical => {
            "  Critical command detected. Explicit approval is required.".to_string()
        }
        PermissionMode::AutoApprove { .. } => format!(
            "  Auto-approving in {}s... press N to cancel.",
            countdown.unwrap_or(0)
        ),
    }
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