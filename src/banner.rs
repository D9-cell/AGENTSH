use std::io::{self, Write};

use anyhow::Result;
use crossterm::execute;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::tty::IsTty;

pub fn print_startup_banner(model: &str, runtime_name: &str, connected: bool) -> Result<()> {
    if !io::stdout().is_tty() {
        return Ok(());
    }

    let mut stdout = io::stdout();
    let art = [
        "  ╔══════════════════════════════════════════════════════════════════════════╗",
        "  ║   █████╗  ██████╗ ███████╗███╗   ██╗████████╗███████╗██╗  ██╗          ║",
        "  ║  ██╔══██╗██╔════╝ ██╔════╝████╗  ██║╚══██╔══╝██╔════╝██║  ██║          ║",
        "  ║  ███████║██║  ███╗█████╗  ██╔██╗ ██║   ██║   ███████╗███████║          ║",
        "  ║  ██╔══██║██║   ██║██╔══╝  ██║╚██╗██║   ██║   ╚════██║██╔══██║          ║",
        "  ║  ██║  ██║╚██████╔╝███████╗██║ ╚████║   ██║   ███████║██║  ██║          ║",
        "  ╚══════════════════════════════════════════════════════════════════════════╝",
    ];

    for (index, line) in art.iter().enumerate() {
        execute!(
            stdout,
            SetForegroundColor(gradient_color(index, art.len())),
            Print(*line),
            ResetColor,
            Print("\n")
        )?;
    }

    let status = if connected { "connected ✓" } else { "not detected" };
    execute!(
        stdout,
        SetAttribute(Attribute::Bold),
        Print("     Agentic terminal · powered by local LLM\n"),
        SetAttribute(Attribute::Reset),
        Print(format!("     Model: {model}  ·  {runtime_name}: {status}\n")),
        Print("     Type naturally or use shell commands directly.\n"),
        Print("     --allow-all  to enable full-permission mode\n\n")
    )?;
    stdout.flush()?;

    Ok(())
}

fn gradient_color(index: usize, total: usize) -> Color {
    if total <= 1 {
        return Color::Rgb { r: 32, g: 220, b: 220 };
    }

    let start = (32u8, 220u8, 220u8);
    let end = (72u8, 116u8, 255u8);
    let factor = index as f32 / (total.saturating_sub(1)) as f32;

    let mix = |from: u8, to: u8| -> u8 {
        (from as f32 + (to as f32 - from as f32) * factor).round() as u8
    };

    Color::Rgb {
        r: mix(start.0, end.0),
        g: mix(start.1, end.1),
        b: mix(start.2, end.2),
    }
}