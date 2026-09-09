use super::*;
use windows_sys::Win32::Graphics::Gdi::{
    RedrawWindow, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE, RDW_UPDATENOW,
};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;

use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetWindow, SetWindowsHookExW, UnhookWindowsHookEx, CWPRETSTRUCT, GW_OWNER,
    HCBT_ACTIVATE, SWP_SHOWWINDOW, WH_CALLWNDPROCRET, WH_CBT, WINDOWPOS, WM_GETICON,
    WM_WINDOWPOSCHANGED,
};

#[cfg(test)]
thread_local! {
    static FIRST_PAINTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PAINT_STARTED: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    static PAINT_MICROS: std::cell::Cell<u128> = const { std::cell::Cell::new(0) };
}

pub(super) unsafe fn paint_now(window: HWND) {
    RedrawWindow(
        window,
        std::ptr::null(),
        std::ptr::null_mut(),
        RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
    );
}

// MessageBox owns its window and modal loop. A thread-local CBT hook applies
// the owner's title-bar icon before activation without replacing native UI.
pub(super) unsafe fn message_box(
    owner: HWND,
    text: *const u16,
    title: *const u16,
    style: u32,
) -> i32 {
    let hook = SetWindowsHookExW(
        WH_CBT,
        Some(activate),
        std::ptr::null_mut(),
        GetCurrentThreadId(),
    );
    let paint_hook = SetWindowsHookExW(
        WH_CALLWNDPROCRET,
        Some(after_show),
        std::ptr::null_mut(),
        GetCurrentThreadId(),
    );
    let result = MessageBoxW(owner, text, title, style);
    if !paint_hook.is_null() {
        UnhookWindowsHookEx(paint_hook);
    }
    if !hook.is_null() {
        UnhookWindowsHookEx(hook);
    }
    result
}

unsafe extern "system" fn activate(code: i32, wparam: WPARAM, lparam: LPARAM) -> isize {
    if code == HCBT_ACTIVATE as i32 {
        let window = wparam as HWND;
        let owner = GetWindow(window, GW_OWNER);
        if is_message_box(window) {
            let icon = if owner.is_null() {
                load_icon(GetModuleHandleW(std::ptr::null()), ICON_BLACK)
            } else {
                SendMessageW(owner, WM_GETICON, ICON_SMALL as usize, 0) as HICON
            };
            if !icon.is_null() {
                set_window_icon(window, icon);
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

unsafe fn is_message_box(window: HWND) -> bool {
    let mut class = [0u16; 16];
    let length = windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW(
        window,
        class.as_mut_ptr(),
        class.len() as i32,
    );
    String::from_utf16_lossy(&class[..length as usize]) == "#32770"
}

unsafe extern "system" fn after_show(code: i32, wparam: WPARAM, lparam: LPARAM) -> isize {
    if code >= 0 {
        let message = &*(lparam as *const CWPRETSTRUCT);
        if message.message == WM_WINDOWPOSCHANGED && message.lParam != 0 {
            let position = &*(message.lParam as *const WINDOWPOS);
            if position.flags & SWP_SHOWWINDOW != 0 && is_message_box(message.hwnd) {
                // Paint after native show processing, when the client and its
                // controls are visible. No sleeps, polling, or nested UI loop.
                paint_now(message.hwnd);
                #[cfg(test)]
                {
                    FIRST_PAINTS.with(|v| v.set(v.get() + 1));
                    if let Some(started) = PAINT_STARTED.with(|v| v.take()) {
                        PAINT_MICROS.with(|v| v.set(started.elapsed().as_micros()));
                    }
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, GetBkMode, GetCurrentObject, GetTextColor, OBJ_BRUSH, OBJ_PEN,
    };

    #[test]
    fn owner_draw_restores_borrowed_dc() {
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            assert!(!dc.is_null());
            SetBkMode(dc, OPAQUE as i32);
            SetTextColor(dc, rgb(17, 34, 51));
            let before = (
                GetCurrentObject(dc, OBJ_BRUSH as u32),
                GetCurrentObject(dc, OBJ_PEN as u32),
                GetBkMode(dc),
                GetTextColor(dc),
            );
            for high_contrast in [false, true] {
                for id in [CMD_MAIN, CMD_PLUGINS, CMD_CHECK_LAUNCHER] {
                    for item_state in [0, ODS_DISABLED, ODS_SELECTED, ODS_FOCUS] {
                        let item = DRAWITEMSTRUCT {
                            CtlID: id,
                            itemState: item_state,
                            hDC: dc,
                            rcItem: RECT {
                                left: 0,
                                top: 0,
                                right: 100,
                                bottom: 32,
                            },
                            ..DRAWITEMSTRUCT::default()
                        };
                        for _ in 0..100 {
                            draw_button(high_contrast, &item);
                        }
                        assert_eq!(
                            (
                                GetCurrentObject(dc, OBJ_BRUSH as u32),
                                GetCurrentObject(dc, OBJ_PEN as u32),
                                GetBkMode(dc),
                                GetTextColor(dc)
                            ),
                            before
                        );
                    }
                }
            }
            DeleteDC(dc);
        }
    }

    #[test]
    #[ignore = "opens real modal windows; run alone with an external 60-second timeout"]
    fn native_message_boxes_paint_and_keep_owner_icon() {
        use std::cell::Cell;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            FindWindowExW, GetDlgItem, IDI_APPLICATION, IDNO, IDOK, WM_COMMAND,
        };
        thread_local! {
            static OBSERVED: Cell<(usize, bool)> = const { Cell::new((0, false)) };
        }
        unsafe extern "system" fn close_box(owner: HWND, _: u32, _: usize, _: u32) {
            let dialog = FindWindowExW(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                to_wide("#32770").as_ptr(),
                to_wide("DSH modal regression").as_ptr(),
            );
            if !dialog.is_null() {
                let icon = SendMessageW(dialog, WM_GETICON, ICON_SMALL as usize, 0) as usize;
                let pending = FIRST_PAINTS.with(Cell::get) == 0;
                OBSERVED.with(|v| v.set((icon, pending)));
                let button = if GetDlgItem(dialog, IDNO).is_null() {
                    if GetDlgItem(dialog, IDOK).is_null() {
                        2
                    } else {
                        IDOK
                    }
                } else {
                    IDNO
                };
                PostMessageW(dialog, WM_COMMAND, button as usize, 0);
            } else {
                // Still allow the outer test watchdog to catch a missing dialog.
                let _ = owner;
            }
        }
        unsafe {
            let owner = CreateWindowExW(
                0,
                to_wide("STATIC").as_ptr(),
                to_wide("DSH modal owner").as_ptr(),
                WS_OVERLAPPED | WS_CAPTION,
                100,
                100,
                400,
                300,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            assert!(!owner.is_null());
            let icon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
            set_window_icon(owner, icon);
            ShowWindow(owner, SW_SHOW);
            let mut paint_samples = Vec::new();
            for style in [
                MB_YESNO | MB_ICONQUESTION,
                MB_OK | MB_ICONINFORMATION,
                MB_OK | MB_ICONERROR,
            ] {
                for _ in 0..5 {
                    FIRST_PAINTS.with(|v| v.set(0));
                    assert_ne!(SetTimer(owner, 77, 200, Some(close_box)), 0);
                    let started = Instant::now();
                    PAINT_STARTED.with(|v| v.set(Some(started)));
                    let result = message_box(
                        owner,
                        to_wide("Native modal painting and icon regression").as_ptr(),
                        to_wide("DSH modal regression").as_ptr(),
                        style,
                    );
                    if style & MB_YESNO != 0 {
                        assert_eq!(result, IDNO);
                    } else {
                        assert!(result == IDOK || result == 2);
                    }
                    assert_eq!(OBSERVED.with(Cell::get), (icon as usize, false));
                    paint_samples.push(PAINT_MICROS.with(Cell::get));
                    assert!(started.elapsed() < Duration::from_secs(2));
                    assert_ne!(IsWindowEnabled(owner), 0);
                }
            }
            KillTimer(owner, 77);
            DestroyWindow(owner);
            paint_samples.sort_unstable();
            println!(
                "{}",
                serde_json::json!({
                    "samples": paint_samples.len(),
                    "firstPaintP95Microseconds": paint_samples[(paint_samples.len() * 95).div_ceil(100) - 1],
                    "method": "MessageBox call to completed native client and child repaint; excludes user dwell time"
                })
            );
        }
    }
}
