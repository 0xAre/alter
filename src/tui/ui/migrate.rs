//! Rendering: Migrate screen (vault v1 → v2).

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::theme::{DIM, ERROR, TEXT, WARNING};
use super::helpers::{centered_rect_abs, d, k, render_footer};
use super::super::app::App;

pub(super) fn render_migrate(f: &mut Frame, app: &App) {
    let area = f.area();
    let card = centered_rect_abs(64, 14, area);
    f.render_widget(Clear, card);

    let phase = app.migration_phase;
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            " Migrasi Vault v1 → v2 ",
            Style::default().fg(WARNING).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Vault lama terdeteksi. ALTER perlu upgrade ke format v2",
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(
            "yang mendukung dual-slot (ALTER + Password Manager).",
            Style::default().fg(TEXT),
        )),
        Line::from(""),
    ];

    match phase {
        0 => {
            // Masukkan PM passphrase baru
            lines.push(Line::from(Span::styled(
                "Buat passphrase untuk Password Manager (decoy):",
                Style::default().fg(DIM),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  Passphrase PM  {}▏", "•".repeat(app.pm_pass_input.len())),
                Style::default().fg(TEXT),
            )));
        }
        1 => {
            // Konfirmasi PM passphrase
            lines.push(Line::from(Span::styled(
                "Konfirmasi passphrase Password Manager:",
                Style::default().fg(DIM),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  Konfirmasi     {}▏", "•".repeat(app.pm_pass_confirm.len())),
                Style::default().fg(TEXT),
            )));
        }
        _ => {
            lines.push(Line::from(Span::styled(
                "Memproses migrasi…",
                Style::default().fg(super::theme::ACCENT),
            )));
        }
    }

    lines.push(Line::from(""));
    if let Some(err) = &app.auth_error {
        lines.push(Line::from(Span::styled(
            format!("  [!] {}", err),
            Style::default().fg(ERROR),
        )));
    }

    let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(WARNING)));
    f.render_widget(para, card);

    let hint_line = Line::from(vec![
        d(" "), k("[Enter]"), d(" lanjut   "),
        k("[Esc]"), d(" keluar"),
    ]);
    render_footer(f, area, hint_line);
}
