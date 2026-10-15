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
            CREATE INDEX IF NOT EXISTS idx_daily_usage_day ON daily_usage(day);")?;
        Ok(Self { conn })
    }

    pub fn add_usage(
        &self,
        day: &NaiveDate,
        id: &str,
        name: &str,
        seconds: i64,
    ) -> rusqlite::Result<()> {
        if seconds <= 0 {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("INSERT INTO applications(id,display_name) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name", params![id, name])?;
        tx.execute("INSERT INTO daily_usage(day,app_id,active_seconds) VALUES(?1,?2,?3) ON CONFLICT(day,app_id) DO UPDATE SET active_seconds=active_seconds+excluded.active_seconds", params![day.to_string(), id, seconds])?;
        tx.commit()
    }

    pub fn today_summary(&self) -> rusqlite::Result<(i64, Vec<(String, i64)>)> {
        let day = chrono::Local::now().date_naive().to_string();
        let mut stmt = self.conn.prepare("SELECT a.display_name,u.active_seconds FROM daily_usage u JOIN applications a ON a.id=u.app_id WHERE u.day=?1 ORDER BY u.active_seconds DESC,a.display_name COLLATE NOCASE ASC")?;
        let rows = stmt
            .query_map([day], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((rows.iter().map(|r| r.1).sum(), rows))
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
}
