#[cfg(windows)]
mod native {
    use std::ffi::c_void;
    #[repr(C)]
    struct LastInputInfo {
        cb_size: u32,
        dw_time: u32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut c_void;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
        fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetTickCount() -> u32;
    }
    pub fn foreground_window() -> Option<(u32, String)> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid == 0 {
            None
        } else {
            Some((pid, format!("Process {pid}")))
        }
    }
    pub fn idle_millis() -> Option<u64> {
        let mut info = LastInputInfo {
            cb_size: std::mem::size_of::<LastInputInfo>() as u32,
            dw_time: 0,
        };
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return None;
        }
        Some(unsafe { GetTickCount() }.wrapping_sub(info.dw_time) as u64)
    }
    pub fn supported() -> bool {
        true
    }
}

#[cfg(target_os = "linux")]
mod native {
    use std::sync::OnceLock;
    use x11rb::{
        connection::Connection,
        protocol::{
            screensaver::ConnectionExt as _,
            xproto::{AtomEnum, ConnectionExt},
        },
        rust_connection::RustConnection,
    };
    static X: OnceLock<Option<(RustConnection, usize, u32, u32)>> = OnceLock::new();
    fn x() -> Option<&'static (RustConnection, usize, u32, u32)> {
        X.get_or_init(|| {
            let (conn, screen) = RustConnection::connect(None).ok()?;
            let active = conn
                .intern_atom(false, b"_NET_ACTIVE_WINDOW")
                .ok()?
                .reply()
                .ok()?
                .atom;
            let pid = conn
                .intern_atom(false, b"_NET_WM_PID")
                .ok()?
                .reply()
                .ok()?
                .atom;
            Some((conn, screen, active, pid))
        })
        .as_ref()
    }
    pub fn supported() -> bool {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return false;
        }
        idle_millis().is_some()
    }
    pub fn foreground_window() -> Option<(u32, String)> {
        let (c, screen, active, pid_atom) = x()?;
        let root = c.setup().roots.get(*screen)?.root;
        let win = c
            .get_property(false, root, *active, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()?;
        let pid = c
            .get_property(false, win, *pid_atom, AtomEnum::CARDINAL, 0, 1)
            .ok()?
            .reply()
            .ok()?
            .value32()?
            .next()?;
        Some((pid, format!("Process {pid}")))
    }
    pub fn idle_millis() -> Option<u64> {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return None;
        }
        let (c, screen, _, _) = x()?;
        let root = c.setup().roots.get(*screen)?.root;
        c.screensaver_query_info(root)
            .ok()?
            .reply()
            .ok()
            .map(|info| info.ms_since_user_input as u64)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod native {
    pub fn foreground_window() -> Option<(u32, String)> {
        None
    }
    pub fn idle_millis() -> Option<u64> {
        None
    }
    pub fn supported() -> bool {
        false
    }
}

pub use native::{foreground_window, idle_millis, supported};
