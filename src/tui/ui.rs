//! Rendering layer — murni fungsi dari `&App` ke widget.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::identity::pm::PM_CODES_MAX;
use super::{App, CreatePhase, Mode, NotifLevel, RoomState, Screen, Who, pm_visible_entries};


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
        Screen::Unlocking => render_unlocking(f, app),
        Screen::Init => render_init(f, app),
        Screen::Onboard => render_onboard(f, app),
        Screen::Main => render_main(f, app),
        Screen::PmMain => render_pm_main(f, app),
        Screen::PmAdd => render_pm_add(f, app),
        Screen::Migrate => render_migrate(f, app),
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

// ─────────────────────────────── Unlocking spinner ────────────────────────────

fn render_unlocking(f: &mut Frame, app: &App) {
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

// ─────────────────────────────── Onboarding (R-07) ───────────────────────────

fn render_onboard(f: &mut Frame, _app: &App) {
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
    if app.mode == Mode::RenameContact {
        render_rename_contact_modal(f, app, area);
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
        Mode::Browsing | Mode::AddContact | Mode::RenameContact => render_idle_panel(f, app, area),
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
                "  [Enter] buka sesi   [r] ubah nama   [d] hapus",
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

// ─────────────────────────────── Rename Contact Modal ────────────────────────

fn render_rename_contact_modal(f: &mut Frame, app: &App, area: Rect) {
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
            k("[r]"), d(" ubah nama   "),
            k("[d]"), d(" hapus   "),
            k("[i]"), d(" identitas   "),
            k("[q]"), d(" keluar"),
        ]),
        Mode::AddContact | Mode::RenameContact => Line::from(vec![
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

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

// ─── PM Main Screen ───────────────────────────────────────────────────────────

pub(super) fn render_pm_main(f: &mut Frame, app: &App) {
    let area = f.area();
    let visible: Vec<usize> = pm_visible_entries(app);

    // Layout: header(3) | search(3) | content(min) | footer(1)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(area);

    // ── Header ──────────────────────────────────────────────────────────────
    let total = app.pm_entries.len();
    let shown = visible.len();
    let count_label = if app.pm_search.is_empty() {
        format!(" {} entri ", total)
    } else {
        format!(" {}/{} entri ", shown, total)
    };
    let readonly_badge = if app.pm_is_readonly {
        Span::styled(" READ-ONLY ", Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" ● aktif ", Style::default().fg(SUCCESS))
    };
    let header = Paragraph::new(Line::from(vec![
        Span::styled("  Password Manager", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(count_label, Style::default().fg(DIM)),
        readonly_badge,
    ])).block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(DIM)));
    f.render_widget(header, outer[0]);

    // ── Search bar ──────────────────────────────────────────────────────────
    let search_active = app.pm_search_active;
    let search_content = if app.pm_search.is_empty() && !search_active {
        Line::from(Span::styled("  [/] cari service, username…", Style::default().fg(DIM)))
    } else {
        Line::from(vec![
            Span::styled("  / ", Style::default().fg(ACCENT)),
            Span::styled(app.pm_search.clone(), Style::default().fg(TEXT)),
            Span::styled(if search_active { "▏" } else { "" }, Style::default().fg(ACCENT)),
        ])
    };
    let search_bar = Paragraph::new(search_content)
        .block(Block::default().borders(Borders::ALL).border_style(
            if search_active { Style::default().fg(ACCENT) } else { Style::default().fg(DIM) }
        ));
    f.render_widget(search_bar, outer[1]);

    // ── Content: entry list (left 60%) | detail panel (right 40%) ──────────
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(outer[2]);

    // Column widths based on left pane
    let pane_w = panes[0].width.saturating_sub(6) as usize;
    let svc_w  = (pane_w * 38 / 100).max(12);
    let usr_w  = (pane_w * 34 / 100).max(10);

    // Column header row
    let col_sep = Span::styled("─".repeat(panes[0].width.saturating_sub(2) as usize), Style::default().fg(DIM));
    let col_hdr = Line::from(vec![
        Span::styled(format!("   {:<3}", "#"), Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:<w$}", "SERVICE", w = svc_w), Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {:<w$}", "USERNAME", w = usr_w), Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
        Span::styled("  PASSWORD", Style::default().fg(DIM).add_modifier(Modifier::BOLD)),
    ]);

    let mut list_lines: Vec<Line> = vec![col_hdr, Line::from(col_sep)];

    if visible.is_empty() {
        list_lines.push(Line::from(""));
        list_lines.push(Line::from(Span::styled(
            if app.pm_search.is_empty() {
                "   Vault kosong — tekan [n] untuk tambah entri"
            } else {
                "   Tidak ada entri yang cocok"
            },
            Style::default().fg(DIM),
        )));
    } else {
        for (list_idx, &entry_idx) in visible.iter().enumerate() {
            let e       = &app.pm_entries[entry_idx];
            let sel     = list_idx == app.pm_selected;
            let pending = app.pm_pending_delete == Some(entry_idx);
            let revealed = app.pm_reveal_tick.is_some() && sel;

            let svc = truncate_str(&e.service, svc_w);
            let usr = truncate_str(&e.username, usr_w);
            let pass = if revealed {
                e.password.clone()
            } else {
                "•".repeat(e.password.len().min(16))
            };
            let marker = if sel { "▶" } else { " " };
            let num_str = format!("{:>3}", list_idx + 1);

            list_lines.push(if pending {
                Line::from(vec![
                    Span::styled(format!(" {} {} ", marker, num_str), Style::default().fg(ERROR)),
                    Span::styled(format!("{:<w$}", svc, w = svc_w), Style::default().fg(ERROR)),
                    Span::styled("  Hapus entri ini? ", Style::default().fg(DIM)),
                    Span::styled("[y]", Style::default().fg(WARNING)),
                    Span::styled(" ya  ", Style::default().fg(DIM)),
                    Span::styled("[Esc]", Style::default().fg(DIM)),
                    Span::styled(" batal", Style::default().fg(DIM)),
                ])
            } else {
                let row_sty = if sel {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                let dim_sty = if sel { Style::default().fg(TEXT) } else { Style::default().fg(DIM) };
                let pass_sty = if revealed {
                    Style::default().fg(WARNING).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(DIM)
                };
                Line::from(vec![
                    Span::styled(format!(" {} {} ", marker, num_str), row_sty),
                    Span::styled(format!("{:<w$}", svc, w = svc_w), row_sty),
                    Span::styled(format!("  {:<w$}", usr, w = usr_w), dim_sty),
                    Span::styled(format!("  {}", pass), pass_sty),
                ])
            });
        }
    }

    f.render_widget(
        Paragraph::new(list_lines)
            .block(Block::default().borders(Borders::ALL)
                .border_style(Style::default().fg(DIM))
                .title(Span::styled(
                    format!(" Vault ({}) ", shown),
                    Style::default().fg(ACCENT),
                ))),
        panes[0],
    );

    // ── Detail panel ────────────────────────────────────────────────────────
    let detail_lines: Vec<Line> = if visible.is_empty() {
        vec![
            Line::from(""),
            Line::from(Span::styled("  Tidak ada entri.", Style::default().fg(DIM))),
            Line::from(""),
            Line::from(Span::styled(
                if app.pm_is_readonly { "  Mode read-only." } else { "  Tekan [n] untuk tambah." },
                Style::default().fg(DIM),
            )),
        ]
    } else {
        let e       = &app.pm_entries[visible[app.pm_selected]];
        let revealed = app.pm_reveal_tick.is_some();
        let pass_display = if revealed {
            e.password.clone()
        } else {
            "•".repeat(e.password.len().min(24))
        };
        let sep = Span::styled("─".repeat(panes[1].width.saturating_sub(4) as usize), Style::default().fg(DIM));
        {
            let codes_line = match &e.codes {
                None => Line::from(vec![
                    Span::styled("  [k] ", Style::default().fg(DIM)),
                    Span::styled("tambah backup codes", Style::default().fg(DIM)),
                ]),
                Some(codes) => {
                    let total = codes.len();
                    let unused = codes.iter().filter(|c| !c.used).count();
                    let color = if unused == 0 { ERROR } else if unused <= 2 { WARNING } else { SUCCESS };
                    Line::from(vec![
                        Span::styled("  Backup Codes  ", Style::default().fg(DIM)),
                        Span::styled(
                            format!("{}/{} tersisa", unused, total),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                    ])
                }
            };
            vec![
                Line::from(""),
                Line::from(Span::styled("  Service", Style::default().fg(DIM))),
                Line::from(Span::styled(format!("  {}", e.service), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from(Span::styled("  Username", Style::default().fg(DIM))),
                Line::from(Span::styled(format!("  {}", e.username), Style::default().fg(TEXT))),
                Line::from(""),
                Line::from(Span::styled("  Password", Style::default().fg(DIM))),
                Line::from(Span::styled(
                    format!("  {}", pass_display),
                    if revealed {
                        Style::default().fg(WARNING).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(TEXT)
                    },
                )),
                Line::from(""),
                codes_line,
                Line::from(""),
                Line::from(sep),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  [Enter] ", Style::default().fg(ACCENT)),
                    Span::styled(
                        if revealed { "sembunyikan" } else { "reveal password" },
                        Style::default().fg(DIM),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  [c] ", Style::default().fg(ACCENT)),
                    Span::styled("copy password  ", Style::default().fg(DIM)),
                    Span::styled("[u] ", Style::default().fg(ACCENT)),
                    Span::styled("copy user", Style::default().fg(DIM)),
                ]),
                Line::from(vec![
                    Span::styled("  [k] ", Style::default().fg(ACCENT)),
                    Span::styled("kelola backup codes", Style::default().fg(DIM)),
                ]),
            ]
        }
    };

    f.render_widget(
        Paragraph::new(detail_lines)
            .block(Block::default().borders(Borders::ALL)
                .border_style(Style::default().fg(DIM))
                .title(Span::styled(" Detail ", Style::default().fg(DIM)))),
        panes[1],
    );

    // ── Codes modal overlay ─────────────────────────────────────────────────
    if app.pm_codes_open {
        render_pm_codes_modal(f, app, area);
    }

    // ── Footer — notifikasi menggantikan hints saat aktif ───────────────────
    if let Some(notif) = &app.notification {
        let color = match notif.level {
            NotifLevel::Error   => ERROR,
            NotifLevel::Warn    => WARNING,
            NotifLevel::Success => SUCCESS,
            NotifLevel::Info    => DIM,
        };
        render_footer(f, area, Line::from(Span::styled(
            format!("  {}", notif.text),
            Style::default().fg(color),
        )));
    } else {
        let hints: Vec<Span> = if app.pm_is_readonly {
            vec![
                d(" "), k("[↑↓]"), d(" pilih  "),
                k("[Enter]"), d(" reveal  "),
                k("[c]"), d(" copy pass  "),
                k("[u]"), d(" copy user  "),
                k("[k]"), d(" backup codes  "),
                k("[/]"), d(" cari  "),
                k("[q]"), d(" keluar"),
            ]
        } else {
            vec![
                d(" "), k("[↑↓]"), d(" pilih  "),
                k("[n]"), d(" tambah  "),
                k("[d]"), d(" hapus  "),
                k("[Enter]"), d(" reveal  "),
                k("[c]"), d(" copy pass  "),
                k("[k]"), d(" backup codes  "),
                k("[/]"), d(" cari  "),
                k("[q]"), d(" keluar"),
            ]
        };
        render_footer(f, area, Line::from(hints));
    }
}

// ─── PM Codes Modal ───────────────────────────────────────────────────────────

fn render_pm_codes_modal(f: &mut Frame, app: &App, area: Rect) {
    let visible = pm_visible_entries(app);
    if visible.is_empty() { return; }
    let e = &app.pm_entries[visible[app.pm_selected]];

    let modal = centered_rect_abs(60, 22, area);
    f.render_widget(Clear, modal);

    let codes = e.codes.as_deref().unwrap_or(&[]);
    let total = codes.len();
    let unused = codes.iter().filter(|c| !c.used).count();

    let title_color = if unused == 0 { ERROR } else if unused <= 2 { WARNING } else { ACCENT };

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Service  ", Style::default().fg(DIM)),
            Span::styled(e.service.clone(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Tersisa  ", Style::default().fg(DIM)),
            Span::styled(
                format!("{}/{} codes", unused, total),
                Style::default().fg(title_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(Span::styled(
            "  ─".repeat((modal.width.saturating_sub(4) / 2) as usize),
            Style::default().fg(DIM),
        )),
    ];

    if codes.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Belum ada backup codes — tekan [n] untuk tambah",
            Style::default().fg(DIM),
        )));
    } else {
        for (i, bc) in codes.iter().enumerate() {
            let sel = i == app.pm_codes_selected;
            let marker = if sel { "▶" } else { " " };
            let (code_style, status) = if bc.used {
                (Style::default().fg(DIM).add_modifier(Modifier::CROSSED_OUT), " [used]")
            } else if sel {
                (Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD), "")
            } else {
                (Style::default().fg(TEXT), "")
            };
            lines.push(Line::from(vec![
                Span::styled(format!(" {} {:>2}  ", marker, i + 1), Style::default().fg(DIM)),
                Span::styled(bc.code.clone(), code_style),
                Span::styled(status, Style::default().fg(DIM)),
            ]));
        }
    }

    // Input tambah code baru
    if app.pm_codes_add_mode {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  + ", Style::default().fg(ACCENT)),
            Span::styled(app.pm_codes_input.clone(), Style::default().fg(TEXT)),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ]));
    }

    // Padding
    while lines.len() < 17 {
        lines.push(Line::from(""));
    }

    // Hints
    let hint = if app.pm_codes_add_mode {
        Line::from(vec![
            d(" "), k("[Enter]"), d(" tambah  "), k("[Esc]"), d(" batal"),
        ])
    } else if app.pm_is_readonly {
        Line::from(vec![
            d(" "), k("[↑↓]"), d(" pilih  "), k("[y]"), d(" copy  "), k("[Esc]"), d(" tutup"),
        ])
    } else {
        Line::from(vec![
            d(" "), k("[↑↓]"), d(" pilih  "),
            k("[n]"), d(" tambah  "),
            k("[m]"), d(" mark used  "),
            k("[y]"), d(" copy  "),
            k("[d]"), d(" hapus  "),
            k("[Esc]"), d(" tutup"),
        ])
    };
    lines.push(hint);

    let title = format!(
        " Backup Codes ({}/{}) ",
        unused,
        PM_CODES_MAX
    );
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(title_color))
                .title(Span::styled(title, Style::default().fg(title_color).add_modifier(Modifier::BOLD))),
        ),
        modal,
    );
}

// ─── PM Add Screen ────────────────────────────────────────────────────────────

pub(super) fn render_pm_add(f: &mut Frame, app: &App) {
    let area = f.area();
    let card = centered_rect_abs(64, 24, area);
    f.render_widget(Clear, card);

    let active = app.pm_add_field as usize;

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            "  Tambah Entri Baru  ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = outer_block.inner(card);
    f.render_widget(outer_block, card);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // step indicator
            Constraint::Length(1), // spacer
            Constraint::Length(3), // field 1 / codes area header
            Constraint::Length(3), // field 2 / codes list line 1-2
            Constraint::Length(3), // field 3 / codes list line 3-4
            Constraint::Length(3), // (unused for steps 0-2) / codes input
            Constraint::Length(1), // error / hint
        ])
        .split(inner);

    // Step indicator
    let dot = |i: usize| {
        if i < active { Span::styled("● ", Style::default().fg(SUCCESS)) }
        else if i == active { Span::styled("● ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)) }
        else { Span::styled("○ ", Style::default().fg(DIM)) }
    };
    let label_sty = |i: usize| {
        if i == active { Style::default().fg(ACCENT) }
        else if i < active { Style::default().fg(SUCCESS) }
        else { Style::default().fg(DIM) }
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            dot(0), Span::styled("Service  ", label_sty(0)),
            dot(1), Span::styled("Username  ", label_sty(1)),
            dot(2), Span::styled("Password  ", label_sty(2)),
            dot(3), Span::styled("Backup Codes", label_sty(3)),
        ])),
        chunks[0],
    );

    if active <= 2 {
        // ── Steps 0-2: tampilkan 3 field ────────────────────────────────
        let defs: [(&str, &str, bool); 3] = [
            ("Service / URL",    app.pm_add_service.as_str(),  false),
            ("Username / Email", app.pm_add_username.as_str(), false),
            ("Password",         app.pm_add_password.as_str(), true),
        ];
        for (i, (label, val, is_pass)) in defs.iter().enumerate() {
            let is_active = i == active;
            let display: String = if *is_pass && !is_active {
                "•".repeat(val.len())
            } else {
                val.to_string()
            };
            let cursor = if is_active { "▏" } else { "" };
            let border_sty = if is_active { Style::default().fg(ACCENT) }
                else if val.is_empty() { Style::default().fg(DIM) }
                else { Style::default().fg(SUCCESS) };
            let title_sty = if is_active { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) }
                else if val.is_empty() { Style::default().fg(DIM) }
                else { Style::default().fg(SUCCESS) };
            let check = if !val.is_empty() && !is_active { " ✓" } else { "" };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{}{}", display, cursor),
                    Style::default().fg(TEXT),
                ))).block(Block::default().borders(Borders::ALL)
                    .border_style(border_sty)
                    .title(Span::styled(format!(" {}{} ", label, check), title_sty))),
                chunks[i + 2],
            );
        }

        // Error row
        if let Some(err) = &app.auth_error {
            f.render_widget(
                Paragraph::new(Span::styled(format!("[!] {}", err), Style::default().fg(ERROR))),
                chunks[6],
            );
        }

        render_footer(f, area, Line::from(vec![
            d(" "), k("[Tab/↓]"), d(" berikut  "),
            k("[Enter]"), d(" lanjut / simpan  "),
            k("[Esc]"), d(" batal"),
        ]));
    } else {
        // ── Step 3: input backup codes ───────────────────────────────────
        let codes = &app.pm_add_codes;
        let count = codes.len();

        // Deskripsi step
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Paste backup codes satu per baris. ", Style::default().fg(DIM)),
                Span::styled(
                    format!("{}/{}", count, PM_CODES_MAX),
                    Style::default().fg(if count == 0 { DIM } else { ACCENT }),
                ),
            ])).block(Block::default().borders(Borders::ALL)
                .border_style(Style::default().fg(DIM))
                .title(Span::styled(" Backup Codes (opsional) ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))),
            chunks[2],
        );

        // Tampilkan codes yang sudah dimasukkan (maks 4 baris)
        let display_codes: Vec<Line> = codes.iter().enumerate().take(4).map(|(i, c)| {
            Line::from(vec![
                Span::styled(format!("  {:>2}. ", i + 1), Style::default().fg(DIM)),
                Span::styled(c.clone(), Style::default().fg(SUCCESS)),
            ])
        }).collect();
        let codes_block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(DIM));
        f.render_widget(
            Paragraph::new(if display_codes.is_empty() {
                vec![Line::from(Span::styled("  (belum ada)", Style::default().fg(DIM)))]
            } else {
                display_codes
            }).block(codes_block),
            chunks[3],
        );

        // Padding baris lebih → pakai chunk 4 kosong
        f.render_widget(
            Paragraph::new("").block(Block::default().borders(Borders::NONE)),
            chunks[4],
        );

        // Input field untuk code baru
        let input_done = count >= PM_CODES_MAX;
        let input_text = if input_done {
            Span::styled(format!("Maksimum {} codes tercapai", PM_CODES_MAX), Style::default().fg(WARNING))
        } else {
            Span::styled(
                format!("  {}▏", app.pm_add_code_input),
                Style::default().fg(TEXT),
            )
        };
        f.render_widget(
            Paragraph::new(Line::from(input_text))
                .block(Block::default().borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT))
                    .title(Span::styled(
                        " Ketik code, [Enter] tambah — [Enter] kosong untuk simpan ",
                        Style::default().fg(DIM),
                    ))),
            chunks[5],
        );

        render_footer(f, area, Line::from(vec![
            d(" "), k("[Enter]"), d(" tambah code / simpan  "),
            k("[Esc]"), d(" batal"),
        ]));
    }
}

// ─── Migrate Screen ───────────────────────────────────────────────────────────

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
                Style::default().fg(ACCENT),
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
