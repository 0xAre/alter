//! Rendering: modals — add contact, rename contact, delete confirm, invite popup, file received.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::theme::{ACCENT, DIM, ERROR, TEXT};
use super::helpers::{centered_rect_abs, centered_rect_pct, format_fingerprint};
use super::super::app::App;
use super::super::types::{FileTransferUiState, RoomState};

pub(super) fn render_add_contact_modal(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect_abs(64, 10, area);
    f.render_widget(Clear, popup);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "TAMBAH KONTAK",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Paste invite code di bawah ini.",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "(Opsional: tambah spasi diikuti nickname)",
            Style::default().fg(DIM),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(app.add_buffer.clone(), Style::default().fg(TEXT)),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ]),
        Line::from(""),
    ];

    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT)),
            ),
        popup,
    );
}

pub(super) fn render_rename_contact_modal(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect_abs(50, 8, area);
    f.render_widget(Clear, popup);

    let current = app.contacts.get(app.selected).map(|c| c.nickname.as_str()).unwrap_or("");

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "GANTI NAMA KONTAK",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  Saat ini: {current}"),
            Style::default().fg(DIM),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(app.rename_buffer.clone(), Style::default().fg(TEXT)),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ]),
        Line::from(""),
    ];

    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT)),
            ),
        popup,
    );
}

pub(super) fn render_delete_confirm(f: &mut Frame, app: &App, area: Rect) {
    let Some(idx) = app.pending_delete else { return };
    let nick = app.contacts.get(idx).map(|c| c.nickname.as_str()).unwrap_or("kontak ini");
    let popup = centered_rect_abs(50, 6, area);
    f.render_widget(Clear, popup);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  Hapus '{nick}'?"),
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  [y] konfirmasi   tombol lain batal",
            Style::default().fg(DIM),
        )),
        Line::from(""),
    ];

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ERROR)),
        ),
        popup,
    );
}

/// FT-01: modal prompt path file saat Mode::SendFile aktif.
pub(super) fn render_send_file_modal(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect_abs(64, 10, area);
    f.render_widget(Clear, popup);

    let (border_color, hint) = if !app.peer_ft_capable {
        (ERROR, "  Peer belum mendukung file transfer.")
    } else if app.room != RoomState::Open {
        (ERROR, "  Sesi tidak aktif.")
    } else {
        (ACCENT, "  Masukkan path file yang akan dikirim:")
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "KIRIM FILE",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(hint, Style::default().fg(DIM))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(app.send_file_buffer.clone(), Style::default().fg(TEXT)),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  [Enter] kirim   [Esc] batal",
            Style::default().fg(DIM),
        )),
    ];

    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color)),
            ),
        popup,
    );
}

/// FT-01: modal prompt setelah file diterima dan SHA-256 terverifikasi.
/// Menunggu pilihan user: [S] Simpan  [L] Lihat inline (image saja)  [T] Tolak.
pub(super) fn render_file_received_modal(f: &mut Frame, app: &App, area: Rect) {
    let FileTransferUiState::Received { name, data, is_image } = &app.file_transfer else { return };

    let size_str = {
        let bytes = data.len();
        if bytes >= 1_048_576 {
            format!("{:.1} MB", bytes as f64 / 1_048_576.0)
        } else {
            format!("{} KB", bytes / 1024)
        }
    };

    let peer = app.peer_name.as_deref().unwrap_or("Peer");
    let mime = infer::get(data)
        .map(|k| k.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let keys_hint = if *is_image {
        "  [S] Simpan   [L] Lihat inline   [T] Tolak"
    } else {
        "  [S] Simpan ke disk   [T] Tolak"
    };

    let height: u16 = if *is_image { 10 } else { 10 };
    let popup = centered_rect_abs(64, height, area);
    f.render_widget(Clear, popup);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "TRANSFER FILE MASUK",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  ◎ {peer} mengirim: {name}"),
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(
            format!("  {size_str} · {mime}"),
            Style::default().fg(DIM),
        )),
        Line::from(""),
        Line::from(Span::styled(keys_hint, Style::default().fg(ACCENT))),
        Line::from(""),
    ];

    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT))
                    .title(" Transfer File "),
            ),
        popup,
    );
}

pub(super) fn render_invite_popup(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect_pct(72, 50, area);
    f.render_widget(Clear, popup);

    let invite = app.keys.as_ref().map(|k| k.invite.clone()).unwrap_or_default();
    let fp = app.keys.as_ref().map(|k| k.fingerprint.clone()).unwrap_or_default();

    // Format invite code: bagi per 44 karakter
    let mut invite_lines: Vec<Line> = Vec::new();
    let chunks_44: Vec<&str> = invite
        .as_bytes()
        .chunks(44)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    for chunk in &chunks_44 {
        invite_lines.push(Line::from(Span::styled(
            chunk.to_string(),
            Style::default().fg(ACCENT),
        )));
    }

    let mut text = vec![
        Line::from(Span::styled(
            "Bagikan invite code via channel aman lain:",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    text.extend(invite_lines);
    text.push(Line::from(""));
    text.push(Line::from(Span::styled(
        format!("Fingerprint: {}", format_fingerprint(&fp)),
        Style::default().fg(DIM),
    )));
    if !app.tor_active() {
        text.push(Line::from(""));
        let note = if app.tor_connecting {
            "Tor sedang menyambung… onion address akan muncul di invite saat siap."
        } else {
            "(LAN-only — mode --offline. Tanpa --offline, invite otomatis menyertakan onion.)"
        };
        text.push(Line::from(Span::styled(note, Style::default().fg(DIM))));
    }
    text.push(Line::from(""));
    text.push(Line::from(Span::styled(
        "[c] salin ke clipboard   [i] tutup",
        Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
    )));

    f.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT))
                    .title(" Identitas Saya "),
            )
            .wrap(Wrap { trim: false }),
        popup,
    );
}
