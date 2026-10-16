use std::{ffi::c_void, path::PathBuf, ptr, thread, time::Duration};

use winreg::{RegKey, enums::HKEY_CURRENT_USER};

const STARTUP_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const STARTUP_VALUE: &str = "DigitalWellbeing";
const ERROR_ALREADY_EXISTS: u32 = 183;
const EVENT_MODIFY_STATE: u32 = 0x0002;
const WAIT_OBJECT_0: u32 = 0;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(attributes: *mut c_void, initial_owner: i32, name: *const u16) -> *mut c_void;
    fn SetLastError(error: u32);
    fn CreateEventW(
        attributes: *mut c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> *mut c_void;
    fn SetEvent(event: *mut c_void) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

pub fn configure_shell_identity() {
    // Set this before creating any windows so Windows uses this app's shortcut
    // and icon instead of inheriting the launcher's taskbar grouping.
    let id = wide("DigitalWellbeing.Desktop");
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(
        callback: Option<unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32>,
        parameter: *mut c_void,
    ) -> i32;
    fn GetWindowThreadProcessId(window: *mut c_void, process_id: *mut u32) -> u32;
    fn IsWindowVisible(window: *mut c_void) -> i32;
    fn ShowWindow(window: *mut c_void, command: i32) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
    fn ShellExecuteW(
        window: *mut c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show_command: i32,
    ) -> *mut c_void;
}

pub fn hide_dashboard_window() {
    unsafe {
        EnumWindows(
            Some(hide_process_window),
            std::process::id() as usize as *mut c_void,
        );
    }
}

pub fn open_notification_settings() -> Result<(), String> {
    let operation = wide("open");
    let uri = wide("ms-settings:notifications");
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            uri.as_ptr(),
            ptr::null(),
            ptr::null(),
            1,
        )
    };
    if result as usize > 32 {
        Ok(())
    } else {
        Err(format!(
            "Could not open Windows notification settings ({})",
            result as usize
        ))
    }
}

pub fn open_browser_extension_folder() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|_| "Could not locate the application folder.".to_owned())?;
    let directory = executable
        .parent()
        .ok_or("Could not locate the application folder.")?;
    let installed = directory.join("browser/chromium");
    let development = directory.join("../../browser/chromium");
    let folder = if installed.join("manifest.json").is_file() {
        installed
    } else if development.join("manifest.json").is_file() {
        development
    } else {
        return Err(
            "Install the latest Digital Wellbeing update to set up website tracking.".into(),
        );
    };
    open_target(&folder.to_string_lossy(), None)
}

pub fn open_opera_extensions() -> Result<(), String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or("Open opera://extensions in Opera GX to continue.")?;
    let executable = ["Programs/Opera GX/opera.exe", "Programs/Opera/opera.exe"]
        .into_iter()
        .map(|path| local.join(path))
        .find(|path| path.is_file())
        .ok_or("Open opera://extensions in Opera GX to continue.")?;
    open_target(&executable.to_string_lossy(), Some("opera://extensions"))
}

fn open_target(target: &str, arguments: Option<&str>) -> Result<(), String> {
    let operation = wide("open");
    let target = wide(target);
    let arguments = arguments.map(wide);
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            arguments
                .as_ref()
                .map_or(ptr::null(), |value| value.as_ptr()),
            ptr::null(),
            1,
        )
    };
    if result as usize > 32 {
        Ok(())
    } else {
        Err("Could not open the requested location. Please try again.".into())
    }
}

unsafe extern "system" fn hide_process_window(window: *mut c_void, parameter: *mut c_void) -> i32 {
    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(window, &mut process_id);
        if process_id as usize == parameter as usize && IsWindowVisible(window) != 0 {
            ShowWindow(window, 0);
        }
    }
    1
}

pub struct SingleInstance {
    mutex: *mut c_void,
    activation_event: *mut c_void,
}

impl SingleInstance {
    /// Returns None when another process owns the app; that process is signaled
    /// to show its existing window instead of starting a second tracker.
    pub fn acquire() -> Result<Option<Self>, String> {
        let mutex_name = wide("Local\\DigitalWellbeing.0.2.Instance");
        let event_name = wide("Local\\DigitalWellbeing.0.2.Activate");
        unsafe { SetLastError(0) };
        let mutex = unsafe { CreateMutexW(ptr::null_mut(), 0, mutex_name.as_ptr()) };
        if mutex.is_null() {
            return Err(format!(
                "Could not create the single-instance mutex ({}).",
                unsafe { GetLastError() }
            ));
        }

        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            let mut event = ptr::null_mut();
            for _ in 0..20 {
                event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, event_name.as_ptr()) };
                if !event.is_null() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            if !event.is_null() {
                unsafe {
                    SetEvent(event);
                    CloseHandle(event);
                }
            }
            unsafe { CloseHandle(mutex) };
            return Ok(None);
        }

        let activation_event = unsafe { CreateEventW(ptr::null_mut(), 0, 0, event_name.as_ptr()) };
        if activation_event.is_null() {
            unsafe { CloseHandle(mutex) };
            return Err(format!(
                "Could not create the app activation event ({}).",
                unsafe { GetLastError() }
            ));
        }
        Ok(Some(Self {
            mutex,
            activation_event,
        }))
    }

    pub fn take_activation_request(&self) -> bool {
        unsafe { WaitForSingleObject(self.activation_event, 0) == WAIT_OBJECT_0 }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.activation_event);
            CloseHandle(self.mutex);
        }
    }
}

pub fn startup_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(STARTUP_KEY)
        .and_then(|key| key.get_value::<String, _>(STARTUP_VALUE))
        .is_ok()
}

pub fn set_startup_enabled(enabled: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(STARTUP_KEY)
        .map_err(|error| format!("Could not open the current-user startup settings: {error}"))?;
    if enabled {
        let executable: PathBuf = std::env::current_exe()
            .map_err(|error| format!("Could not locate the application executable: {error}"))?;
        let command = format!("\"{}\" --background", executable.display());
        key.set_value(STARTUP_VALUE, &command)
            .map_err(|error| format!("Could not enable sign-in startup: {error}"))
    } else {
        match key.delete_value(STARTUP_VALUE) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Could not disable sign-in startup: {error}")),
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
