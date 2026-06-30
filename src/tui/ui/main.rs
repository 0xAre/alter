//! Rendering: main screen — header, contacts, chat panel, idle panel.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use super::theme::{
    ACCENT, DIM, ERROR, LOGO_SMALL, SPINNER, SPINNER_LEN, SUCCESS, TEXT, WARNING,
};
use super::helpers::{
    footer_hint, format_fingerprint, format_fingerprint_short, render_footer, truncate_nick,
};
use super::modals::{
    render_add_contact_modal, render_delete_confirm, render_file_received_modal,
    render_invite_popup, render_rename_contact_modal, render_send_file_modal,
};
use super::super::app::App;
use super::super::types::{FileTransferUiState, Mode, NotifLevel, RoomState, Who};
use super::super::chat::parse_reply_text;

pub(super) fn render_main(f: &mut Frame, app: &App) {
    let area = f.area();

    // Layout vertikal: header | separator | body | footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // header (3 logo + 1 status bar)
            Constraint::Length(1), // separator line
            Constraint::Min(3),    // body
            Constraint::Length(1), // footer
        ])
        .split(area);

    render_header(f, app, chunks[0]);
    render_separator(f, chunks[1]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(26), Constraint::Min(10)])
        .split(chunks[2]);

    render_contacts(f, app, body[0]);
    render_right_panel(f, app, body[1]);
    render_footer(f, area, footer_hint(app));

    if app.mode == Mode::AddContact {
        render_add_contact_modal(f, app, area);
    }
    if app.mode == Mode::RenameContact {
        render_rename_contact_modal(f, app, area);
    }
    if app.show_invite {
        render_invite_popup(f, app, area);
    }
    if app.pending_delete.is_some() {
        render_delete_confirm(f, app, area);
    }
    if app.mode == Mode::SendFile {
        render_send_file_modal(f, app, area);
    }
    if matches!(&app.file_transfer, FileTransferUiState::Received { .. }) {
        render_file_received_modal(f, app, area);
    }
}

pub(super) fn render_separator(f: &mut Frame, area: Rect) {
    let line = "─".repeat(area.width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(line, Style::default().fg(DIM))),
        area,
    );
}

pub(super) fn render_header(f: &mut Frame, app: &App, area: Rect) {
    // Bagi header: blok logo (3 baris) | status bar (1 baris)
    let vrows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(1)])
        .split(area);

    // ── Logo block (3 baris) ──────────────────────────────────────────────────
    let logo_lines: Vec<Line> = LOGO_SMALL
        .iter()
        .map(|row| {
            Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    *row,
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(logo_lines), vrows[0]);

    // ── Status bar: [pill transport]   <status sesi>            fingerprint ──────
    let fp_short = app
        .keys
        .as_ref()
        .map(|k| format_fingerprint_short(&k.fingerprint))
        .unwrap_or_default();

    // Transport sebagai PILL (reverse-video) — tampak modern, bukan teks polos.
    let pill = |label: String, bg: Color| {
        Span::styled(
            label,
            Style::default()
                .fg(Color::Black)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )
    };
    let transport_pill: Span = if app.tor_restarting {
        let spinner = SPINNER[(app.tick_count % SPINNER_LEN) as usize];
        pill(format!(" {spinner} UPDATING "), WARNING)
    } else if app.tor_active() {
        pill(" ◉ ONLINE ".to_string(), ACCENT)
    } else if app.tor_connecting {
        let spinner = SPINNER[(app.tick_count % SPINNER_LEN) as usize];
        pill(format!(" {spinner} LINKING "), WARNING)
    } else {
        pill(" ⌂ LOCAL ".to_string(), Color::Gray)
    };

    // Status sesi / notifikasi — pakai dot berwarna semantik.
    let status_span: Span = if let Some(notif) = &app.notification {
        let color = match notif.level {
            NotifLevel::Error => ERROR,
            NotifLevel::Warn => WARNING,
            NotifLevel::Success => SUCCESS,
            NotifLevel::Info => DIM,
        };
        Span::styled(notif.text.clone(), Style::default().fg(color))
    } else {
        match app.room {
            RoomState::None => Span::styled("○ idle", Style::default().fg(DIM)),
            RoomState::Connecting | RoomState::Handshaking => {
                let spinner = SPINNER[(app.tick_count % SPINNER_LEN) as usize];
                Span::styled(
                    format!("{spinner} {}", app.peer_name.as_deref().unwrap_or("peer")),
                    Style::default().fg(WARNING),
                )
            }
            RoomState::Open => Span::styled(
                format!("● {}", app.peer_name.as_deref().unwrap_or("peer")),
                Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
            ),
            RoomState::PeerLeft | RoomState::Closed => {
                Span::styled("✕ sesi ditutup", Style::default().fg(ERROR))
            }
        }
    };

    // Kiri (pill + status) | kanan (fingerprint, rata kanan).
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(16)])
        .split(vrows[1]);

    // SEC-06: obfs4 badge — tampil hanya jika obfs4proxy ditemukan di PATH.
    let obfs4_label = app.obfs4_status.badge_label();
    let obfs4_span = if !obfs4_label.is_empty() {
        Span::styled(
            obfs4_label,
            Style::default().fg(Color::Black).bg(DIM).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let left = Line::from(vec![
        Span::raw(" "),
        transport_pill,
        obfs4_span,
        Span::raw("   "),
        status_span,
    ]);
    f.render_widget(Paragraph::new(left), cols[0]);

    let right = Line::from(Span::styled(
        format!("{fp_short} "),
        Style::default().fg(DIM),
    ));
    f.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[1]);
}

pub(super) fn render_contacts(f: &mut Frame, app: &App, area: Rect) {
    // Title row
    let title_area = Rect { height: 1, ..area };
    let list_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };

    f.render_widget(
        Paragraph::new(Span::styled(
            "CONTACTS",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        title_area,
    );

    let items: Vec<ListItem> = if app.contacts.is_empty() {
        vec![
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(Span::styled(
                "  Belum ada kontak.",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            ))),
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("[a]", Style::default().fg(ACCENT)),
                Span::styled(" untuk menambah.", Style::default().fg(DIM)),
            ])),
        ]
    } else {
        app.contacts
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let selected = i == app.selected;
                let marker = if c.onion.is_some() { "◎" } else { "○" };
                let nick = truncate_nick(&c.nickname, 18);

                if selected {
                    ListItem::new(Line::from(vec![
                        Span::styled("▸ ", Style::default().fg(ACCENT)),
                        Span::styled(marker, Style::default().fg(ACCENT)),
                        Span::styled(
                            format!("  {nick}"),
                            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                        ),
                    ]))
                } else {
                    let m_style = if c.onion.is_some() {
                        Style::default().fg(ACCENT)
                    } else {
                        Style::default().fg(DIM)
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(marker, m_style),
                        Span::styled(format!("  {nick}"), Style::default().fg(TEXT)),
                    ]))
                }
            })
            .collect()
    };

    // Sidebar: hanya border kanan sebagai pemisah
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(DIM)),
    );
    f.render_widget(list, list_area);
}

pub(super) fn render_right_panel(f: &mut Frame, app: &App, area: Rect) {
    match app.mode {
        Mode::Browsing | Mode::AddContact | Mode::RenameContact => render_idle_panel(f, app, area),
        // FT-01: SendFile tetap render panel chat (modal prompt di-overlay Fase 2).
        Mode::InRoom | Mode::SendFile => render_chat_panel(f, app, area),
    }
}

pub(super) fn render_idle_panel(f: &mut Frame, app: &App, area: Rect) {
    if app.contacts.is_empty() {
        // Empty state guidance
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Mulai dengan:",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  1.  ", Style::default().fg(ACCENT)),
                Span::styled("Tekan [i] untuk melihat identitasmu.", Style::default().fg(DIM)),
            ]),
            Line::from(vec![
                Span::styled("  2.  ", Style::default().fg(ACCENT)),
                Span::styled("Bagikan invite code ke peer via channel aman lain.", Style::default().fg(DIM)),
            ]),
            Line::from(vec![
                Span::styled("  3.  ", Style::default().fg(ACCENT)),
                Span::styled("Minta peer membagikan invite-nya.", Style::default().fg(DIM)),
            ]),
            Line::from(vec![
                Span::styled("  4.  ", Style::default().fg(ACCENT)),
                Span::styled("Tekan [a] dan paste invite code peer.", Style::default().fg(DIM)),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), area);
    } else if !app.contacts.is_empty() {
        // Contact detail panel untuk kontak yang dipilih
        let c = &app.contacts[app.selected];
        let transport_line = if c.onion.is_some() {
            Line::from(vec![
                Span::styled("  ◎", Style::default().fg(ACCENT)),
                Span::styled("  LAN + ", Style::default().fg(DIM)),
                Span::styled("Tor", Style::default().fg(ACCENT)),
            ])
        } else {
            Line::from(Span::styled("  ○  LAN", Style::default().fg(DIM)))
        };
        let fp = format_fingerprint(&hex::encode(c.ed25519_pub));
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", c.nickname),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            transport_line,
            Line::from(""),
            Line::from(Span::styled("  Fingerprint:", Style::default().fg(DIM))),
            Line::from(Span::styled(
                format!("  {fp}"),
                Style::default().fg(DIM),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  [Enter] buka sesi   [r] ubah nama   [d] hapus",
                Style::default().fg(DIM),
            )),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}

pub(super) fn render_chat_panel(f: &mut Frame, app: &App, area: Rect) {
    // Split: title | messages | ft-status | separator | input
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title row (selaras dengan "CONTACTS")
            Constraint::Min(1),    // chat messages
            Constraint::Length(1), // FT-01: status transfer aktif
            Constraint::Length(1), // separator
            Constraint::Length(1), // input line
        ])
        .split(area);

    // Title row — mirip "CONTACTS" di sidebar
    let peer = app.peer_name.as_deref().unwrap_or("peer");
    let peer_has_tor = app.contacts.iter()
        .find(|c| c.nickname == peer)
        .map(|c| c.onion.is_some())
        .unwrap_or(false);
    let (marker, marker_style) = if peer_has_tor {
        ("◎", Style::default().fg(ACCENT))
    } else {
        ("○", Style::default().fg(DIM))
    };
    let title_spans: Vec<Span> = match app.room {
        RoomState::Open => vec![
            Span::styled("SESI", Style::default().fg(DIM)),
            Span::styled("  ·  ", Style::default().fg(DIM)),
            Span::styled(marker, marker_style),
            Span::styled(
                format!("  {peer}"),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ●", Style::default().fg(SUCCESS)),
        ],
        RoomState::Connecting | RoomState::Handshaking => vec![
            Span::styled("SESI", Style::default().fg(DIM)),
            Span::styled("  ·  ", Style::default().fg(DIM)),
            Span::styled(marker, marker_style),
            Span::styled(format!("  {peer}"), Style::default().fg(WARNING)),
        ],
        RoomState::PeerLeft | RoomState::Closed => vec![
            Span::styled("SESI", Style::default().fg(DIM)),
            Span::styled("  ·  ", Style::default().fg(DIM)),
            Span::styled(marker, marker_style),
            Span::styled(format!("  {peer}"), Style::default().fg(ERROR)),
            Span::styled("  ✕", Style::default().fg(ERROR)),
        ],
        RoomState::None => vec![
            Span::styled("SESI", Style::default().fg(DIM)),
        ],
    };
    f.render_widget(Paragraph::new(Line::from(title_spans)), rows[0]);

    // Chat messages — dengan scroll support
    let inner_h = rows[1].height as usize;
    let total_msgs = app.messages.len();

    // Hitung total visual lines (pesan reply = 2 baris, pesan biasa = 1 baris)
    let total_visual: usize = app.messages.iter()
        .map(|m| if parse_reply_text(&m.text).is_some() { 2 } else { 1 })
        .sum();

    // Clamp scroll agar tidak melebihi riwayat yang tersedia
    let max_scroll = total_visual.saturating_sub(inner_h);
    let effective_scroll = app.chat_scroll.min(max_scroll);

    // Cari pesan mana yang harus mulai ditampilkan (dari bawah + scroll offset)
    let mut visual_from_bottom = 0usize;
    let mut start_msg_idx = total_msgs;
    for (i, msg) in app.messages.iter().enumerate().rev() {
        let msg_lines = if parse_reply_text(&msg.text).is_some() { 2 } else { 1 };
        if visual_from_bottom >= inner_h + effective_scroll {
            start_msg_idx = i + 1;
            break;
        }
        visual_from_bottom += msg_lines;
        if i == 0 {
            start_msg_idx = 0;
        }
    }

    let mut lines: Vec<Line> = Vec::new();

    // Indikator scroll atas — tampilkan berapa pesan di atas yang tidak terlihat
    if effective_scroll > 0 {
        let hidden = total_msgs.saturating_sub(start_msg_idx + inner_h);
        lines.push(Line::from(Span::styled(
            format!("  ↑  {} pesan lebih atas  [PageUp/PageDown]", hidden.max(effective_scroll)),
            Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
        )));
    }

    // Render messages dalam window
    for msg in &app.messages[start_msg_idx..] {
        lines.extend(render_chat_line(msg));
    }

    // Connecting/Handshaking spinner
    if matches!(app.room, RoomState::Connecting | RoomState::Handshaking) {
        let spinner = SPINNER[(app.tick_count % SPINNER_LEN) as usize];
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {spinner}  Menghubungkan ke {peer}…"),
                Style::default().fg(WARNING),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Menunggu peer masuk sesi. Peer harus juga menekan Enter.",
            Style::default().fg(DIM),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[1]);

    // rows[2]: FT status, atau reply context bila tidak ada transfer aktif
    let ft_line = match &app.file_transfer {
        FileTransferUiState::None | FileTransferUiState::Prompting(..) => {
            if let Some(ref quote) = app.replying_to {
                Line::from(vec![
                    Span::styled("  ↩ ", Style::default().fg(ACCENT)),
                    Span::styled("Membalas: ", Style::default().fg(DIM)),
                    Span::styled(
                        format!("\"{quote}\""),
                        Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
                    ),
                    Span::styled("  [Esc batal]", Style::default().fg(DIM)),
                ])
            } else {
                Line::from("")
            }
        }
        FileTransferUiState::Received { name, is_image, .. } => {
            let hint = if *is_image { "[S] [L] [T]" } else { "[S] [T]" };
            Line::from(vec![
                Span::styled("  ✓ ", Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD)),
                Span::styled(format!("'{name}' diterima  "), Style::default().fg(DIM)),
                Span::styled(hint, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            ])
        }
        FileTransferUiState::Sending { name, total, sent } => {
            let pct = if *total > 0 { sent * 100 / total } else { 0 };
            let spinner = SPINNER[(app.tick_count % SPINNER_LEN) as usize];
            Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(WARNING)),
                Span::styled(format!("Mengirim {name}  "), Style::default().fg(DIM)),
                Span::styled(format!("{pct}%"), Style::default().fg(WARNING).add_modifier(Modifier::BOLD)),
            ])
        }
        FileTransferUiState::Receiving { name, total, received } => {
            let pct = if *total > 0 { received * 100 / total } else { 0 };
            let spinner = SPINNER[(app.tick_count % SPINNER_LEN) as usize];
            Line::from(vec![
                Span::styled(format!("  {spinner} "), Style::default().fg(ACCENT)),
                Span::styled(format!("Menerima {name}  "), Style::default().fg(DIM)),
                Span::styled(format!("{pct}%"), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            ])
        }
    };
    f.render_widget(Paragraph::new(ft_line), rows[2]);

    // Input separator (rows[3])
    render_separator(f, rows[3]);

    // Input line (rows[4]) — visible saat room Open, disembunyikan saat SendFile modal aktif
    let input_line = if app.room == RoomState::Open && app.mode != Mode::SendFile {
        let prompt = if app.replying_to.is_some() { "↩ " } else { "› " };
        Line::from(vec![
            Span::styled(prompt, Style::default().fg(ACCENT)),
            Span::styled(app.input.clone(), Style::default().fg(TEXT)),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ])
    } else if app.mode == Mode::SendFile {
        Line::from("") // input tertutup modal kirim file
    } else {
        Line::from(Span::styled(
            "  [Esc] keluar sesi",
            Style::default().fg(DIM),
        ))
    };
    f.render_widget(Paragraph::new(input_line), rows[4]);
}

pub(super) fn render_chat_line(line: &super::super::types::ChatLine) -> Vec<Line<'_>> {
    // Cek apakah pesan adalah reply (format: `↩ "quote"\nactual`)
    if let Some((quote, actual)) = parse_reply_text(&line.text) {
        let (arrow, arrow_style, text_style) = match line.who {
            Who::Me   => ("  → ", Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD), Style::default().fg(SUCCESS)),
            Who::Peer => ("  ← ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),  Style::default()),
            Who::System => ("  ·  ", Style::default().fg(DIM), Style::default().fg(DIM)),
        };
        return vec![
            // Baris 1: kutipan (dim, italic)
            Line::from(vec![
                Span::styled("     ╷ ", Style::default().fg(DIM)),
                Span::styled(
                    format!("{quote}"),
                    Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
                ),
            ]),
            // Baris 2: actual reply
            Line::from(vec![
                Span::styled(arrow, arrow_style),
                Span::styled(actual.to_string(), text_style),
            ]),
        ];
    }

    // Pesan biasa — satu baris
    vec![match line.who {
        Who::Me => Line::from(vec![
            Span::styled("  → ", Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled(line.text.clone(), Style::default().fg(SUCCESS)),
        ]),
        Who::Peer => Line::from(vec![
            Span::styled("  ← ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw(line.text.clone()),
        ]),
        Who::System => Line::from(Span::styled(
            format!("  ·  {}", line.text),
            Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
        )),
    }]
}
