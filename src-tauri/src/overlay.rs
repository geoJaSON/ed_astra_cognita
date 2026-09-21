//! Win32 details for keeping the overlay on top of the game without ever taking focus.

pub const GAME_EXE: &str = "EliteDangerous64.exe";

/// `Status.json` GuiFocus values where the overlay stays visible:
/// no panel open, FSS scanner, and DSS surface mapping.
pub fn gui_focus_allows_overlay(gui_focus: u32) -> bool {
    matches!(gui_focus, 0 | 9 | 10)
}

#[cfg(windows)]
mod imp {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId, SetWindowLongPtrW, SetWindowPos,
        GWL_EXSTYLE, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };

    /// Marks the window as never-activating and hides it from Alt+Tab.
    pub fn make_non_activating(hwnd: HWND) {
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
        }
    }

    /// Borderless games can climb above other topmost windows when focused, so this is re-asserted
    /// periodically. Async because it is called off the UI thread.
    pub fn raise_topmost(hwnd: HWND) {
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
            );
        }
    }

    #[derive(Default)]
    pub struct ForegroundWatcher {
        last_hwnd: usize,
        last_is_game: bool,
    }

    impl ForegroundWatcher {
        pub fn game_is_foreground(&mut self) -> bool {
            let hwnd = unsafe { GetForegroundWindow() };
            let key = hwnd.0 as usize;
            if key != self.last_hwnd {
                self.last_hwnd = key;
                self.last_is_game = process_exe_name(hwnd).is_some_and(|n| n.eq_ignore_ascii_case(super::GAME_EXE));
            }
            self.last_is_game
        }
    }

    fn process_exe_name(hwnd: HWND) -> Option<String> {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return None;
        }
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(process);
            result.ok()?;
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit('\\').next().map(str::to_string)
        }
    }
}

#[cfg(windows)]
pub use imp::*;

#[cfg(not(windows))]
#[derive(Default)]
pub struct ForegroundWatcher;

#[cfg(not(windows))]
impl ForegroundWatcher {
    pub fn game_is_foreground(&mut self) -> bool {
        false
    }
}
