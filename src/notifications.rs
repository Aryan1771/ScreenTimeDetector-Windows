#[cfg(windows)]
pub fn register() -> Result<(), String> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey("Software\\Classes\\AppUserModelId\\DigitalWellbeing.Desktop")
        .map_err(|error| error.to_string())?;
    key.set_value("DisplayName", &"Digital Wellbeing")
        .map_err(|error| error.to_string())?;
    if let Ok(executable) = std::env::current_exe()
        && let Some(folder) = executable.parent()
    {
        let icon = folder.join(format!("assets/logo-{}.png", env!("CARGO_PKG_VERSION")));
        if icon.is_file() {
            key.set_value("IconUri", &icon.to_string_lossy().as_ref())
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn register() -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub fn toast(title: &str, body: &str) -> Result<(), String> {
    use winrt_toast::{Toast, ToastManager};
    let manager = ToastManager::new("DigitalWellbeing.Desktop");
    let mut notification = Toast::new();
    notification.text1(title).text2(body);
    manager
        .show(&notification)
        .map_err(|error| format!("{error:?}"))
}

#[cfg(not(windows))]
pub fn toast(_title: &str, _body: &str) -> Result<(), String> {
    Ok(())
}
