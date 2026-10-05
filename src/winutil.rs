//! Thin Win32 helpers: message pump, timer, autostart registry key, console attach,
//! single-instance mutex, shell open, message box.

use anyhow::{Result, bail};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, SetConsoleOutputCP,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MSG,
    MessageBoxW, PostQuitMessage, SW_SHOWNORMAL, SetTimer, TranslateMessage, WM_TIMER,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "LogiTray";

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Opt in to per-monitor DPI awareness (best effort; ignored on failure).
pub fn enable_dpi_awareness() {
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// True when Logitech G HUB (the app or its agent) is running and will take over devices.
pub fn ghub_running() -> bool {
    const NAMES: &[&str] = &["lghub.exe", "lghub_agent.exe"];
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let name = exe_name(&entry.szExeFile);
                if NAMES.iter().any(|n| name.eq_ignore_ascii_case(n)) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        found
    }
}

fn exe_name(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Local time as `YYYY-MM-DD HH:MM:SS`.
pub fn timestamp() -> String {
    unsafe {
        let mut t = std::mem::zeroed::<windows_sys::Win32::Foundation::SYSTEMTIME>();
        GetLocalTime(&mut t);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
        )
    }
}

pub enum Pump {
    /// A regular message was dispatched.
    Message,
    /// A thread timer fired (carries the timer id passed to `set_timer`).
    Timer(usize),
    /// WM_QUIT received.
    Quit,
}

/// Block for one message, dispatch it, and report what happened.
pub fn pump() -> Pump {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        let r = GetMessageW(&mut msg, null_mut(), 0, 0);
        if r <= 0 {
            return Pump::Quit;
        }
        if msg.message == WM_TIMER && msg.hwnd.is_null() {
            return Pump::Timer(msg.wParam);
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
        Pump::Message
    }
}

/// Creates a thread timer. For window-less timers Windows ignores `id` and allocates its
/// own; the returned value is what `Pump::Timer` will carry.
pub fn set_timer(id: usize, interval_ms: u32) -> usize {
    unsafe { SetTimer(null_mut(), id, interval_ms, None) }
}

pub fn kill_timer(id: usize) {
    unsafe {
        KillTimer(null_mut(), id);
    }
}

pub fn post_quit() {
    unsafe { PostQuitMessage(0) }
}

/// Returns false if another instance already holds the mutex.
pub fn acquire_single_instance(name: &str) -> bool {
    unsafe {
        let h = CreateMutexW(null(), 1, wide(name).as_ptr());
        if h.is_null() {
            return true; // cannot tell; don't block startup
        }
        GetLastError() != ERROR_ALREADY_EXISTS
    }
}

/// Attach to the parent console (for CLI output when built as a GUI subsystem app).
pub fn attach_parent_console() {
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
        SetConsoleOutputCP(65001);
    }
}

pub fn message_box(title: &str, text: &str, error: bool) {
    let flags = MB_OK
        | if error {
            MB_ICONERROR
        } else {
            MB_ICONINFORMATION
        };
    unsafe {
        MessageBoxW(null_mut(), wide(text).as_ptr(), wide(title).as_ptr(), flags);
    }
}

pub fn shell_open(path: &str) {
    unsafe {
        ShellExecuteW(
            null_mut(),
            wide("open").as_ptr(),
            wide(path).as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}

fn open_run_key(access: u32) -> Result<HKEY> {
    let mut key: HKEY = null_mut();
    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            access,
            &mut key,
        )
    };
    if rc != ERROR_SUCCESS {
        bail!("RegOpenKeyExW failed: {rc}");
    }
    Ok(key)
}

pub fn autostart_enabled() -> bool {
    let Ok(key) = open_run_key(KEY_READ) else {
        return false;
    };
    let mut size: u32 = 0;
    let rc = unsafe {
        RegQueryValueExW(
            key,
            wide(RUN_VALUE).as_ptr(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut size,
        )
    };
    unsafe { RegCloseKey(key) };
    rc == ERROR_SUCCESS
}

pub fn set_autostart(enable: bool) -> Result<()> {
    let key = open_run_key(KEY_SET_VALUE)?;
    let rc = unsafe {
        if enable {
            let exe = std::env::current_exe()?;
            let cmd = format!("\"{}\"", exe.display());
            let data = wide(&cmd);
            RegSetValueExW(
                key,
                wide(RUN_VALUE).as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, wide(RUN_VALUE).as_ptr())
        }
    };
    unsafe { RegCloseKey(key) };
    if rc != ERROR_SUCCESS && !(rc == 2 && !enable) {
        bail!("registry write failed: {rc}");
    }
    Ok(())
}
