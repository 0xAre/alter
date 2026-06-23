//! Rendering layer — murni fungsi dari `&App` ke widget.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use super::{App, Mode, NotifLevel, RoomState, Screen, Who};

// ─── ASCII logo ───────────────────────────────────────────────────────────────
const LOGO: &[&str] = &[
    "   █████╗ ██╗  ████████╗███████╗██████╗ ",
    "  ██╔══██╗██║  ╚══██╔══╝██╔════╝██╔══██╗",
    "  ███████║██║     ██║   █████╗  ██████╔╝ ",
    "  ██╔══██║██║     ██║   ██╔══╝  ██╔══██╗ ",
    "  ██║  ██║███████╗██║   ███████╗██║  ██║ ",
    "  ╚═╝  ╚═╝╚══════╝╚═╝   ╚══════╝╚═╝  ╚═╝",
];

// Versi ringkas 3 baris untuk header utama
const LOGO_SMALL: &[&str] = &[
    "▄▀█ █   ▀█▀ ██▀ █▀█",
    "█▀█ █    █  █▄▄ █▀▄",
    "▀ ▀ ▀▀▀  ▀  ▀▀▀ ▀ ▀",
];

// ─── Color palette ────────────────────────────────────────────────────────────
const ACCENT: Color = Color::LightCyan;
const DIM: Color = Color::DarkGray;
const TEXT: Color = Color::White;
const SUCCESS: Color = Color::LightGreen;
const WARNING: Color = Color::Yellow;
const ERROR: Color = Color::LightRed;

// Spinner frames (Braille) — index via tick_count % SPINNER_LEN
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_LEN: u64 = 10;

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;

pub(super) fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(f, area);
        return;
    }
    match app.screen {
        Screen::Splash => render_splash(f, app),
        Screen::Unlock | Screen::Create => render_auth(f, app),
        Screen::Init => render_init(f, app),
        Screen::Main => render_main(f, app),
    }
}

fn render_too_small(f: &mut Frame, area: Rect) {
    f.render_widget(Clear, area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("ALTER", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled(
            format!("Terminal terlalu kecil. Minimal {}×{} karakter.", MIN_WIDTH, MIN_HEIGHT),
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            format!("Saat ini: {}×{}", area.width, area.height),
            Style::default().fg(DIM),
        )),
    ];
    let card = centered_rect_abs(52, 7, area);
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), card);
}

// ─────────────────────────────── Splash ───────────────────────────────────────

fn render_logo_lines() -> Vec<Line<'static>> {
    LOGO.iter()
        .map(|row| {
            Line::from(Span::styled(
                *row,
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ))
        })
        .collect()
}

fn render_splash(f: &mut Frame, _app: &App) {
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

// ─────────────────────────────── Auth ─────────────────────────────────────────

fn render_auth(f: &mut Frame, app: &App) {
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
        let pass_active = !app.create_confirming;
        lines.push(field_line("Passphrase  ", app.pass_input.len(), pass_active));
        lines.push(field_line(
            "Konfirmasi  ",
            app.pass_confirm.len(),
            app.create_confirming,
        ));
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
            "Passphrase mengenkripsi identitasmu secara lokal.",
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

fn field_line(label: &str, len: usize, active: bool) -> Line<'static> {
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

// ─────────────────────────────── Init checklist ───────────────────────────────

fn render_init(f: &mut Frame, app: &App) {
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

// ─────────────────────────────── Main screen ──────────────────────────────────

fn render_main(f: &mut Frame, app: &App) {
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
    if app.show_invite {
        render_invite_popup(f, app, area);
    }
    if app.pending_delete.is_some() {
        render_delete_confirm(f, app, area);
    }
}

fn render_separator(f: &mut Frame, area: Rect) {
    let line = "─".repeat(area.width as usize);
    f.render_widget(
        Paragraph::new(Span::styled(line, Style::default().fg(DIM))),
        area,
    );
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
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
    let transport_pill: Span = if app.tor_active() {
        pill(" ⬡ TOR ".to_string(), ACCENT)
    } else if app.tor_connecting {
        let spinner = SPINNER[(app.tick_count % SPINNER_LEN) as usize];
        pill(format!(" ⬡ TOR {spinner} "), WARNING)
    } else {
        pill(" LAN ".to_string(), Color::Gray)
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

    let left = Line::from(vec![
        Span::raw(" "),
        transport_pill,
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

// ─────────────────────────────── Contacts ─────────────────────────────────────

fn render_contacts(f: &mut Frame, app: &App, area: Rect) {
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

// ─────────────────────────────── Right panel ──────────────────────────────────

fn render_right_panel(f: &mut Frame, app: &App, area: Rect) {
    match app.mode {
        Mode::Browsing | Mode::AddContact => render_idle_panel(f, app, area),
        Mode::InRoom => render_chat_panel(f, app, area),
    }
}

fn render_idle_panel(f: &mut Frame, app: &App, area: Rect) {
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
                "  [Enter] buka sesi   [d] hapus",
                Style::default().fg(DIM),
            )),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}

fn render_chat_panel(f: &mut Frame, app: &App, area: Rect) {
    // Split: title | messages | separator | input
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title row (selaras dengan "CONTACTS")
            Constraint::Min(1),    // chat messages
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

    // Chat messages
    let inner_h = rows[1].height as usize;
    let start = app.messages.len().saturating_sub(inner_h.max(1));
    let mut lines: Vec<Line> = Vec::new();

    // Messages
    for msg in &app.messages[start..] {
        lines.push(render_chat_line(msg));
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

    // Input separator
    render_separator(f, rows[2]);

    // Input line — hanya visible saat room Open
    let input_line = if app.room == RoomState::Open {
        Line::from(vec![
            Span::styled("› ", Style::default().fg(ACCENT)),
            Span::styled(app.input.clone(), Style::default().fg(TEXT)),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ])
    } else {
        Line::from(Span::styled(
            "  [Esc] keluar sesi",
            Style::default().fg(DIM),
        ))
    };
    f.render_widget(Paragraph::new(input_line), rows[3]);
}

fn render_chat_line(line: &super::ChatLine) -> Line<'_> {
    match line.who {
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
    }
}

// ─────────────────────────────── Add Contact Modal ────────────────────────────

fn render_add_contact_modal(f: &mut Frame, app: &App, area: Rect) {
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

// ─────────────────────────────── Delete confirm ───────────────────────────────

fn render_delete_confirm(f: &mut Frame, app: &App, area: Rect) {
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

// ─────────────────────────────── Footer / hints ───────────────────────────────

fn k(s: &'static str) -> Span<'static> {
    Span::styled(s, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
}

fn d(s: &'static str) -> Span<'static> {
    Span::styled(s, Style::default().fg(DIM))
}

fn footer_hint(app: &App) -> Line<'static> {
    match app.mode {
        Mode::Browsing => Line::from(vec![
            d(" "),
            k("[↑↓]"), d(" pilih   "),
            k("[Enter]"), d(" sesi   "),
            k("[a]"), d(" tambah   "),
            k("[d]"), d(" hapus   "),
            k("[i]"), d(" identitas   "),
            k("[q]"), d(" keluar"),
        ]),
        Mode::AddContact => Line::from(vec![
            d(" "),
            k("[Enter]"), d(" simpan   "),
            k("[Esc]"), d(" batal"),
        ]),
        Mode::InRoom => Line::from(vec![
            d(" "),
            k("[Enter]"), d(" kirim   "),
            k("[Esc]"), d(" keluar sesi"),
        ]),
    }
}

fn render_footer(f: &mut Frame, area: Rect, hint: Line<'_>) {
    let footer_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    f.render_widget(Paragraph::new(hint), footer_area);
}

// ─────────────────────────────── Invite popup ─────────────────────────────────

fn render_invite_popup(f: &mut Frame, app: &App, area: Rect) {
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

// ─────────────────────────────── Helpers ──────────────────────────────────────

fn centered_rect_abs(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn centered_rect_pct(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = area.width * percent_x / 100;
    let h = area.height * percent_y / 100;
    centered_rect_abs(w, h, area)
}

fn truncate_nick(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

fn format_fingerprint(fp: &str) -> String {
    fp.as_bytes()
        .chunks(8)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn format_fingerprint_short(fp: &str) -> String {
    fp.get(..6).unwrap_or(fp).to_string()
}
