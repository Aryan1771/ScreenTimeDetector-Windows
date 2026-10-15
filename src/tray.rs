use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    StartFocus,
    ToggleTracking,
    Quit,
}

#[derive(Clone)]
pub struct Tray {
    icon: TrayIcon,
    open_id: tray_icon::menu::MenuId,
    focus_id: tray_icon::menu::MenuId,
    toggle_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
}

impl Tray {
    pub fn new() -> Result<Self, String> {
        let open = MenuItem::with_id("open-dashboard", "Open dashboard", true, None);
        let focus = MenuItem::with_id("start-focus", "Start 25-minute focus", true, None);
        let toggle = MenuItem::with_id("toggle-tracking", "Pause or resume tracking", true, None);
        let quit = MenuItem::with_id("quit", "Quit Digital Wellbeing", true, None);
        let menu = Menu::new();
        menu.append(&open).map_err(|e| e.to_string())?;
        menu.append(&focus).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&toggle).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&quit).map_err(|e| e.to_string())?;

        let icon = Icon::from_rgba(wellbeing_icon(), 32, 32).map_err(|e| e.to_string())?;
        let icon = TrayIconBuilder::new()
            .with_tooltip("Digital Wellbeing · tracking")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            icon,
            open_id: open.id().clone(),
            focus_id: focus.id().clone(),
            toggle_id: toggle.id().clone(),
            quit_id: quit.id().clone(),
        })
    }

    pub fn set_tooltip(&self, text: &str) {
        let _ = self.icon.set_tooltip(Some(text));
    }

    pub fn next_action(&self) -> Option<Action> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.open_id {
                return Some(Action::Open);
            }
            if event.id == self.focus_id {
                return Some(Action::StartFocus);
            }
            if event.id == self.toggle_id {
                return Some(Action::ToggleTracking);
            }
            if event.id == self.quit_id {
                return Some(Action::Quit);
            }
        }
        None
    }
}

fn wellbeing_icon() -> Vec<u8> {
    let mut pixels = vec![0_u8; 32 * 32 * 4];
    for y in 0..32_i32 {
        for x in 0..32_i32 {
            let dx = x - 15;
            let dy = y - 15;
            let distance_squared = dx * dx + dy * dy;
            let ring = (82..=196).contains(&distance_squared);
            let pulse = (6..=25).contains(&x)
                && y == 16
                    + if x < 13 {
                        0
                    } else if x < 16 {
                        -5
                    } else if x < 19 {
                        5
                    } else {
                        0
                    };
            if ring || pulse {
                let index = ((y * 32 + x) * 4) as usize;
                pixels[index] = if ring { 84 } else { 131 };
                pixels[index + 1] = if ring { 125 } else { 155 };
                pixels[index + 2] = if ring { 120 } else { 138 };
                pixels[index + 3] = 255;
            }
        }
    }
    pixels
}
