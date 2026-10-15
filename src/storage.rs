use chrono::NaiveDate;
use rusqlite::{Connection, params};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS applications (id TEXT PRIMARY KEY, display_name TEXT NOT NULL);\
            CREATE TABLE IF NOT EXISTS daily_usage (day TEXT NOT NULL, app_id TEXT NOT NULL REFERENCES applications(id), active_seconds INTEGER NOT NULL DEFAULT 0 CHECK(active_seconds >= 0), PRIMARY KEY(day, app_id));\
            CREATE TABLE IF NOT EXISTS preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL);\
            CREATE TABLE IF NOT EXISTS hourly_usage (day TEXT NOT NULL, hour INTEGER NOT NULL CHECK(hour BETWEEN 0 AND 23), app_id TEXT NOT NULL REFERENCES applications(id), active_seconds INTEGER NOT NULL DEFAULT 0 CHECK(active_seconds >= 0), PRIMARY KEY(day,hour,app_id));\
            CREATE TABLE IF NOT EXISTS app_limits (app_id TEXT PRIMARY KEY REFERENCES applications(id), daily_minutes INTEGER NOT NULL CHECK(daily_minutes BETWEEN 5 AND 1440));\
            CREATE TABLE IF NOT EXISTS focus_sessions (id INTEGER PRIMARY KEY, started_at TEXT NOT NULL, ended_at TEXT, status TEXT NOT NULL, planned_minutes INTEGER NOT NULL);\
            CREATE INDEX IF NOT EXISTS idx_daily_usage_day ON daily_usage(day);")?;
        Ok(Self { conn })
    }

    pub fn add_usage(
        &self,
        day: &NaiveDate,
        id: &str,
        name: &str,
        hour: u32,
        seconds: i64,
    ) -> rusqlite::Result<()> {
        if seconds <= 0 {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("INSERT INTO applications(id,display_name) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name", params![id, name])?;
        tx.execute("INSERT INTO daily_usage(day,app_id,active_seconds) VALUES(?1,?2,?3) ON CONFLICT(day,app_id) DO UPDATE SET active_seconds=active_seconds+excluded.active_seconds", params![day.to_string(), id, seconds])?;
        tx.execute("INSERT INTO hourly_usage(day,hour,app_id,active_seconds) VALUES(?1,?2,?3,?4) ON CONFLICT(day,hour,app_id) DO UPDATE SET active_seconds=active_seconds+excluded.active_seconds", params![day.to_string(), hour, id, seconds])?;
        tx.commit()
    }

    pub fn summary_for_day(
        &self,
        selected_day: &NaiveDate,
    ) -> rusqlite::Result<(i64, Vec<(String, i64)>)> {
        let mut stmt = self.conn.prepare("SELECT a.display_name,u.active_seconds FROM daily_usage u JOIN applications a ON a.id=u.app_id WHERE u.day=?1 ORDER BY u.active_seconds DESC,a.display_name COLLATE NOCASE ASC")?;
        let rows = stmt
            .query_map([selected_day.to_string()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((rows.iter().map(|r| r.1).sum(), rows))
    }

    pub fn hourly_summary(&self, day: &NaiveDate) -> rusqlite::Result<Vec<i64>> {
        let mut hours = vec![0_i64; 24];
        let mut stmt = self.conn.prepare("SELECT hour,SUM(active_seconds) FROM hourly_usage WHERE day=?1 GROUP BY hour ORDER BY hour")?;
        let rows = stmt.query_map([day.to_string()], |r| {
            Ok((r.get::<_, i32>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (hour, seconds) = row?;
            if (0..24).contains(&hour) {
                hours[hour as usize] = seconds;
            }
        }
        Ok(hours)
    }

    pub fn week_summary(&self, end_day: &NaiveDate) -> rusqlite::Result<Vec<(NaiveDate, i64)>> {
        let start = *end_day - chrono::Duration::days(6);
        let mut out = (0..7)
            .map(|offset| (start + chrono::Duration::days(offset), 0_i64))
            .collect::<Vec<_>>();
        let mut stmt = self.conn.prepare("SELECT day,SUM(active_seconds) FROM daily_usage WHERE day BETWEEN ?1 AND ?2 GROUP BY day")?;
        let rows = stmt.query_map(params![start.to_string(), end_day.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (day, seconds) = row?;
            if let Ok(day) = NaiveDate::parse_from_str(&day, "%Y-%m-%d") {
                if let Some(item) = out.iter_mut().find(|item| item.0 == day) {
                    item.1 = seconds;
                }
            }
        }
        Ok(out)
    }

    pub fn export_csv(&self, path: &Path) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        use std::io::Write;
        writeln!(file, "date,application,active_seconds")?;
        let mut stmt = self.conn.prepare("SELECT u.day,a.display_name,u.active_seconds FROM daily_usage u JOIN applications a ON a.id=u.app_id ORDER BY u.day DESC,u.active_seconds DESC").map_err(std::io::Error::other)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(std::io::Error::other)?;
        for row in rows {
            let (day, name, seconds) = row.map_err(std::io::Error::other)?;
            writeln!(file, "{day},\"{}\",{seconds}", name.replace('"', "\"\""))?;
        }
        Ok(())
    }

    pub fn goal_minutes(&self) -> rusqlite::Result<i64> {
        match self.conn.query_row(
            "SELECT CAST(value AS INTEGER) FROM preferences WHERE key='daily_goal_minutes'",
            [],
            |r| r.get(0),
        ) {
            Ok(value) => Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(240),
            Err(error) => Err(error),
        }
    }

    pub fn set_goal_minutes(&self, minutes: i64) -> rusqlite::Result<()> {
        self.conn.execute("INSERT INTO preferences(key,value) VALUES('daily_goal_minutes',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [minutes.clamp(15, 720).to_string()])?;
        Ok(())
    }

    pub fn set_preference(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute("INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
        Ok(())
    }

    pub fn preference(&self, key: &str, default: &str) -> rusqlite::Result<String> {
        match self
            .conn
            .query_row("SELECT value FROM preferences WHERE key=?1", [key], |r| {
                r.get(0)
            }) {
            Ok(value) => Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(default.to_string()),
            Err(error) => Err(error),
        }
    }

    pub fn app_limits(&self) -> rusqlite::Result<std::collections::HashMap<String, i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT app_id,daily_minutes FROM app_limits")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        rows.collect()
    }

    pub fn app_limit(&self, app_id: &str) -> rusqlite::Result<Option<i64>> {
        match self.conn.query_row(
            "SELECT daily_minutes FROM app_limits WHERE app_id=?1",
            [app_id],
            |r| r.get(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn set_app_limit(&self, app_id: &str, minutes: i64) -> rusqlite::Result<()> {
        self.conn.execute("INSERT INTO app_limits(app_id,daily_minutes) VALUES(?1,?2) ON CONFLICT(app_id) DO UPDATE SET daily_minutes=excluded.daily_minutes", params![app_id,minutes.clamp(5,1440)])?;
        Ok(())
    }

    pub fn remove_app_limit(&self, app_id: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM app_limits WHERE app_id=?1", [app_id])?;
        Ok(())
    }

    pub fn usage_seconds(&self, day: &NaiveDate, app_id: &str) -> rusqlite::Result<i64> {
        match self.conn.query_row(
            "SELECT active_seconds FROM daily_usage WHERE day=?1 AND app_id=?2",
            params![day.to_string(), app_id],
            |r| r.get(0),
        ) {
            Ok(value) => Ok(value),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
            Err(error) => Err(error),
        }
    }

    pub fn start_focus(&self, planned_minutes: i64) -> rusqlite::Result<i64> {
        self.conn.execute(
            "UPDATE focus_sessions SET status='interrupted',ended_at=?1 WHERE status='active'",
            [chrono::Local::now().to_rfc3339()],
        )?;
        self.conn.execute(
            "INSERT INTO focus_sessions(started_at,status,planned_minutes) VALUES(?1,'active',?2)",
            params![chrono::Local::now().to_rfc3339(), planned_minutes],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn finish_focus(&self, id: i64, status: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE focus_sessions SET ended_at=?1,status=?2 WHERE id=?3 AND status='active'",
            params![chrono::Local::now().to_rfc3339(), status, id],
        )?;
        Ok(())
    }

    pub fn completed_focus_count(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM focus_sessions WHERE status='completed' AND date(started_at,'localtime') >= date('now','localtime','-6 day')", [], |r| r.get(0))
    }

    pub fn clear_history(&self) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(
            "DELETE FROM daily_usage; DELETE FROM hourly_usage; DELETE FROM focus_sessions; DELETE FROM applications WHERE id NOT IN (SELECT app_id FROM app_limits);",
        )?;
        tx.commit()
    }

    pub fn recover_focus_sessions(&self) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE focus_sessions SET status='interrupted',ended_at=?1 WHERE status='active'",
            [chrono::Local::now().to_rfc3339()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Database {
        Database::open(Path::new(":memory:")).expect("initialize in-memory SQLite database")
    }

    #[test]
    fn usage_updates_daily_and_hourly_totals_and_sorts_ties_by_name() {
        let db = memory_db();
        let day = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        db.add_usage(&day, "z.exe", "Zulu", 9, 30).unwrap();
        db.add_usage(&day, "a.exe", "Alpha", 9, 30).unwrap();
        db.add_usage(&day, "a.exe", "Alpha", 10, 15).unwrap();

        let (total, apps) = db.summary_for_day(&day).unwrap();
        assert_eq!(total, 75);
        assert_eq!(apps, vec![("Alpha".into(), 45), ("Zulu".into(), 30)]);
        let hours = db.hourly_summary(&day).unwrap();
        assert_eq!(hours[9], 60);
        assert_eq!(hours[10], 15);
        assert_eq!(hours.iter().sum::<i64>(), total);
    }

    #[test]
    fn limits_preferences_and_history_deletion_have_expected_scope() {
        let db = memory_db();
        let day = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        db.add_usage(&day, "editor.exe", "Editor", 12, 120).unwrap();
        db.set_app_limit("editor.exe", 2).unwrap();
        assert_eq!(db.app_limit("editor.exe").unwrap(), Some(5));
        db.set_preference("theme_mode", "dark").unwrap();
        assert_eq!(db.preference("theme_mode", "system").unwrap(), "dark");

        db.clear_history().unwrap();
        assert_eq!(db.summary_for_day(&day).unwrap().0, 0);
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM applications", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1,
            "only the application still referenced by its retained limit remains"
        );
        assert_eq!(db.app_limit("editor.exe").unwrap(), Some(5));
        assert_eq!(db.preference("theme_mode", "system").unwrap(), "dark");
    }

    #[test]
    fn focus_sessions_transition_once_and_recover_after_restart() {
        let db = memory_db();
        let id = db.start_focus(25).unwrap();
        db.finish_focus(id, "completed").unwrap();
        db.finish_focus(id, "cancelled").unwrap();
        assert_eq!(db.completed_focus_count().unwrap(), 1);

        let interrupted = db.start_focus(30).unwrap();
        db.recover_focus_sessions().unwrap();
        assert_eq!(
            db.conn
                .query_row(
                    "SELECT status FROM focus_sessions WHERE id=?1",
                    [interrupted],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "interrupted"
        );
    }
}
