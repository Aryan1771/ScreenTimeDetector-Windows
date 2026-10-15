#[cfg(windows)]
pub fn register() -> Result<(), String> {
    winrt_toast::register("DigitalWellbeing.Desktop", "Digital Wellbeing", None)
        .map_err(|error| format!("{error:?}"))
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
