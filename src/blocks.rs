use std::io::{self, Write};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use unicode_width::UnicodeWidthStr;

const MAX_VISIBLE_LINES: usize = 40;

pub fn print_command_block(
    command: &str,
    output: &str,
    exit_code: Option<i32>,
    elapsed: Duration,
) -> Result<()> {
    let lines = output_lines(output);
    if lines.len() > MAX_VISIBLE_LINES {
        return page_output(command, &lines, exit_code, elapsed);
    }

    draw_block(&lines, command, exit_code, elapsed, None)
}

fn page_output(
    command: &str,
    lines: &[String],
    exit_code: Option<i32>,
    elapsed: Duration,
) -> Result<()> {
    let (_, terminal_height) = terminal::size().unwrap_or((80, 24));
    let window_size = usize::from(terminal_height).saturating_sub(6).clamp(5, MAX_VISIBLE_LINES);
    let mut offset = 0usize;
    let mut stdout = io::stdout();

    terminal::enable_raw_mode().context("failed to enable raw mode for pager")?;
    execute!(stdout, EnterAlternateScreen, Hide).context("failed to enter pager screen")?;

    let ui_result = (|| -> Result<()> {
        loop {
            let visible = page_window(lines, offset, window_size);
            draw_block(&visible, command, exit_code, elapsed, Some((offset, lines.len())))?;

            match event::read().context("failed to read pager input")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Up => {
                        offset = offset.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        let max_offset = lines.len().saturating_sub(window_size);
                        offset = (offset + 1).min(max_offset);
                    }
                    KeyCode::PageUp => {
                        offset = offset.saturating_sub(window_size);
                    }
                    KeyCode::PageDown => {
                        let max_offset = lines.len().saturating_sub(window_size);
                        offset = (offset + window_size).min(max_offset);
                    }
                    KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => return Ok(()),
                    _ => {}
                },
                _ => {}
            }
        }
    })();

    terminal::disable_raw_mode().ok();
    execute!(stdout, LeaveAlternateScreen, Show).ok();
    ui_result
}

fn draw_block(
    lines: &[String],
    command: &str,
    exit_code: Option<i32>,
    elapsed: Duration,
    pager_state: Option<(usize, usize)>,
) -> Result<()> {
    let (width, _) = terminal::size().unwrap_or((80, 24));
    let width = usize::from(width).max(40);
    let content_width = width.saturating_sub(4).max(10);
    let border_color = if exit_code.unwrap_or(1) == 0 {
        Color::DarkCyan
    } else {
        Color::DarkRed
    };

    let header = header_line(command, exit_code, elapsed, width);
    let footer = format!("╰{}╯", "─".repeat(width.saturating_sub(2)));
    let mut stdout = io::stdout();

    if pager_state.is_some() {
        execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;
    }

    execute!(
        stdout,
        SetForegroundColor(border_color),
        SetAttribute(Attribute::Dim),
        Print(&header),
        Print("\n")
    )?;

    for line in lines {
        let fitted = fit_line(line, content_width);
        execute!(stdout, Print("│ "), ResetColor, Print(&fitted), Print(" "))?;
        execute!(
            stdout,
            SetForegroundColor(border_color),
            SetAttribute(Attribute::Dim),
            Print("│\n")
        )?;
    }

    if let Some((offset, total)) = pager_state {
        let hint = format!("scroll ↑/↓ · q to close · {}-{} of {}", offset + 1, offset + lines.len(), total);
        let fitted = fit_line(&hint, content_width);
        execute!(stdout, Print("│ "), ResetColor, Print(&fitted), Print(" "))?;
        execute!(
            stdout,
            SetForegroundColor(border_color),
            SetAttribute(Attribute::Dim),
            Print("│\n")
        )?;
    }

    execute!(stdout, Print(&footer), ResetColor, SetAttribute(Attribute::Reset), Print("\n"))?;
    stdout.flush().context("failed to flush command block")?;
    Ok(())
}

fn page_window(lines: &[String], offset: usize, window_size: usize) -> Vec<String> {
    let end = (offset + window_size).min(lines.len());
    let mut visible = lines[offset..end].to_vec();

    if offset > 0 {
        visible.insert(0, format!("... ({} lines above) ...", offset));
    }
    if end < lines.len() {
        visible.push(format!("... ({} more lines) ...", lines.len() - end));
    }

    visible
}

fn header_line(command: &str, exit_code: Option<i32>, elapsed: Duration, width: usize) -> String {
    let header_width = width.saturating_sub(2);
    let exit_text = format!("exit {} · {:.1}s", exit_code.unwrap_or(-1), elapsed.as_secs_f32());
    let command_text = truncate_text(command, 40);
    let left = format!("─ {command_text} ");
    let right = format!(" {exit_text} ─");
    let filler_width = header_width.saturating_sub(display_width(&left) + display_width(&right));
    format!("╭{left}{}{right}╮", "─".repeat(filler_width))
}

fn output_lines(output: &str) -> Vec<String> {
    if output.trim().is_empty() {
        return vec!["(no output)".to_string()];
    }

    output.lines().map(str::to_string).collect()
}

fn fit_line(line: &str, width: usize) -> String {
    let truncated = truncate_text(line, width);
    let padding = width.saturating_sub(display_width(&truncated));
    format!("{truncated}{}", " ".repeat(padding))
}

fn truncate_text(text: &str, max_width: usize) -> String {
    if display_width(text) <= max_width {
        return text.to_string();
    }

    if max_width <= 1 {
        return "…".to_string();
    }

    let mut width = 0usize;
    let mut result = String::new();
    for character in text.chars() {
        let char_width = character.to_string().width();
        if width + char_width >= max_width {
            break;
        }
        width += char_width;
        result.push(character);
    }
    result.push('…');
    result
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}