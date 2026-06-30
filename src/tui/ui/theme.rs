//! Konstanta visual: logo, warna, spinner.

use ratatui::style::Color;

// ─── ASCII logo ───────────────────────────────────────────────────────────────
pub(super) const LOGO: &[&str] = &[
    "   █████╗ ██╗  ████████╗███████╗██████╗ ",
    "  ██╔══██╗██║  ╚══██╔══╝██╔════╝██╔══██╗",
    "  ███████║██║     ██║   █████╗  ██████╔╝ ",
    "  ██╔══██║██║     ██║   ██╔══╝  ██╔══██╗ ",
    "  ██║  ██║███████╗██║   ███████╗██║  ██║ ",
    "  ╚═╝  ╚═╝╚══════╝╚═╝   ╚══════╝╚═╝  ╚═╝",
];

// Versi ringkas 3 baris untuk header utama
pub(super) const LOGO_SMALL: &[&str] = &[
    "▄▀█ █   ▀█▀ ██▀ █▀█",
    "█▀█ █    █  █▄▄ █▀▄",
    "▀ ▀ ▀▀▀  ▀  ▀▀▀ ▀ ▀",
];

// ─── Color palette ────────────────────────────────────────────────────────────
pub(super) const ACCENT: Color = Color::LightCyan;
pub(super) const DIM: Color = Color::DarkGray;
pub(super) const TEXT: Color = Color::White;
pub(super) const SUCCESS: Color = Color::LightGreen;
pub(super) const WARNING: Color = Color::Yellow;
pub(super) const ERROR: Color = Color::LightRed;

// Spinner frames (Braille) — index via tick_count % SPINNER_LEN
pub(super) const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub(super) const SPINNER_LEN: u64 = 10;

pub(super) const MIN_WIDTH: u16 = 80;
pub(super) const MIN_HEIGHT: u16 = 24;
