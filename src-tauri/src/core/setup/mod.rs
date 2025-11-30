use tauri::{AppHandle, WebviewWindow};

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub mod common;

#[cfg(target_os = "windows")]
mod win;

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
use win::*;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use common::*;

pub fn default(
    app_handle: &AppHandle,
    main_window: WebviewWindow,
    preference_window: WebviewWindow,
) {
    #[cfg(debug_assertions)]
    main_window.open_devtools();

    platform(app_handle, main_window.clone(), preference_window.clone());
}
