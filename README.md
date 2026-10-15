# Digital Wellbeing

Digital Wellbeing is a local-first native desktop application for seeing where active computer time goes. It uses Rust, Slint, and SQLite; it does not wrap a browser in a desktop shell.

> **Current release status: early, functional foundation (0.1.0).** This release records foreground application use while the session is recently active, offers a small dashboard, a local focus timer, a persistent daily target, and CSV export. Several features in the long-term product vision are not implemented; the capability table below is deliberately explicit.

## Build and run

Install the Rust stable toolchain and the native build prerequisites for Slint on your platform, then:

```sh
cargo run --release
```

On Windows, the project targets Windows 10/11 x64 with the MSVC Rust toolchain. `cargo build --release` creates `target/release/digital-wellbeing.exe`. This repo does not yet include an installer or ARM64 validation.

On Arch Linux, install the dependencies listed in `packaging/arch/PKGBUILD`, then run `makepkg -si` from that directory. The package installs the binary and desktop launcher. Wayland sessions are unsupported for global foreground tracking in this release; use an X11 session to collect application usage. Do not disable compositor security to use this app.

On Debian-family distributions, build from source using the platform's Rust and native development packages. An AppImage and Debian package are not yet provided.

## What works today

- **Dashboard:** today's active time and a descending per-application breakdown, refreshed every five seconds.
- **Tracking:** Windows foreground process detection and recent-input detection through Win32 APIs; Linux X11 foreground-window/PID and idle time through X11 properties and the Screen Saver extension. Samples are recorded every five seconds. Usage while idle for more than 60 seconds is excluded.
- **Storage:** per-user SQLite database in WAL mode. Data remains on this computer; no telemetry or network connection is used.
- **Daily goal:** adjustable 15–720 minute target, saved locally.
- **Focus:** local 25-minute countdown that does not block or kill applications.
- **Pause/resume:** pauses recording without deleting history.
- **Export:** CSV written to Downloads (or the home folder when Downloads is unavailable).
- **Ordering:** apps are ordered by duration, with alphabetical tie-breaking.

## Capability matrix

| Capability | Windows 10/11 | Linux X11 | Linux Wayland |
|---|---|---|---|
| Dashboard / local database | Yes | Yes | Yes |
| Foreground application time | Yes | Yes | Not available; compositor APIs are intentionally not bypassed |
| Idle exclusion | Win32 last-input time | XScreenSaver extension | Not available |
| Per-site browser tracking | Not implemented | Not implemented | Not implemented |
| Lock/suspend event handling | Not directly subscribed; long-idle intervals are excluded | Long-idle intervals are excluded | Not available |
| Native notification / tray / login startup | Not implemented | Not implemented | Not implemented |
| App limits / enforcement / bedtime / DND | Not implemented | Not implemented | Not implemented |
| History charts / weekly analytics | Not implemented | Not implemented | Not implemented |

KDE Plasma Wayland, GNOME Wayland, Hyprland, and Sway do not provide a single portable, unrestricted API for discovering other clients' focused applications. This release therefore reports no app usage on Wayland rather than claiming inaccurate tracking. X11 support depends on an X server exposing `_NET_ACTIVE_WINDOW`, `_NET_WM_PID`, and the XScreenSaver extension.

## Privacy and data

The database is stored in the operating system's per-user application data directory (`%APPDATA%/DigitalWellbeing/DigitalWellbeing/data` on Windows; the XDG data directory on Linux). It stores application executable names, local calendar dates, and accumulated active seconds. It does not collect keystrokes, screenshots, clipboard data, page titles, full URLs, or page contents. Website tracking and browser extensions are not included in this release. Exported CSV may contain application names; keep it private if that matters to you.

Close the app to stop monitoring. Pause tracking from the dashboard to stop adding usage while keeping the app open. There is currently no background service, autostart, data deletion UI, or retention setting.

## Architecture

- `src/platform.rs`: minimal OS-specific foreground-PID and idle-time adapters.
- `src/storage.rs`: SQLite schema, WAL configuration, aggregation, preferences, and CSV export.
- `src/main.rs`: tracker thread and Slint UI integration.
- `ui/main.slint`: native desktop dashboard.
- `packaging/arch`: Arch PKGBUILD and desktop launcher.

The monitor is intentionally unprivileged and does not terminate applications. Its X11 and Win32 polling interval is five seconds. It stores one local aggregate per application/day; gaps longer than the idle threshold are excluded. This is an early implementation rather than a tamper-proof or forensic measurement tool.

## License

GNU General Public License v3.0 only; see [LICENSE](LICENSE).
