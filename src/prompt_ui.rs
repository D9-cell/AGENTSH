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
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear as ClearWidget, Paragraph, Wrap};
use ratatui::Terminal;
use unicode_width::UnicodeWidthStr;

use crate::context::PermissionMode;
use crate::safety::RiskLevel;
use crate::tools::PlannedCommand;

pub enum PermissionDecision {
    Approve,
    Cancel,
    EnableAutoApprove,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PanelAction {
    Approve,
    Cancel,
    AllowAll,
}

pub fn show_permission_panel(commands: &[PlannedCommand], mode: &PermissionMode) -> PermissionDecision {
    try_show_permission_panel(commands, mode).unwrap_or_else(|error| {
        print_error(&error.to_string());
        PermissionDecision::Cancel
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
        " AgentSH  {}  {}  {} ",
        model,
        runtime_name,
        shorten_path(cwd),
    );
    let available_width = usize::from(width).saturating_sub(display_width(mode_label) + 1);
    let fitted = fit_to_width(&bar_text, available_width);

    let mut stdout = io::stdout();
    execute!(
        stdout,
        MoveTo(0, height.saturating_sub(1)),
        SetBackgroundColor(Color::Rgb { r: 18, g: 22, b: 32 }),
        SetForegroundColor(Color::Rgb { r: 210, g: 218, b: 230 }),
        Clear(ClearType::CurrentLine),
        Print(fitted),
        SetForegroundColor(match mode {
            PermissionMode::PerPlan => Color::Rgb { r: 148, g: 163, b: 184 },
            PermissionMode::AutoApprove { .. } => Color::Rgb { r: 251, g: 191, b: 36 },
        }),
        SetAttribute(Attribute::Bold),
        Print(mode_label),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    Ok(())
}

fn try_show_permission_panel(commands: &[PlannedCommand], mode: &PermissionMode) -> Result<PermissionDecision> {
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
    let mut selected_action = default_action(mode, has_critical);

    let ui_result = (|| -> Result<PermissionDecision> {
        loop {
            terminal.draw(|frame| {
                draw_permission_panel(frame, commands, mode, countdown, has_critical, selected_action)
            })?;

            if let Some(remaining) = countdown {
                let timeout = Duration::from_millis(100);
                if event::poll(timeout).context("failed to poll terminal input")? {
                    match event::read().context("failed to read terminal input")? {
                        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                            KeyCode::Enter => return Ok(resolve_action(selected_action)),
                            KeyCode::Tab | KeyCode::Char('\t') | KeyCode::Right => {
                                selected_action = next_action(selected_action, mode, has_critical)
                            }
                            KeyCode::BackTab | KeyCode::Left => {
                                selected_action = previous_action(selected_action, mode, has_critical)
                            }
                            KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(PermissionDecision::Approve),
                            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                                return Ok(PermissionDecision::Cancel)
                            }
                            KeyCode::F(2) if !has_critical => {
                                return Ok(PermissionDecision::EnableAutoApprove)
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }

                if last_tick.elapsed() >= Duration::from_secs(1) {
                    if remaining == 0 {
                        return Ok(PermissionDecision::Approve);
                    }

                    countdown = Some(remaining.saturating_sub(1));
                    last_tick = Instant::now();
                    if countdown == Some(0) {
                        return Ok(PermissionDecision::Approve);
                    }
                }
                continue;
            }

            match event::read().context("failed to read terminal input")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter => return Ok(resolve_action(selected_action)),
                    KeyCode::Tab | KeyCode::Char('\t') | KeyCode::Right => {
                        selected_action = next_action(selected_action, mode, has_critical)
                    }
                    KeyCode::BackTab | KeyCode::Left => {
                        selected_action = previous_action(selected_action, mode, has_critical)
                    }
                    KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(PermissionDecision::Approve),
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        return Ok(PermissionDecision::Cancel)
                    }
                    KeyCode::F(2) if !has_critical => return Ok(PermissionDecision::EnableAutoApprove),
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
    selected_action: PanelAction,
) {
    let area = centered_rect(frame.size());
    frame.render_widget(ClearWidget, area);
    let block = Block::default()
        .title(Span::styled(
            " AgentSH Plan ",
            Style::default()
                .fg(ratatui::style::Color::Rgb(96, 165, 250))
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(ratatui::style::Color::Rgb(15, 23, 42)))
        .border_style(Style::default().fg(ratatui::style::Color::Rgb(51, 65, 85)))
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
        .constraints([
            Constraint::Length(2),
            Constraint::Min(7),
            Constraint::Length(2),
            Constraint::Length(3),
        ])
        .split(inner);

    let meta = Line::from(vec![
        Span::styled(
            " Workspace ",
            Style::default()
                .bg(ratatui::style::Color::Rgb(30, 41, 59))
                .fg(ratatui::style::Color::Rgb(148, 163, 184)),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{} command{} queued", commands.len(), if commands.len() == 1 { "" } else { "s" }),
            Style::default().fg(ratatui::style::Color::Rgb(203, 213, 225)),
        ),
    ]);
    frame.render_widget(Paragraph::new(meta), chunks[0]);

    let available_width = inner.width.saturating_sub(4).max(28) as usize;
    let command_width = available_width.saturating_sub(18).max(12);
    let mut lines = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        let row_style = if command.risk == RiskLevel::Critical {
            Style::default().bg(ratatui::style::Color::Rgb(69, 10, 10))
        } else {
            Style::default().bg(ratatui::style::Color::Rgb(17, 24, 39))
        };
        let display_text = pad_to_width(&truncate_text(&command.display_text, command_width), command_width);
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:>2}   ", index + 1),
                row_style.fg(ratatui::style::Color::Rgb(148, 163, 184)),
            ),
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
                    Style::default()
                        .bg(ratatui::style::Color::Rgb(15, 23, 42))
                        .fg(ratatui::style::Color::Rgb(125, 140, 160))
                        .add_modifier(Modifier::DIM),
                )));
            }
        }

        if index + 1 < commands.len() {
            lines.push(Line::from(Span::styled(
                " ",
                Style::default().bg(ratatui::style::Color::Rgb(15, 23, 42)),
            )));
        }
    }

    let commands_widget = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(commands_widget, chunks[1]);

    let estimate_secs = 2 + commands.len() as u64;
    let summary_lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  {} commands", commands.len()),
                Style::default().fg(ratatui::style::Color::Rgb(226, 232, 240)),
            ),
            Span::styled(
                format!("  ·  estimated ~{}s", estimate_secs),
                Style::default().fg(ratatui::style::Color::Rgb(148, 163, 184)),
            ),
        ]),
        Line::from(permission_hint(mode, countdown, has_critical)),
    ];
    frame.render_widget(Paragraph::new(summary_lines), chunks[2]);

    let buttons = button_row(button_specs(mode, has_critical), selected_action, chunks[3].width as usize);
    frame.render_widget(
        Paragraph::new(buttons)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
        chunks[3],
    );
}

fn centered_rect(area: Rect) -> Rect {
    if area.width < 40 || area.height < 8 {
        return area;
    }

    let width = area.width.min(96);
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
    let content_width = total_width.saturating_sub(8).max(16);
    let lines = wrap_text(text, content_width.saturating_sub(2));
    let mut stdout = io::stdout();

    let top = format!("╭─ Insight {}╮", "─".repeat(content_width.saturating_sub(10)));
    let bottom = format!("╰{}╯", "─".repeat(content_width));

    execute!(
        stdout,
        SetForegroundColor(Color::Rgb { r: 139, g: 92, b: 246 }),
        SetAttribute(Attribute::Bold),
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
        SetForegroundColor(Color::Rgb { r: 139, g: 92, b: 246 }),
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
        PermissionMode::PerPlan => {
            "  Tab or arrow keys move focus. Enter confirms the highlighted action.".to_string()
        }
        PermissionMode::AutoApprove { .. } if has_critical => {
            "  Critical command detected. Explicit approval is required.".to_string()
        }
        PermissionMode::AutoApprove { .. } => format!(
            "  Auto-approving in {}s... press N to cancel or Tab to switch action.",
            countdown.unwrap_or(0)
        ),
    }
}

fn button_specs(mode: &PermissionMode, has_critical: bool) -> Vec<(PanelAction, &'static str)> {
    match mode {
        PermissionMode::PerPlan => vec![
            (PanelAction::Approve, "Y Approve"),
            (PanelAction::Cancel, "N Cancel"),
            (PanelAction::AllowAll, "F2 Allow All"),
        ],
        PermissionMode::AutoApprove { .. } if has_critical => vec![
            (PanelAction::Approve, "Y Approve Critical"),
            (PanelAction::Cancel, "N Cancel"),
        ],
        PermissionMode::AutoApprove { .. } => vec![
            (PanelAction::Approve, "Y Run Now"),
            (PanelAction::Cancel, "N Cancel"),
        ],
    }
}

fn button_row(
    buttons: Vec<(PanelAction, &'static str)>,
    selected_action: PanelAction,
    width: usize,
) -> Line<'static> {
    let mut spans = Vec::new();
    let total_label_width = buttons
        .iter()
        .map(|(_, label)| display_width(label) + 4)
        .sum::<usize>()
        + buttons.len().saturating_sub(1) * 2;
    let leading_padding = width.saturating_sub(total_label_width) / 2;
    if leading_padding > 0 {
        spans.push(Span::raw(" ".repeat(leading_padding)));
    }

    for (index, (action, label)) in buttons.iter().enumerate() {
        let selected = *action == selected_action;
        spans.push(Span::styled(
            format!(" {} ", label),
            if selected {
                Style::default()
                    .bg(button_color(*action))
                    .fg(ratatui::style::Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .bg(ratatui::style::Color::Rgb(30, 41, 59))
                    .fg(ratatui::style::Color::Rgb(203, 213, 225))
            },
        ));
        if index + 1 < buttons.len() {
            spans.push(Span::raw("  "));
        }
    }

    Line::from(spans)
}

fn button_color(action: PanelAction) -> ratatui::style::Color {
    match action {
        PanelAction::Approve => ratatui::style::Color::Rgb(52, 211, 153),
        PanelAction::Cancel => ratatui::style::Color::Rgb(248, 113, 113),
        PanelAction::AllowAll => ratatui::style::Color::Rgb(251, 191, 36),
    }
}

fn default_action(mode: &PermissionMode, has_critical: bool) -> PanelAction {
    match mode {
        PermissionMode::PerPlan => PanelAction::Approve,
        PermissionMode::AutoApprove { .. } if has_critical => PanelAction::Approve,
        PermissionMode::AutoApprove { .. } => PanelAction::Cancel,
    }
}

fn next_action(current: PanelAction, mode: &PermissionMode, has_critical: bool) -> PanelAction {
    cycle_action(current, mode, has_critical, true)
}

fn previous_action(current: PanelAction, mode: &PermissionMode, has_critical: bool) -> PanelAction {
    cycle_action(current, mode, has_critical, false)
}

fn cycle_action(current: PanelAction, mode: &PermissionMode, has_critical: bool, forward: bool) -> PanelAction {
    let actions = button_specs(mode, has_critical)
        .into_iter()
        .map(|(action, _)| action)
        .collect::<Vec<_>>();
    let index = actions.iter().position(|action| *action == current).unwrap_or(0);
    let next_index = if forward {
        (index + 1) % actions.len()
    } else if index == 0 {
        actions.len() - 1
    } else {
        index - 1
    };
    actions[next_index]
}

fn resolve_action(action: PanelAction) -> PermissionDecision {
    match action {
        PanelAction::Approve => PermissionDecision::Approve,
        PanelAction::Cancel => PermissionDecision::Cancel,
        PanelAction::AllowAll => PermissionDecision::EnableAutoApprove,
    }
}

fn shorten_path(path: &Path) -> String {
    let display = path.display().to_string();
    if display_width(&display) <= 28 {
        return display;
    }

    let mut parts = display.split('/').filter(|segment| !segment.is_empty()).collect::<Vec<_>>();
    if parts.len() < 2 {
        return truncate_text(&display, 28);
    }

    let tail = parts.split_off(parts.len().saturating_sub(2));
    format!("…/{}/{}", tail[0], tail[1])
}

#[cfg(test)]
mod tests {
    use super::{
        cycle_action, default_action, resolve_action, PanelAction, PermissionDecision,
    };
    use crate::context::PermissionMode;

    #[test]
    fn cycles_through_per_plan_actions() {
        let next = cycle_action(PanelAction::Approve, &PermissionMode::PerPlan, false, true);
        assert!(matches!(next, PanelAction::Cancel));

        let wrapped = cycle_action(PanelAction::AllowAll, &PermissionMode::PerPlan, false, true);
        assert!(matches!(wrapped, PanelAction::Approve));
    }

    #[test]
    fn critical_auto_mode_hides_allow_all() {
        let default = default_action(&PermissionMode::AutoApprove { countdown_secs: 2 }, true);
        assert!(matches!(default, PanelAction::Approve));

        let next = cycle_action(default, &PermissionMode::AutoApprove { countdown_secs: 2 }, true, true);
        assert!(matches!(next, PanelAction::Cancel));
    }

    #[test]
    fn resolve_allow_all_action() {
        assert!(matches!(
            resolve_action(PanelAction::AllowAll),
            PermissionDecision::EnableAutoApprove
        ));
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