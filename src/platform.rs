//! Platform-specific utilities.
//!
//! SEC-09: set nama proses visible di OS (mengurangi fingerprinting).
//! SEC-04: mlock secrets ke RAM agar tidak swap ke disk.
//!
//! Catatan per platform:
//! - Linux   : prctl(PR_SET_NAME) + mlock(2) + madvise(MADV_DONTDUMP=16).
//!             Kernel limit PR_SET_NAME = 15 karakter.
//! - macOS   : pthread_setname_np + mlock(2).
//! - Windows : SetConsoleTitleW + VirtualLock (butuh hak istimewa; gagal secara
//!             diam-diam bila tidak tersedia — graceful fallback).
//!
//! No-op di platform lain.

/// Set nama proses sesuai platform. Dipanggil sekali di awal `main()` sebelum
/// tokio runtime dibuat.
pub fn set_process_name(name: &str) {
    set_impl(name);
}

/// Kunci halaman memori ke RAM (mencegah swap ke disk).
///
/// SEC-04: dipanggil setelah secret diletakkan di heap (mis. `noise_sk`,
/// `tor_client_auth_secret`). Kegagalan ditangani secara diam-diam — lebih
/// baik berjalan tanpa mlock daripada crash.
///
/// Pada Linux, juga memanggil `madvise(MADV_DONTDUMP)` agar halaman tidak
/// termasuk dalam core dump.
pub fn try_mlock(ptr: *mut u8, len: usize) {
    mlock_impl(ptr, len);
}

// ─── Linux ────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn set_impl(name: &str) {
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

#[cfg(target_os = "linux")]
fn mlock_impl(ptr: *mut u8, len: usize) {
    unsafe {
        let _ = libc::mlock(ptr as *const _, len);
        let _ = libc::madvise(ptr as *mut _, len, libc::MADV_DONTDUMP);
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

#[cfg(target_os = "macos")]
fn mlock_impl(ptr: *mut u8, len: usize) {
    unsafe {
        let _ = libc::mlock(ptr as *const _, len);
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
fn mlock_impl(ptr: *mut u8, len: usize) {
    // VirtualLock butuh SE_LOCK_MEMORY_NAME privilege; gagal secara diam-diam.
    unsafe { let _ = VirtualLock(ptr as *mut _, len); }
}

#[cfg(windows)]
extern "system" {
    fn SetConsoleTitleW(lpConsoleTitle: *const u16) -> i32;
    fn VirtualLock(lpAddress: *mut core::ffi::c_void, dwSize: usize) -> i32;
}

// ─── Fallback ────────────────────────────────────────────────────────────────

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn set_impl(_name: &str) {}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn mlock_impl(_ptr: *mut u8, _len: usize) {}
