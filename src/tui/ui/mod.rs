//! Rendering layer — murni fungsi dari `&App` ke widget.

mod auth;
mod helpers;
mod main;
mod migrate;
mod modals;
mod pm;
mod theme;

use ratatui::Frame;
use ratatui::widgets::Clear;
use ratatui::text::{Line, Span};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;
use ratatui::layout::Alignment;

use super::app::App;
use super::types::Screen;

use theme::{ACCENT, DIM, MIN_HEIGHT, MIN_WIDTH};
use helpers::centered_rect_abs;

pub(super) fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(f, area);
        return;
    }
    match app.screen {
        Screen::Splash => auth::render_splash(f, app),
        Screen::Unlock | Screen::Create => auth::render_auth(f, app),
        Screen::Unlocking => auth::render_unlocking(f, app),
        Screen::Init => auth::render_init(f, app),
        Screen::Onboard => auth::render_onboard(f, app),
        Screen::Main => main::render_main(f, app),
        Screen::PmMain => pm::render_pm_main(f, app),
        Screen::PmAdd => pm::render_pm_add(f, app),
        Screen::Migrate => migrate::render_migrate(f, app),
    }
}

fn render_too_small(f: &mut Frame, area: ratatui::layout::Rect) {
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
