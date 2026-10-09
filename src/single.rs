//! Single instance: a named mutex, plus a named event the second instance
//! uses to ask the first one to come to the front. Two instances writing the
//! same database at once would only cause confusion.

use std::ffi::c_void;
use std::ptr;

const ERROR_ALREADY_EXISTS: u32 = 183;
const EVENT_MODIFY_STATE: u32 = 0x0002;
const WAIT_OBJECT_0: u32 = 0;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(attrs: *mut c_void, initial_owner: i32, name: *const u16) -> *mut c_void;
    fn CreateEventW(attrs: *mut c_void, manual_reset: i32, initial_state: i32, name: *const u16) -> *mut c_void;
    fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> *mut c_void;
    fn SetEvent(handle: *mut c_void) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, ms: u32) -> u32;
    fn GetLastError() -> u32;
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const MUTEX_NAME: &str = "Local\\fitledger-single-instance";
const EVENT_NAME: &str = "Local\\fitledger-show";

/// Windows object name. FITLEDGER_INSTANCE (tests only) separates a test
/// instance from the everyday app open in the same session.
fn name(base: &str) -> Vec<u16> {
    match std::env::var("FITLEDGER_INSTANCE") {
        Ok(suffix) if !suffix.is_empty() => wide(&format!("{base}-{suffix}")),
        _ => wide(base),
    }
}

/// True if this is the first instance (the mutex stays alive until the
/// process ends). Otherwise signals the running instance and returns false.
pub fn acquire_or_signal() -> bool {
    let n = name(MUTEX_NAME);
    // SAFETY: `n` is a NUL-terminated UTF-16 string alive during the call;
    // null attributes are allowed. The handle is intentionally never closed:
    // it must live as long as the process.
    let handle = unsafe { CreateMutexW(ptr::null_mut(), 0, n.as_ptr()) };
    // SAFETY: reads the thread's last error code; no arguments.
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let _ = handle;
    if !already {
        return true;
    }
    let ev = name(EVENT_NAME);
    // SAFETY: valid NUL-terminated name; the returned handle is checked for null.
    let h = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, ev.as_ptr()) };
    if !h.is_null() {
        // SAFETY: `h` is a valid event handle opened just above.
        unsafe { SetEvent(h) };
    }
    false
}

/// Thread that brings the window to the front whenever a second instance signals.
pub fn spawn_show_listener(ctx: eframe::egui::Context) {
    let ev = name(EVENT_NAME);
    // SAFETY: valid NUL-terminated name; null attributes are allowed.
    let handle = unsafe { CreateEventW(ptr::null_mut(), 0, 0, ev.as_ptr()) } as usize;
    if handle == 0 {
        return;
    }
    std::thread::spawn(move || {
        loop {
            // SAFETY: `handle` is the event created above, never closed.
            let r = unsafe { WaitForSingleObject(handle as *mut c_void, 60_000) };
            if r == WAIT_OBJECT_0 {
                use eframe::egui::ViewportCommand;
                ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
                ctx.request_repaint();
            }
        }
    });
}
