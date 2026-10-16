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
        fn IsIconic(hwnd: *mut c_void) -> i32;
        fn IsWindowVisible(hwnd: *mut c_void) -> i32;
        fn GetClassNameW(hwnd: *mut c_void, class_name: *mut u16, max_count: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
        fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
        fn OpenInputDesktop(flags: u32, inherit: i32, desired_access: u32) -> *mut c_void;
        fn CloseDesktop(desktop: *mut c_void) -> i32;
        fn GetUserObjectInformationW(
            object: *mut c_void,
            index: i32,
            info: *mut c_void,
            length: u32,
            needed: *mut u32,
        ) -> i32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            hkey: *mut c_void,
            subkey: *const u16,
            value: *const u16,
            flags: u32,
            value_type: *mut u32,
            data: *mut c_void,
            data_size: *mut u32,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetTickCount() -> u32;
    }
    pub fn foreground_window() -> Option<(u32, String)> {
        if session_locked() != Some(false) {
            return None;
        }
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() || unsafe { IsIconic(hwnd) } != 0 || unsafe { IsWindowVisible(hwnd) } == 0
        {
            return None;
        }
        let mut class_name = [0_u16; 64];
        let length =
            unsafe { GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32) };
        let class_name = String::from_utf16_lossy(&class_name[..length.max(0) as usize]);
        if matches!(
            class_name.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
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
        if session_locked() != Some(false) {
            return None;
        }
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

    // Windows switches the input desktop to Winlogon while the workstation is
    // locked. Treat an inaccessible desktop as locked so secure desktop time is
    // never attributed to whichever process happened to be in the foreground.
    fn session_locked() -> Option<bool> {
        const UOI_NAME: i32 = 2;
        const DESKTOP_READOBJECTS: u32 = 0x0001;
        let desktop = unsafe { OpenInputDesktop(0, 0, DESKTOP_READOBJECTS) };
        if desktop.is_null() {
            return None;
        }
        let mut name = [0_u16; 256];
        let mut needed = 0_u32;
        let success = unsafe {
            GetUserObjectInformationW(
                desktop,
                UOI_NAME,
                name.as_mut_ptr().cast(),
                (name.len() * std::mem::size_of::<u16>()) as u32,
                &mut needed,
            )
        };
        unsafe { CloseDesktop(desktop) };
        if success == 0 {
            return None;
        }
        let name = String::from_utf16_lossy(
            &name[..name.iter().position(|c| *c == 0).unwrap_or(name.len())],
        );
        Some(name.eq_ignore_ascii_case("Winlogon"))
    }
    pub fn system_dark_theme() -> bool {
        let subkey: Vec<u16> =
            "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
                .encode_utf16()
                .collect();
        let name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
        let mut value = 1_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                (-2147483647_isize) as *mut c_void,
                subkey.as_ptr(),
                name.as_ptr(),
                0x10,
                std::ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut size,
            )
        };
        status == 0 && value == 0
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
    pub fn system_dark_theme() -> bool {
        false
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
    pub fn system_dark_theme() -> bool {
        false
    }
}

pub use native::{foreground_window, idle_millis, supported, system_dark_theme};
