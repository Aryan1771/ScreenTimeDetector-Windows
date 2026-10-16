# Digital Wellbeing

Digital Wellbeing is a local-first native desktop screen-time tracker built with Rust, Slint, and SQLite. The current development focus is Windows 10/11 x64. Linux files remain in the repository but are not being expanded in this Windows-focused release.

## Windows status

This is an early Windows release, not yet the complete product vision. The Windows dashboard, usage database, focus timer, goal and break reminders, app soft limits, theme setting, export, and installer are implemented. The compatibility table below calls out features that still need implementation so they are not mistaken for finished functionality.

| Feature | Windows status |
|---|---|
| Foreground application usage | One focused application at a time, sampled every second; minimized, hidden, desktop and taskbar windows are excluded |
| Recent-input idle exclusion | Implemented; usage pauses after 60 seconds idle |
| Lock screen / secure desktop exclusion | Implemented using the active Windows input desktop |
| Sleep / long sampling gap recovery | Long gaps are discarded instead of counted as use |
| Daily, hourly, weekly history | Implemented |
| Dashboard chart selector | Daily view shows a Monday–Sunday week; select a bar to change the day. Hourly view shows the selected day's 24 hours; select an hour to see its apps and durations. The day total includes hours, minutes and seconds. |
| Daily goal and 80% / reached reminders | Implemented |
| App daily limits | Soft warning only; the app is never forcibly terminated |
| Focus timer | Start, pause/resume, cancel, completion notification, persisted history |
| Break reminders | Configurable continuous-active-use reminder; can be disabled |
| System / light / dark theme | Implemented and saved locally |
| CSV export / delete all history | Implemented |
| Native Windows notifications | Implemented in the installed build; the Start-menu shortcut registers the notification identity |
| Windows installer / sign-in launch | Per-user installer; startup is selected by default and can be changed in setup or Settings; starts hidden after sign-in |
| System tray / keep tracking with dashboard hidden | Implemented; tray opens dashboard, starts/resumes a 25-minute focus, toggles tracking, or quits; tooltip shows today's total |
| One running tracker / fast dashboard reopen | Implemented; a second launch activates the existing process instead of duplicating tracking |
| Windows console window | Disabled; app launches as a GUI process |
| Browser domains | Optional Chromium extension and native bridge for Opera GX, Chrome and Edge; app includes setup instructions and extension-folder shortcut |
| Browser private windows | Website attribution is rejected for incognito/private windows; browser process time remains included in app totals |
| Firefox, Brave, Vivaldi website integration | Not validated |
| Bedtime schedules / OS Do Not Disturb | Not implemented |
| Hard app enforcement | Not implemented; limits are advisory |
| ARM64 build | Not validated; installer targets x64 |

The application does not collect keystrokes, screenshots, clipboard contents, full URLs, page contents, or telemetry. The optional browser extension reads the active tab URL locally, reduces it to a hostname, and sends only the hostname and a short duration sample to the native app. A local Windows pipe forwards samples to the running desktop tracker, so the browser host never opens a separate database, even when Windows redirects a launcher's AppData folder. The tracker verifies the host executable, launching browser, foreground state, idle state and tracking pause setting. Application names, website hostnames, local dates, active seconds, preferences, limits, and focus-session records are stored in a per-user SQLite database. The database is not encrypted by the application; rely on Windows account/device encryption for at-rest protection.

The application and extension use the transparent PNG in `assets/logo.png`, including a transparent heart cutout. Windows executable/installer icons contain PNG frames in the required ICO container; the tray uses RGBA pixels derived from the same PNG.

## Build and run on Windows

Install the stable Rust MSVC toolchain and Visual Studio Build Tools with the C++ desktop workload. From the repository root:

```powershell
cargo fmt --all -- --check
cargo test --locked
cargo build --release --locked
```

The portable executable is `target\release\digital-wellbeing.exe`. The process stays in the tray when the dashboard is hidden. Opening the app again brings the existing dashboard forward and shows accumulated time. Windows toast notifications from a directly launched portable binary may not be associated with a registered application identity; build and install the setup package for reliable notifications.

To build the installer, install Inno Setup 6, then run:

```powershell
cargo build --release --locked
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" packaging\windows\DigitalWellbeing.iss
```

The installer is created at `target\installer\DigitalWellbeing-Setup-x64.exe`. It installs per-user without administrator rights. Its startup task is selected by default and launches the app hidden in the tray at sign-in; you can clear it during setup or change the setting later in Settings. Uninstall it from Windows Installed apps; user history is kept unless manually deleted in Settings before uninstalling.

## Windows verification

Automated checks cover foreground intervals during app switches, minimization/restoration, paused/idle periods and sleep; SQLite aggregation/sorting; website domain validation and aggregation; weekly/monthly analytics; app-limit and preference persistence; history deletion scope; and focus-session recovery. The Chromium extension has a Node test for tab changes, hostname-only output, private windows, idle state, and browser focus. They do not replace real-session verification of Win32 foreground/idle/lock behavior or a live browser native-messaging run.

For domain-level tracking in Opera GX, open **Set up website tracking** on the dashboard, then open `opera://extensions`, enable Developer mode, and choose **Load unpacked** with the installed app's `browser/chromium` folder. The app has buttons to open both locations. Keep a normal website such as youtube.com focused for at least ten seconds. Website tracking starts only after the extension is loaded; installing the desktop app alone cannot install a browser extension. The installer registers the Chrome native bridge location used by Opera, and Edge's native bridge. Private/incognito access is not requested, and private tabs are rejected if the setting is later enabled manually. Website durations are a breakdown of browser time and are never added again to the overall total.

Manual smoke test on the Windows machine:

1. Install `DigitalWellbeing-Setup-x64.exe`, launch it from Start, and leave it open.
2. Use a few apps for a few minutes; confirm the foreground app and hourly/day totals update.
3. Leave input idle for over a minute; confirm the idle period is not added.
4. Press Win+L for over a minute, unlock, and confirm locked time is not added.
5. Start a short focus session, pause/resume, then let one complete; check the completion toast and weekly count.
6. In Settings, send a test notification and switch System/Light/Dark themes.
7. Set a low app limit and daily goal to verify the soft warning and goal notifications. The app intentionally will not close the limited application.
8. Export CSV and inspect it, then optionally delete history and confirm settings/limits remain.
9. Close the dashboard with its X; confirm it hides and the tray tooltip/usage continue updating. Launch the app again and confirm the existing dashboard reopens without a second tracker. Start focus from the tray, reopen the dashboard, then use Quit from the tray to stop.
10. Enable sign-in startup in Settings (or in the installer), sign out/in, and confirm the app starts hidden with no console window. Open it from Start and confirm today's usage accumulated since sign-in.
11. In Chrome or Edge, load the unpacked extension. Browse a normal site while the browser is focused and confirm the hostname appears. Switch to another app, idle for a minute, open a private window, and visit a file or browser settings page; those periods should not be attributed to a website.
12. In Opera GX, follow the dashboard's website setup page, then verify youtube.com usage. Minimize Opera and use a different app: only that app's time should grow. Restore Opera and confirm website attribution resumes. Pause tracking and confirm both app and website durations stop.
13. With many recorded applications and websites, scroll through the dashboard: every row must remain inside its own card, and progress bars must align with their labels. Verify the supplied green heart logo in the sidebar, title/taskbar, tray and installer.

## Linux

The repository still contains the earlier Linux X11 implementation and Arch packaging. Linux is not the focus of this pass and has not been revalidated against these Windows changes. Wayland global foreground-app tracking remains unavailable; the app does not attempt to bypass compositor security.

## Project structure

- `src/main.rs`: Slint application, monitoring loop, soft reminders, and UI actions.
- `src/platform.rs`: Windows foreground, idle, secure desktop, and theme access; existing Linux X11 adapter.
- `src/storage.rs`: SQLite schema, usage aggregation, preferences, limits, focus history, and CSV export.
- `src/notifications.rs`: native notification registration and delivery.
- `ui/main.slint`: desktop dashboard and settings UI.
- `packaging/windows`: per-user Windows installer definition.
- `packaging/arch`: existing Arch package files (unchanged in this pass).

## License

GNU General Public License v3.0 only; see [LICENSE](LICENSE).
