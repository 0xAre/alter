//! FT-01: render gambar inline di terminal via viuer.
//!
//! Dipanggil SETELAH terminal.draw() — viuer menulis escape code langsung ke
//! terminal, bukan via ratatui widget system. Kitty → Sixel → half-block fallback.

use std::io::Write;

/// Render bytes gambar ke terminal di posisi kursor saat ini.
/// `max_width`: lebar terminal dalam kolom karakter.
/// Dipanggil dari event loop utama (mod.rs) setelah LeaveAlternateScreen.
pub fn render_image_inline(data: &[u8], max_width: u16) {
    let Ok(mut tmp) = tempfile::NamedTempFile::new() else { return };
    if tmp.write_all(data).is_err() { return }
    let cfg = viuer::Config {
        width: Some(max_width as u32),
        absolute_offset: false,
        ..Default::default()
    };
    let _ = viuer::print_from_file(tmp.path(), &cfg);
}
