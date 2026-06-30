//! Rendering: Password Manager screens (PmMain, PmCodesModal, PmAdd).

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::identity::pm::PM_CODES_MAX;

use super::theme::{ACCENT, DIM, ERROR, SUCCESS, TEXT, WARNING};
use super::helpers::{centered_rect_abs, d, k, render_footer, truncate_str};
use super::super::app::App;
use super::super::types::NotifLevel;
use super::super::pm::pm_visible_entries;

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
