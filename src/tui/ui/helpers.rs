//! Helpers UI: layout utilities, formatting, footer.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::theme::{ACCENT, DIM};
use super::super::app::App;
use super::super::types::Mode;

pub(super) fn centered_rect_abs(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

pub(super) fn centered_rect_pct(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = area.width * percent_x / 100;
    let h = area.height * percent_y / 100;
    centered_rect_abs(w, h, area)
}

pub(super) fn truncate_nick(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

pub(super) fn format_fingerprint(fp: &str) -> String {
    fp.as_bytes()
        .chunks(8)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(super) fn format_fingerprint_short(fp: &str) -> String {
    fp.get(..6).unwrap_or(fp).to_string()
}

pub(super) fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

pub(super) fn k(s: &'static str) -> Span<'static> {
    Span::styled(s, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
}

pub(super) fn d(s: &'static str) -> Span<'static> {
    Span::styled(s, Style::default().fg(DIM))
}

pub(super) fn footer_hint(app: &App) -> Line<'static> {
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
        // FT-01 Fase 2: hint path-input + Esc batal akan dipasang di sini.
        Mode::SendFile => Line::from(vec![
            d(" "),
            k("[Enter]"), d(" kirim file   "),
            k("[Esc]"), d(" batal"),
        ]),
    }
}

pub(super) fn render_footer(f: &mut Frame, area: Rect, hint: Line<'_>) {
    let footer_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    f.render_widget(Paragraph::new(hint), footer_area);
}
