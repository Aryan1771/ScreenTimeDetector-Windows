#![cfg_attr(windows, windows_subsystem = "windows")]

mod notifications;
mod platform;
mod storage;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod windows_app;
#[cfg(not(windows))]
mod windows_app {
    pub fn startup_enabled() -> bool {
        false
    }
    pub fn set_startup_enabled(_enabled: bool) -> Result<(), String> {
        Err("Sign-in startup is only available on Windows.".into())
    }
}

use chrono::{Local, NaiveDate, Timelike};
use directories::ProjectDirs;
use slint::{ModelRc, TimerMode, VecModel};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use sysinfo::{Pid, System};

slint::include_modules!();

struct FocusSession {
    id: i64,
    deadline: Option<Instant>,
    remaining: Duration,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut background_start = false;
    let mut data_dir_override = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--background" {
            background_start = true;
        } else if arg == "--data-dir" {
            data_dir_override = args.next().map(PathBuf::from);
        } else if let Some(path) = arg
            .to_str()
            .and_then(|value| value.strip_prefix("--data-dir="))
        {
            data_dir_override = Some(PathBuf::from(path));
        }
    }
    #[cfg(windows)]
    let instance_guard = {
        let Some(instance) = windows_app::SingleInstance::acquire()? else {
            return Ok(());
        };
        instance
    };
    let dirs = ProjectDirs::from("org", "DigitalWellbeing", "DigitalWellbeing")
        .ok_or("could not locate per-user data directory")?;
    let data_dir = data_dir_override.unwrap_or_else(|| dirs.data_dir().to_path_buf());
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("wellbeing.sqlite3");
    let db = storage::Database::open(&db_path)?;
    let _ = db.recover_focus_sessions();
    let monitoring_supported = platform::supported();
    let tracking = Arc::new(AtomicBool::new(true));
    let break_minutes = Arc::new(AtomicU32::new(
        db.preference("break_reminder_minutes", "45")?
            .parse()
            .unwrap_or(45),
    ));
    let selected_day = Arc::new(std::sync::Mutex::new(Local::now().date_naive()));
    let focus = Arc::new(std::sync::Mutex::new(None::<FocusSession>));
    let (alert_tx, alert_rx) = mpsc::channel::<(String, String)>();
    let notification_setup = notifications::register();
    start_tracker(
        db_path.clone(),
        tracking.clone(),
        break_minutes.clone(),
        alert_tx.clone(),
    );

    let window = AppWindow::new()?;
    #[cfg(windows)]
    if background_start {
        window.window().hide()?;
    }
    #[cfg(windows)]
    let tray_instance = tray::Tray::new().ok();
    #[cfg(windows)]
    let tray_available = tray_instance.is_some();
    #[cfg(windows)]
    if tray_instance.is_none() {
        window.set_notice_text(
            "System tray integration could not start; keep this window open to track usage.".into(),
        );
    }
    #[cfg(windows)]
    {
        window.window().on_close_requested(move || {
            if tray_available {
                slint::CloseRequestResponse::HideWindow
            } else {
                slint::CloseRequestResponse::KeepWindowShown
            }
        });
    }
    window.set_tracking(true);
    window.set_status_text(
        if monitoring_supported {
            "Tracking is active"
        } else {
            "Global app tracking is unavailable in this session"
        }
        .into(),
    );
    window.set_usages(ModelRc::new(VecModel::from(Vec::<UsageItem>::new())));
    window.set_hourly_bars(ModelRc::new(VecModel::from(Vec::<ChartItem>::new())));
    window.set_weekly_bars(ModelRc::new(VecModel::from(Vec::<ChartItem>::new())));
    let initial_day = Local::now().date_naive();
    window.set_date_label(format_day(initial_day).into());
    if let Ok((total, _)) = db.summary_for_day(&initial_day) {
        window.set_today_total(format_duration(total).into());
    }
    window.set_startup_supported(cfg!(windows));
    window.set_startup_enabled(windows_app::startup_enabled());
    if let Err(error) = notification_setup {
        window.set_notice_text(format!("Windows notifications unavailable: {error}").into());
    }

    {
        let weak = window.as_weak();
        window.on_set_startup_enabled(move |enabled| {
            match windows_app::set_startup_enabled(enabled) {
                Ok(()) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_startup_enabled(enabled);
                        w.set_notice_text(if enabled {
                            "Digital Wellbeing will start hidden in the tray when you sign in."
                                .into()
                        } else {
                            "Automatic sign-in startup is disabled.".into()
                        });
                    }
                }
                Err(error) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_startup_enabled(windows_app::startup_enabled());
                        w.set_notice_text(error.into());
                    }
                }
            }
        });
    }

    {
        let weak = window.as_weak();
        let state = tracking.clone();
        let supported = monitoring_supported;
        window.on_toggle_tracking(move || {
            let next = !state.load(Ordering::Relaxed);
            state.store(next, Ordering::Relaxed);
            if let Some(window) = weak.upgrade() {
                window.set_tracking(next);
                window.set_status_text(
                    if !supported {
                        "Global app tracking is unavailable in this session"
                    } else if next {
                        "Tracking is active"
                    } else {
                        "Tracking is paused"
                    }
                    .into(),
                );
            }
        });
    }
    {
        let weak = window.as_weak();
        let selected = selected_day.clone();
        window.on_navigate_day(move |offset| {
            if let Ok(mut selected_day) = selected.lock() {
                let today = Local::now().date_naive();
                let next = *selected_day + chrono::Duration::days(i64::from(offset));
                *selected_day = next.min(today);
                if let Some(window) = weak.upgrade() {
                    window.set_date_label(format_day(*selected_day).into());
                }
            }
        });
    }
    {
        let weak = window.as_weak();
        let active = focus.clone();
        let path = db_path.clone();
        window.on_toggle_focus(move || {
            let mut session = active.lock().expect("focus mutex poisoned");
            if let Some(session) = session.as_mut() {
                if let Some(deadline) = session.deadline.take() {
                    session.remaining = deadline.saturating_duration_since(Instant::now());
                } else {
                    session.deadline = Some(Instant::now() + session.remaining);
                }
            } else {
                let minutes = weak
                    .upgrade()
                    .map(|w| w.get_focus_minutes().clamp(5, 120))
                    .unwrap_or(25);
                let id = storage::Database::open(&path)
                    .and_then(|db| db.start_focus(i64::from(minutes)))
                    .unwrap_or(-1);
                *session = Some(FocusSession {
                    id,
                    deadline: Some(Instant::now() + Duration::from_secs(minutes as u64 * 60)),
                    remaining: Duration::from_secs(minutes as u64 * 60),
                });
            }
            if let Some(window) = weak.upgrade() {
                window.set_focus_active(session.is_some());
                window.set_focus_text(focus_message(session.as_ref()).into());
            }
        });
    }
    {
        let state = focus.clone();
        let path = db_path.clone();
        let weak = window.as_weak();
        window.on_stop_focus(move || {
            if let Ok(mut session) = state.lock() {
                if let Some(session) = session.take() {
                    if session.id > 0 {
                        if let Ok(db) = storage::Database::open(&path) {
                            let _ = db.finish_focus(session.id, "cancelled");
                        }
                    }
                }
                if let Some(w) = weak.upgrade() {
                    w.set_focus_active(false);
                    w.set_focus_text("No active focus session".into());
                }
            }
        });
    }
    {
        let weak = window.as_weak();
        window.on_navigate(move |page| {
            if let Some(w) = weak.upgrade() {
                w.set_page(page);
            }
        });
    }
    {
        let path = db_path.clone();
        window.on_export_data(move || {
            if let Some(out) = directories::UserDirs::new().map(|d| {
                d.download_dir()
                    .unwrap_or(d.home_dir())
                    .join("digital-wellbeing-usage.csv")
            }) {
                if let Ok(db) = storage::Database::open(&path) {
                    let _ = db.export_csv(&out);
                }
            }
        });
    }
    {
        let path = db_path.clone();
        let minutes = break_minutes.clone();
        window.on_set_break_minutes(move |value| {
            let value = value.clamp(0, 120) as u32;
            minutes.store(value, Ordering::Relaxed);
            if let Ok(db) = storage::Database::open(&path) {
                let _ = db.set_preference("break_reminder_minutes", &value.to_string());
            }
        });
    }
    {
        let path = db_path.clone();
        let weak = window.as_weak();
        window.on_change_app_limit(move |app_id| {
            if let Ok(db) = storage::Database::open(&path) {
                let current = db.app_limit(app_id.as_str()).ok().flatten().unwrap_or(0);
                let next = match current {
                    0 | 240.. => 30,
                    30 => 60,
                    60 => 120,
                    120 => 240,
                    _ => 30,
                };
                let _ = db.set_app_limit(app_id.as_str(), next);
                if let Some(w) = weak.upgrade() {
                    w.set_notice_text(
                        format!("Daily soft limit set to {next} minutes for {app_id}").into(),
                    );
                }
            }
        });
    }
    {
        let path = db_path.clone();
        let weak = window.as_weak();
        window.on_remove_app_limit(move |app_id| {
            if let Ok(db) = storage::Database::open(&path) {
                let _ = db.remove_app_limit(app_id.as_str());
            }
            if let Some(w) = weak.upgrade() {
                w.set_notice_text(format!("Daily limit removed for {app_id}").into());
            }
        });
    }
    {
        let sender = alert_tx.clone();
        window.on_test_notification(move || {
            let _ = sender.send((
                "Digital Wellbeing".into(),
                "Windows notification test succeeded.".into(),
            ));
        });
    }
    {
        let path = db_path.clone();
        let focus = focus.clone();
        let weak = window.as_weak();
        window.on_clear_history(move || {
            if let Ok(db) = storage::Database::open(&path) {
                let _ = db.clear_history();
            }
            if let Ok(mut active) = focus.lock() {
                *active = None;
            }
            if let Some(w) = weak.upgrade() {
                w.set_today_total("0m".into());
                w.set_focus_active(false);
                w.set_focus_text("No active focus session".into());
                w.set_notice_text("Usage history was deleted.".into());
            }
        });
    }
    {
        let path = db_path.clone();
        let weak = window.as_weak();
        window.on_set_goal(move |minutes| {
            if let Ok(db) = storage::Database::open(&path) {
                let goal = i64::from(minutes);
                let _ = db.set_goal_minutes(goal);
                if let Some(w) = weak.upgrade() {
                    w.set_goal_label(format!("{}h {:02}m", goal / 60, goal % 60).into());
                }
            }
        });
    }
    {
        let path = db_path.clone();
        let weak = window.as_weak();
        window.on_set_theme(move |mode| {
            let mode = if mode.as_str() == "dark" {
                "dark"
            } else if mode.as_str() == "light" {
                "light"
            } else {
                "system"
            };
            if let Ok(db) = storage::Database::open(&path) {
                let _ = db.set_preference("theme_mode", mode);
            }
            if let Some(w) = weak.upgrade() {
                w.set_theme_mode(mode.into());
                w.set_dark_theme(
                    mode == "dark" || (mode == "system" && platform::system_dark_theme()),
                );
            }
        });
    }

    let weak = window.as_weak();
    let refresh_path = db_path.clone();
    let focus_state = focus.clone();
    let day_state = selected_day.clone();
    let alert_rx = alert_rx;
    #[cfg(windows)]
    let tray_tracking = tracking.clone();
    #[cfg(windows)]
    let tray_for_tooltip = tray_instance.clone();
    #[cfg(windows)]
    let activation_timer = slint::Timer::default();
    #[cfg(windows)]
    {
        let weak = window.as_weak();
        let tray = tray_instance;
        let tracking = tracking.clone();
        let focus = focus.clone();
        let db_path = db_path.clone();
        activation_timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            let Some(w) = weak.upgrade() else {
                return;
            };
            if instance_guard.take_activation_request() {
                let _ = w.window().show();
            }
            let Some(tray) = tray.as_ref() else {
                return;
            };
            while let Some(action) = tray.next_action() {
                match action {
                    tray::Action::Open => {
                        let _ = w.window().show();
                    }
                    tray::Action::StartFocus => {
                        let mut active = focus.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(session) = active.as_mut() {
                            if session.deadline.is_none() {
                                session.deadline = Some(Instant::now() + session.remaining);
                            }
                        } else {
                            let minutes = 25_u64;
                            let id = storage::Database::open(&db_path)
                                .and_then(|db| db.start_focus(minutes as i64))
                                .unwrap_or(-1);
                            let remaining = Duration::from_secs(minutes * 60);
                            *active = Some(FocusSession {
                                id,
                                deadline: Some(Instant::now() + remaining),
                                remaining,
                            });
                        }
                        w.set_focus_active(true);
                        w.set_focus_text(focus_message(active.as_ref()).into());
                    }
                    tray::Action::ToggleTracking => {
                        let next = !tracking.load(Ordering::Relaxed);
                        tracking.store(next, Ordering::Relaxed);
                        w.set_tracking(next);
                        w.set_status_text(
                            if next {
                                "Tracking is active"
                            } else {
                                "Tracking is paused"
                            }
                            .into(),
                        );
                    }
                    tray::Action::Quit => {
                        let _ = slint::quit_event_loop();
                    }
                }
            }
        });
    }
    let timer = slint::Timer::default();
    if let Ok(goal) = db.goal_minutes() {
        window.set_goal_label(format!("{}h {:02}m", goal / 60, goal % 60).into());
        window.set_goal_minutes(goal.clamp(15, 720) as i32);
    }
    let theme_mode = db.preference("theme_mode", "system")?;
    window.set_theme_mode(theme_mode.clone().into());
    window.set_dark_theme(
        theme_mode == "dark" || (theme_mode == "system" && platform::system_dark_theme()),
    );
    if let Ok(count) = db.completed_focus_count() {
        window.set_focus_count(format!("{count} completed this week").into());
    }
    window.set_break_minutes(break_minutes.load(Ordering::Relaxed).min(120) as i32);
    let mut last_theme_check = Instant::now();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(w) = weak.upgrade() else {
            return;
        };
        if last_theme_check.elapsed() >= Duration::from_secs(15) {
            last_theme_check = Instant::now();
            if let Ok(db) = storage::Database::open(&refresh_path) {
                if let Ok(mode) = db.preference("theme_mode", "system") {
                    w.set_theme_mode(mode.clone().into());
                    w.set_dark_theme(
                        mode == "dark" || (mode == "system" && platform::system_dark_theme()),
                    );
                }
            }
        }
        while let Ok((title, body)) = alert_rx.try_recv() {
            match notifications::toast(&title, &body) {
                Ok(()) => w.set_notice_text(format!("Notification: {body}").into()),
                Err(error) => w.set_notice_text(format!("Notification failed: {error}").into()),
            }
        }
        let day = *day_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        w.set_date_label(format_day(day).into());
        if let Ok(db) = storage::Database::open(&refresh_path) {
            if let Ok((total, rows)) = db.summary_for_day(&day) {
                w.set_today_total(format_duration(total).into());
                #[cfg(windows)]
                if let Some(tray) = tray_for_tooltip.as_ref() {
                    let today_total = db
                        .summary_for_day(&Local::now().date_naive())
                        .map(|summary| summary.0)
                        .unwrap_or(total);
                    let state = if tray_tracking.load(Ordering::Relaxed) {
                        "tracking"
                    } else {
                        "paused"
                    };
                    tray.set_tooltip(&format!(
                        "Digital Wellbeing · {} today · {state}",
                        format_duration(today_total)
                    ));
                }
                let limits = db.app_limits().unwrap_or_default();
                let max = rows.iter().map(|r| r.1).max().unwrap_or(1).max(1);
                let items = rows
                    .into_iter()
                    .map(|(name, secs)| UsageItem {
                        app_id: name.to_lowercase().into(),
                        name: name.clone().into(),
                        duration: format_duration(secs).into(),
                        share: ((secs * 100 / max).clamp(3, 100)) as i32,
                        limit_minutes: limits.get(&name.to_lowercase()).copied().unwrap_or(0)
                            as i32,
                        limit_label: limits
                            .get(&name.to_lowercase())
                            .map(|limit| format!("Limit · {}m", limit))
                            .unwrap_or_else(|| "Set 30m".into())
                            .into(),
                    })
                    .collect::<Vec<_>>();
                w.set_usages(ModelRc::new(VecModel::from(items)));
            }
            if let Ok(hours) = db.hourly_summary(&day) {
                let max = hours.iter().copied().max().unwrap_or(1).max(1);
                let bars = hours
                    .into_iter()
                    .enumerate()
                    .map(|(hour, seconds)| ChartItem {
                        label: format!("{hour:02}").into(),
                        share: if seconds == 0 {
                            0
                        } else {
                            (seconds * 100 / max).clamp(2, 100) as i32
                        },
                        value: format_duration(seconds).into(),
                    })
                    .collect::<Vec<_>>();
                w.set_hourly_bars(ModelRc::new(VecModel::from(bars)));
            }
            if let Ok(days) = db.week_summary(&day) {
                let max = days
                    .iter()
                    .map(|(_, seconds)| *seconds)
                    .max()
                    .unwrap_or(1)
                    .max(1);
                let bars = days
                    .into_iter()
                    .map(|(date, seconds)| ChartItem {
                        label: date.format("%a").to_string().into(),
                        share: if seconds == 0 {
                            0
                        } else {
                            (seconds * 100 / max).clamp(2, 100) as i32
                        },
                        value: format_duration(seconds).into(),
                    })
                    .collect::<Vec<_>>();
                w.set_weekly_bars(ModelRc::new(VecModel::from(bars)));
            }
            if let Ok(count) = db.completed_focus_count() {
                w.set_focus_count(format!("{count} completed this week").into());
            }
        }
        if let Ok(mut f) = focus_state.lock() {
            let mut completed = None;
            if let Some(session) = f.as_mut() {
                if let Some(until) = session.deadline {
                    if Instant::now() >= until {
                        completed = Some(session.id);
                    } else {
                        session.remaining = until.saturating_duration_since(Instant::now());
                    }
                }
            }
            if let Some(id) = completed {
                if id > 0 {
                    if let Ok(db) = storage::Database::open(&refresh_path) {
                        let _ = db.finish_focus(id, "completed");
                    }
                }
                *f = None;
                let _ = alert_tx.send((
                    "Focus session complete".into(),
                    "Nice work. Take a short break before your next session.".into(),
                ));
                w.set_focus_active(false);
                w.set_focus_text("Focus session complete".into());
            } else {
                w.set_focus_active(f.is_some());
                w.set_focus_text(focus_message(f.as_ref()).into());
            }
        }
    });
    #[cfg(windows)]
    if !background_start || !tray_available {
        window.window().show()?;
    }
    #[cfg(not(windows))]
    window.window().show()?;
    #[cfg(windows)]
    let _startup_visibility_timer = if background_start && tray_available {
        let timer = slint::Timer::default();
        let weak = window.as_weak();
        timer.start(
            TimerMode::SingleShot,
            Duration::from_millis(500),
            move || {
                if let Some(window) = weak.upgrade() {
                    let _ = window.window().hide();
                    windows_app::hide_dashboard_window();
                }
            },
        );
        Some(timer)
    } else {
        None
    };
    slint::run_event_loop_until_quit()?;
    Ok(())
}

fn start_tracker(
    db_path: PathBuf,
    tracking: Arc<AtomicBool>,
    break_minutes: Arc<AtomicU32>,
    alerts: mpsc::Sender<(String, String)>,
) {
    thread::Builder::new()
        .name("usage-tracker".into())
        .spawn(move || {
            let mut sys = System::new();
            let db = storage::Database::open(&db_path).ok();
            let mut last = Instant::now();
            let mut continuous = Duration::ZERO;
            let mut break_threshold = None;
            let mut notified_apps = HashSet::<(NaiveDate, String, u8)>::new();
            let mut goal_notified = HashSet::<(NaiveDate,u8)>::new();
            loop {
                thread::sleep(Duration::from_secs(5));
                let now = Instant::now();
                // A long gap usually means suspend, sleep, or a stalled process. Do not
                // credit the gap as active foreground use after the machine wakes.
                let elapsed = now.duration_since(last);
                let seconds = if elapsed <= Duration::from_secs(10) {
                    elapsed.as_secs() as i64
                } else {
                    0
                };
                last = now;
                if seconds == 0 || !tracking.load(Ordering::Relaxed) {
                    continuous=Duration::ZERO;
                    continue;
                }
                if platform::idle_millis().is_none_or(|idle| idle > 60_000) {
                    continuous=Duration::ZERO;
                    continue;
                }
                let Some((pid, title)) = platform::foreground_window() else {
                    continuous = Duration::ZERO;
                    continue;
                };
                let target_pid = Pid::from_u32(pid);
                sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[target_pid]), true);
                let name = sys
                    .process(target_pid)
                    .map(|p| p.name().to_string_lossy().into_owned())
                    .unwrap_or_else(|| title.clone());
                let id = name.to_lowercase();
                if let Some(db) = db.as_ref() {
                    let now = Local::now();
                    let day=now.date_naive();
                    let before_app=db.usage_seconds(&day,&id).unwrap_or(0);
                    let before_total=db.summary_for_day(&day).map(|summary| summary.0).unwrap_or(0);
                    let _ = db.add_usage(&day, &id, &name, now.hour(), seconds);
                    let after_app=before_app+seconds;
                    if let Ok(Some(limit)) = db.app_limit(&id) {
                        for (threshold, mark, title) in [
                            (limit * 48, 0_u8, "App time limit approaching"),
                            (limit * 60, 1_u8, "App time limit reached"),
                        ] {
                            if before_app < threshold
                                && after_app >= threshold
                                && notified_apps.insert((day, id.clone(), mark))
                            {
                                let message = if mark == 0 {
                                    format!("{name} has used 80% of its {limit}-minute daily limit.")
                                } else {
                                    format!("{name} reached its {limit}-minute daily limit. The app is left open; you choose whether to stop.")
                                };
                                let _ = alerts.send((title.into(), message));
                            }
                        }
                    }
                    {
                        let goal=db.goal_minutes().unwrap_or(240)*60;
                        let after_total=before_total+seconds;
                        if before_total < goal && after_total >= goal && goal_notified.insert((day,1)) {
                            let _=alerts.send(("Daily screen-time goal reached".into(),format!("You reached your {}-minute daily target.",goal/60)));
                        } else if before_total < goal*4/5 && after_total >= goal*4/5 && goal_notified.insert((day,0)) {
                            let _=alerts.send(("Approaching daily goal".into(),format!("You have used 80% of your {}-minute daily target.",goal/60)));
                        }
                    }
                }
                continuous+=Duration::from_secs(seconds as u64);
                let threshold=break_minutes.load(Ordering::Relaxed);
                if threshold==0 { break_threshold=None; }
                else {
                    let due=Duration::from_secs(u64::from(threshold)*60);
                    if break_threshold != Some(due) { continuous=Duration::ZERO; break_threshold=Some(due); }
                    if continuous>=due { let _=alerts.send(("Time for a screen break".into(),format!("You have been active for {} minutes. Look away and take a short break.",threshold))); continuous=Duration::ZERO; }
                }
                let _ = title;
            }
        })
        .expect("failed to start usage tracker");
}

fn focus_message(session: Option<&FocusSession>) -> String {
    match session {
        None => "No active focus session".into(),
        Some(session) if session.deadline.is_none() => format!(
            "Paused · {} minutes remaining",
            session.remaining.as_secs().div_ceil(60)
        ),
        Some(session) => format!(
            "Focus session · {} minutes remaining",
            session.remaining.as_secs().div_ceil(60)
        ),
    }
}

fn format_day(day: NaiveDate) -> String {
    if day == Local::now().date_naive() {
        format!("Today · {}", day.format("%A, %B %d"))
    } else {
        day.format("%A, %B %d").to_string()
    }
}

fn format_duration(seconds: i64) -> String {
    if seconds >= 3600 {
        format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}s", seconds)
    }
}
