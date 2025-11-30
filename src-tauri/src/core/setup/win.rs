use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use tauri::{AppHandle, Emitter, WebviewWindow};

use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
use windows::Win32::System::Com::*;
use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, MSG,
    OBJID_WINDOW, TranslateMessage, WINEVENT_OUTOFCONTEXT,
};
use windows::{
    Win32::UI::Shell::{QUNS_BUSY, SHQueryUserNotificationState},
    core::Result,
};

pub struct FullScreenDetector {
    hook: HWINEVENTHOOK,
}

static BROWSER_FULLSCREEN: AtomicBool = AtomicBool::new(false);
static APP_HANDLE: Mutex<Option<AppHandle>> = Mutex::new(None);

impl FullScreenDetector {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).unwrap();
        }

        // https://learn.microsoft.com/zh-cn/windows/win32/api/winuser/nf-winuser-setwineventhook
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_OBJECT_LOCATIONCHANGE,
                None,
                Some(Self::win_event_callback),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };

        if hook.is_invalid() {
            let _ = unsafe { windows::Win32::Foundation::GetLastError() };
        }

        Ok(Self { hook })
    }

    extern "system" fn win_event_callback(
        _h_win_event_hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        id_object: i32,
        _id_child: i32,
        _dw_event_thread: u32,
        _dwms_event_time: u32,
    ) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }

        if id_object != OBJID_WINDOW.0 {
            return;
        }

        match event {
            EVENT_SYSTEM_FOREGROUND => Self::check_browser_fullscreen(hwnd),
            EVENT_OBJECT_LOCATIONCHANGE => Self::check_browser_fullscreen(hwnd),
            EVENT_SYSTEM_MINIMIZEEND => Self::check_browser_fullscreen(hwnd),
            _ => {}
        }
    }

    fn emit_event(name: &str) {
        if let Ok(handle_guard) = APP_HANDLE.lock() {
            if let Some(app_handle) = &*handle_guard {
                let _ = app_handle.emit(&name, ());
            }
        }
    }

    fn check_browser_fullscreen(hwnd: HWND) {
        let full_screen = Self::is_full_screen().unwrap();
        let was_full_screen = BROWSER_FULLSCREEN.load(Ordering::SeqCst);

        unsafe {
            if !was_full_screen && Self::is_browser_window(hwnd) && full_screen {
                Self::emit_event("browser-fullscreen");
                BROWSER_FULLSCREEN.store(true, Ordering::SeqCst);
            } else if was_full_screen && !full_screen {
                Self::emit_event("browser-exit-fullscreen");
                BROWSER_FULLSCREEN.store(false, Ordering::SeqCst);
            }
        }
    }

    fn is_full_screen() -> Result<bool> {
        unsafe {
            let ns = SHQueryUserNotificationState();

            match ns {
                // https://learn.microsoft.com/zh-cn/windows/win32/api/shellapi/nf-shellapi-shqueryusernotificationstate
                Ok(value) => match value {
                    QUNS_BUSY => Ok(true), // full screen mod
                    _ => Ok(false),
                },
                Err(e) => Err(e),
            }
        }
    }

    unsafe fn is_browser_window(hwnd: HWND) -> bool {
        let mut window_text = [0u16; 256];
        let len = unsafe { GetWindowTextW(hwnd, &mut window_text) };
        if len == 0 {
            return false;
        }

        let mut process_id: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        }

        if process_id == 0 {
            return false;
        }

        let process_handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
                false,
                process_id,
            )
        };
        if process_handle.is_err() {
            return false;
        }

        let process_handle = process_handle.unwrap();
        let mut process_name = [0u16; MAX_PATH as usize];
        let size =
            unsafe { GetModuleBaseNameW(process_handle, core::mem::zeroed(), &mut process_name) };

        unsafe {
            let _ = CloseHandle(process_handle);
        }

        if size == 0 {
            return false;
        }
        let process_name_str = String::from_utf16_lossy(&process_name[..size as usize]);

        let browser_processes = [
            "chrome.exe",
            "firefox.exe",
            "msedge.exe",
            "opera.exe",
            "safari.exe",
            "iexplore.exe", // can not only brower
        ];

        let is_browser_process = browser_processes
            .iter()
            .any(|&name| process_name_str.eq_ignore_ascii_case(name));

        is_browser_process
    }

    pub fn run_message_loop(&self) {
        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        };
    }
}

impl Drop for FullScreenDetector {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWinEvent(self.hook);
            CoUninitialize();
        }
    }
}

pub fn platform(
    app_handle: &AppHandle,
    _main_window: WebviewWindow,
    _preference_window: WebviewWindow,
) {
    let mut handle = APP_HANDLE.lock().unwrap();
    *handle = Some(app_handle.clone());

    thread::spawn(move || match FullScreenDetector::new() {
        Ok(detector) => {
            detector.run_message_loop();
        }
        Err(_e) => {}
    });
}
