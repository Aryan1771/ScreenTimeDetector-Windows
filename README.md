# Digital Wellbeing

Digital Wellbeing is a local-first native desktop screen-time tracker built with Rust, Slint, and SQLite. The current development focus is Windows 10/11 x64. Linux files remain in the repository but are not being expanded in this Windows-focused release.

## Windows status

This is an early Windows release, not yet the complete product vision. The Windows dashboard, usage database, focus timer, goal and break reminders, app soft limits, theme setting, export, and installer are implemented. The compatibility table below calls out features that still need implementation so they are not mistaken for finished functionality.

| Feature | Windows status |
|---|---|
| Foreground application usage | Implemented; sampled every five seconds |
| Recent-input idle exclusion | Implemented; usage pauses after 60 seconds idle |
| Lock screen / secure desktop exclusion | Implemented using the active Windows input desktop |
| Sleep / long sampling gap recovery | Long gaps are discarded instead of counted as use |
| Daily, hourly, weekly history | Implemented |
| Daily goal and 80% / reached reminders | Implemented |
| App daily limits | Soft warning only; the app is never forcibly terminated |
| Focus timer | Start, pause/resume, cancel, completion notification, persisted history |
| Break reminders | Configurable continuous-active-use reminder; can be disabled |
| System / light / dark theme | Implemented and saved locally |
| CSV export / delete all history | Implemented |
| Native Windows notifications | Implemented in the installed build; the Start-menu shortcut registers the notification identity |
| Windows installer / optional sign-in launch | Inno Setup script provided |
| System tray / keep tracking with dashboard hidden | Implemented; tray opens dashboard, starts/resumes a 25-minute focus, toggles tracking, or quits; tooltip shows today's total |
| Browser domains / uploads / private-window integration | Not implemented |
| Bedtime schedules / OS Do Not Disturb | Not implemented |
| Hard app enforcement | Not implemented; limits are advisory |
| ARM64 build | Not validated; installer targets x64 |

The application does not collect keystrokes, screenshots, clipboard contents, full URLs, page contents, or telemetry. Application names, local dates, active seconds, preferences, limits, and focus-session records are stored in a per-user SQLite database. The database is not encrypted by the application; rely on Windows account/device encryption for at-rest protection.

## Build and run on Windows

Install the stable Rust MSVC toolchain and Visual Studio Build Tools with the C++ desktop workload. From the repository root:

```powershell
cargo fmt --all -- --check
cargo test --locked
cargo build --release --locked
```

The portable executable is `target\release\digital-wellbeing.exe`. Keep that process open while tracking. Windows toast notifications from a directly launched portable binary may not be associated with a registered application identity; build and install the setup package for reliable notifications.

To build the installer, install Inno Setup 6, then run:

```powershell
cargo build --release --locked
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" packaging\windows\DigitalWellbeing.iss
```

The installer is created at `target\installer\DigitalWellbeing-Setup-x64.exe`. It installs per-user without administrator rights. The optional startup task launches the dashboard at sign-in. Uninstall it from Windows Installed apps; user history is kept unless manually deleted in Settings before uninstalling.

## Windows verification

Automated checks cover SQLite aggregation/sorting, app-limit persistence, preference persistence, history deletion scope, and focus-session recovery. They do not replace real-session verification of Win32 foreground/idle/lock behavior or Windows notification delivery.

Manual smoke test on the Windows machine:

1. Install `DigitalWellbeing-Setup-x64.exe`, launch it from Start, and leave it open.
2. Use a few apps for a few minutes; confirm the foreground app and hourly/day totals update.
3. Leave input idle for over a minute; confirm the idle period is not added.
4. Press Win+L for over a minute, unlock, and confirm locked time is not added.
5. Start a short focus session, pause/resume, then let one complete; check the completion toast and weekly count.
6. In Settings, send a test notification and switch System/Light/Dark themes.
7. Set a low app limit and daily goal to verify the soft warning and goal notifications. The app intentionally will not close the limited application.
8. Export CSV and inspect it, then optionally delete history and confirm settings/limits remain.
9. Close the dashboard with its X; confirm it hides and the tray tooltip/usage continue updating. Start focus from the tray, reopen the dashboard, then use Quit from the tray to stop. If startup was selected during setup, sign out/in and confirm the dashboard starts.

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
