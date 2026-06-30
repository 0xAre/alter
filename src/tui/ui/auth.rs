//! Rendering: splash, auth (unlock/create), unlocking spinner, init, onboard.

use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::theme::{ACCENT, DIM, ERROR, LOGO, SPINNER, SPINNER_LEN, TEXT};
use super::helpers::{centered_rect_abs, d, k, render_footer};
use super::super::app::App;
use super::super::types::{CreatePhase, Screen};

pub(super) fn render_logo_lines() -> Vec<Line<'static>> {
    LOGO.iter()
        .map(|row| {
            Line::from(Span::styled(
                *row,
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ))
        })
        .collect()
}

pub(super) fn render_splash(f: &mut Frame, _app: &App) {
    let area = f.area();
    f.render_widget(Clear, area);

    // Logo 6 baris + 1 gap + subtitle 1 = 8 baris total
    let card = centered_rect_abs(50, 8, area);
    let mut lines = render_logo_lines();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "secure • encrypted • sovereign",
        Style::default().fg(DIM),
    )));

    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        card,
    );
}

pub(super) fn render_auth(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Clear, area);

    let creating = app.screen == Screen::Create;

    // Logo (6) + subtitle (1) + gap (1) + vault label (1) + gap (1)
    // + passphrase field(s) + gap + error/hint = ~14-16 baris
    let card_h = if creating { 17 } else { 15 };
    let card = centered_rect_abs(52, card_h, area);

    let mut lines = render_logo_lines();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "secure • encrypted • sovereign",
        Style::default().fg(DIM),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if creating { "Buat Identitas Baru" } else { "Local Identity Vault" },
        Style::default().fg(DIM),
    )));
    lines.push(Line::from(""));

    if creating {
        // M6: 4-fase create — tampilkan field sesuai fase aktif
        use CreatePhase::*;
        let (alter_active, confirm_active, pm_active, pm_confirm_active) = match app.create_phase {
            AlterPass    => (true,  false, false, false),
            AlterConfirm => (false, true,  false, false),
            PmPass       => (false, false, true,  false),
            PmConfirm    => (false, false, false, true),
        };
        // Fase 1 & 2: ALTER passphrase
        lines.push(Line::from(Span::styled("— passphrase ALTER (rahasia) —", Style::default().fg(DIM))));
        lines.push(field_line("Passphrase  ", app.pass_input.len(), alter_active));
        lines.push(field_line("Konfirmasi  ", app.pass_confirm.len(), confirm_active));
        lines.push(Line::from(""));
        // Fase 3 & 4: PM passphrase
        lines.push(Line::from(Span::styled("— passphrase Password Manager (decoy) —", Style::default().fg(DIM))));
        lines.push(field_line("Passphrase  ", app.pm_pass_input.len(), pm_active));
        lines.push(field_line("Konfirmasi  ", app.pm_pass_confirm.len(), pm_confirm_active));
    } else {
        lines.push(Line::from(Span::styled(
            "Passphrase",
            Style::default().fg(DIM),
        )));
        lines.push(Line::from(Span::styled(
            format!("{}▏", "•".repeat(app.pass_input.len())),
            Style::default().fg(TEXT),
        )));
    }

    lines.push(Line::from(""));
    if let Some(err) = &app.auth_error {
        lines.push(Line::from(Span::styled(
            format!("[!] {err}"),
            Style::default().fg(ERROR),
        )));
    } else if creating {
        lines.push(Line::from(Span::styled(
            "Dua passphrase untuk dua mode: ALTER + Password Manager.",
            Style::default().fg(DIM),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        card,
    );

    let hint_line = if creating {
        Line::from(vec![d(" "), k("[Enter]"), d(" lanjut   "), k("[Esc]"), d(" keluar")])
    } else {
        Line::from(vec![d(" "), k("[Enter]"), d(" buka   "), k("[Esc]"), d(" keluar")])
    };
    render_footer(f, area, hint_line);
}

pub(super) fn field_line(label: &str, len: usize, active: bool) -> Line<'static> {
    let dots: String = "•".repeat(len);
    let cursor = if active { "▏" } else { "" };
    let value_style = if active {
        Style::default().fg(TEXT)
    } else {
        Style::default().fg(DIM)
    };
    Line::from(vec![
        Span::styled(label.to_string(), Style::default().fg(DIM)),
        Span::styled(format!("{dots}{cursor}"), value_style),
    ])
}

pub(super) fn render_unlocking(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Clear, area);

    let card = centered_rect_abs(44, 7, area);
    f.render_widget(Clear, card);

    let spinner = SPINNER[(app.tick_count % SPINNER_LEN) as usize];
    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {} ", spinner),
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                "Membuka vault…",
                Style::default().fg(TEXT),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Menurunkan kunci (Argon2id), harap tunggu.",
            Style::default().fg(DIM),
        )),
        Line::from(""),
    ];

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(DIM))
                .title(Span::styled(" ALTER ", Style::default().fg(ACCENT))),
        ),
        card,
    );
}

pub(super) fn render_init(f: &mut Frame, app: &App) {
    use super::theme::SUCCESS;
    let area = f.area();
    f.render_widget(Clear, area);

    let card = centered_rect_abs(52, 11, area);

    let (step_text, step_style) = match app.init_step {
        1 => ("Unlocking identity...",          Style::default().fg(TEXT)),
        2 => ("Deriving session keys...",        Style::default().fg(TEXT)),
        3 => ("Establishing secure transport...", Style::default().fg(TEXT)),
        _ => ("Runtime ready.",                  Style::default().fg(SUCCESS)),
    };

    let mut lines = render_logo_lines();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "secure • encrypted • sovereign",
        Style::default().fg(DIM),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(step_text, step_style)));

    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        card,
    );
}

pub(super) fn render_onboard(f: &mut Frame, _app: &App) {
    let area = f.area();
    f.render_widget(Clear, area);

    let card = centered_rect_abs(58, 20, area);

    let mut lines = render_logo_lines();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Selamat datang di ALTER",
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Baca sebelum mulai — ALTER berbeda:",
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  ●  ", Style::default().fg(ACCENT)),
        Span::styled("Kedua pihak harus online bersamaan.", Style::default().fg(TEXT)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("       ", Style::default()),
        Span::styled("Tidak ada notifikasi, tidak ada pesan offline.", Style::default().fg(DIM)),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  ●  ", Style::default().fg(ACCENT)),
        Span::styled("Pesan tidak tersimpan di mana pun.", Style::default().fg(TEXT)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("       ", Style::default()),
        Span::styled("Begitu sesi ditutup, pesan tidak bisa dibaca kembali.", Style::default().fg(DIM)),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  ●  ", Style::default().fg(ACCENT)),
        Span::styled("Reconnect = sesi baru.", Style::default().fg(TEXT)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("       ", Style::default()),
        Span::styled("Koneksi terputus tidak bisa dilanjutkan.", Style::default().fg(DIM)),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Ini bukan keterbatasan — ini adalah identitas ALTER.",
        Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
    )));

    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Left),
        card,
    );

    render_footer(
        f,
        area,
        Line::from(vec![d(" "), k("[Tombol apa pun]"), d(" lanjut ke aplikasi")]),
    );
}
