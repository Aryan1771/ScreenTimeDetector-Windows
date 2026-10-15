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
            CREATE TABLE IF NOT EXISTS website_usage (day TEXT NOT NULL, hour INTEGER NOT NULL CHECK(hour BETWEEN 0 AND 23), domain TEXT NOT NULL, active_seconds INTEGER NOT NULL DEFAULT 0 CHECK(active_seconds >= 0), PRIMARY KEY(day,hour,domain));\
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

    pub fn add_website_usage(
        &self,
        day: &NaiveDate,
        hour: u32,
        domain: &str,
        seconds: i64,
    ) -> rusqlite::Result<()> {
        if seconds <= 0 {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO website_usage(day,hour,domain,active_seconds) VALUES(?1,?2,?3,?4) ON CONFLICT(day,hour,domain) DO UPDATE SET active_seconds=active_seconds+excluded.active_seconds",
            params![day.to_string(), hour, domain, seconds],
        )?;
        Ok(())
    }

    pub fn website_summary_for_day(&self, day: &NaiveDate) -> rusqlite::Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT domain,SUM(active_seconds) FROM website_usage WHERE day=?1 GROUP BY domain ORDER BY SUM(active_seconds) DESC,domain COLLATE NOCASE ASC",
        )?;
        stmt.query_map([day.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect()
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
        self.period_summary(end_day, 7)
    }

    pub fn month_summary(&self, end_day: &NaiveDate) -> rusqlite::Result<Vec<(NaiveDate, i64)>> {
        self.period_summary(end_day, 30)
    }

    fn period_summary(
        &self,
        end_day: &NaiveDate,
        days: i64,
    ) -> rusqlite::Result<Vec<(NaiveDate, i64)>> {
        let start = *end_day - chrono::Duration::days(days - 1);
        let mut out = (0..days)
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

    pub fn weekly_comparison_percent(&self, end_day: &NaiveDate) -> rusqlite::Result<Option<i64>> {
        let current_start = *end_day - chrono::Duration::days(6);
        let previous_end = current_start - chrono::Duration::days(1);
        let previous_start = previous_end - chrono::Duration::days(6);
        let query = "SELECT COALESCE(SUM(active_seconds),0),COUNT(DISTINCT day) FROM daily_usage WHERE day BETWEEN ?1 AND ?2";
        let (current_total, current_days): (i64, i64) = self.conn.query_row(
            query,
            params![current_start.to_string(), end_day.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let (previous_total, previous_days): (i64, i64) = self.conn.query_row(
            query,
            params![previous_start.to_string(), previous_end.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if current_days < 3 || previous_days < 3 || previous_total == 0 {
            return Ok(None);
        }
        Ok(Some(
            (current_total - previous_total) * 100 / previous_total,
        ))
    }

    pub fn export_csv(&self, path: &Path) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        use std::io::Write;
        writeln!(file, "date,category,application_or_domain,active_seconds")?;
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
            writeln!(
                file,
                "{day},application,\"{}\",{seconds}",
                name.replace('"', "\"\"")
            )?;
        }
        let mut stmt = self.conn.prepare("SELECT day,domain,SUM(active_seconds) FROM website_usage GROUP BY day,domain ORDER BY day DESC,SUM(active_seconds) DESC,domain COLLATE NOCASE ASC").map_err(std::io::Error::other)?;
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
            let (day, domain, seconds) = row.map_err(std::io::Error::other)?;
            writeln!(
                file,
                "{day},website,\"{}\",{seconds}",
                domain.replace('"', "\"\"")
            )?;
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
            "DELETE FROM daily_usage; DELETE FROM hourly_usage; DELETE FROM website_usage; DELETE FROM focus_sessions; DELETE FROM applications WHERE id NOT IN (SELECT app_id FROM app_limits);",
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
    fn website_usage_aggregates_by_domain_without_url_data() {
        let db = memory_db();
        let day = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        db.add_website_usage(&day, 9, "example.com", 20).unwrap();
        db.add_website_usage(&day, 10, "example.com", 30).unwrap();
        db.add_website_usage(&day, 10, "docs.example.org", 30)
            .unwrap();
        assert_eq!(
            db.website_summary_for_day(&day).unwrap(),
            vec![("example.com".into(), 50), ("docs.example.org".into(), 30)]
        );
    }

    #[test]
    fn monthly_history_and_week_comparisons_require_recorded_data() {
        let db = memory_db();
        let end = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let month = db.month_summary(&end).unwrap();
        assert_eq!(month.len(), 30);
        assert_eq!(month.first().unwrap().0, end - chrono::Duration::days(29));
        assert_eq!(month.last().unwrap().0, end);
        assert_eq!(db.weekly_comparison_percent(&end).unwrap(), None);

        for offset in [1, 2, 3] {
            let day = end - chrono::Duration::days(7 + offset);
            db.add_usage(&day, "app.exe", "App", 12, 100).unwrap();
        }
        for offset in [1, 2, 3] {
            let day = end - chrono::Duration::days(offset);
            db.add_usage(&day, "app.exe", "App", 12, 150).unwrap();
        }
        assert_eq!(db.weekly_comparison_percent(&end).unwrap(), Some(50));
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
