//! SEC-09: Platform-specific utilities — set nama proses visible di OS.
//!
//! Mengurangi fingerprinting dari `ps aux` / Task Manager dengan mengganti
//! nama proses ke nama generik via flag `--process-name`.
//!
//! Catatan per platform:
//! - Linux   : `prctl(PR_SET_NAME)` — visible di `ps aux` dan `/proc/self/comm`.
//!             Kernel membatasi 15 karakter; nama lebih panjang dipotong.
//! - macOS   : `pthread_setname_np` — visible di Activity Monitor.
//! - Windows : `SetConsoleTitleW` mengubah judul jendela konsol.
//!             Process image name di Task Manager ditentukan dari nama file binary.
//!             Untuk menyembunyikan image name: install binary dengan nama berbeda
//!             (mis. `update-agent.exe`) — ini adalah mitigasi SEC-09 yang sesungguhnya.
//!
//! No-op di platform lain (tidak ada syscall yang dipanggil).

/// Set nama proses sesuai platform. Dipanggil sekali di awal `main()` sebelum
/// tokio runtime dibuat.
pub fn set_process_name(name: &str) {
    set_impl(name);
}

// ─── Linux ────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn set_impl(name: &str) {
    // Kernel limit PR_SET_NAME = 15 karakter + null byte (16 byte total).
    let n = name.len().min(15);
    let truncated = &name[..n];
    if let Ok(cname) = std::ffi::CString::new(truncated) {
        unsafe {
            libc::prctl(
                libc::PR_SET_NAME,
                cname.as_ptr() as libc::c_ulong,
                0usize,
                0usize,
                0usize,
            );
        }
    }
}

// ─── macOS ───────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn set_impl(name: &str) {
    let n = name.len().min(63);
    let truncated = &name[..n];
    if let Ok(cname) = std::ffi::CString::new(truncated) {
        unsafe {
            libc::pthread_setname_np(cname.as_ptr());
        }
    }
}

// ─── Windows ─────────────────────────────────────────────────────────────────

#[cfg(windows)]
fn set_impl(name: &str) {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = std::ffi::OsStr::new(name)
        .encode_wide()
        .chain(std::iter::once(0u16))
        .collect();
    unsafe { SetConsoleTitleW(wide.as_ptr()); }
}

#[cfg(windows)]
extern "system" {
    fn SetConsoleTitleW(lpConsoleTitle: *const u16) -> i32;
}

// ─── Fallback ────────────────────────────────────────────────────────────────

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn set_impl(_name: &str) {}
