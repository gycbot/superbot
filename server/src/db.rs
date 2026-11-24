//! SQLite storage: Bots, messages, routines. One connection behind a mutex;
//! every call is short.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;

#[derive(Clone, Serialize)]
pub struct Bot {
    pub id: String,
    pub name: String,
    pub color: String,
    pub shape: String,
    pub harness: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    #[serde(skip)]
    pub session_id: Option<String>,
    pub primary: bool,
    pub hidden: bool,
    pub unread: bool,
    pub created_at: i64,
}

#[derive(Clone, Serialize)]
pub struct Message {
    pub id: i64,
    #[serde(rename = "botId")]
    pub bot_id: String,
    /// "user", "bot", or "routine"
    pub role: String,
    pub text: String,
    /// { title, options, picked? }
    pub question: Option<Value>,
    pub error: bool,
    /// "queued" | "running" | "done". Kept for the messages table; runs own the
    /// real state now.
    #[serde(skip)]
    #[allow(dead_code)]
    pub status: Option<String>,
    /// For "peer" and "reply" messages: the other Bot's id.
    #[serde(rename = "fromBot")]
    pub from_bot: Option<String>,
    /// Attached images/videos/files: [{ id, name, mime, kind }]
    pub attachments: Option<Value>,
    #[serde(rename = "at")]
    pub created_at: i64,
}

#[derive(Clone, Serialize)]
pub struct Routine {
    pub id: String,
    pub bot_id: String,
    pub name: String,
    pub cron: String,
    pub instruction: String,
    pub enabled: bool,
    pub last_run_at: Option<i64>,
    pub created_at: i64,
}

/// One durable unit of work. A run is queued, claimed by the Bot's worker
/// loop, and finished with exactly one outcome. The row outlives the process.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct Run {
    pub id: i64,
    pub bot_id: String,
    /// "chat" today; event work will use "<source>:<subject>".
    pub thread: String,
    /// "user" | "peer" | "reply" | "routine"
    pub kind: String,
    pub message_id: Option<i64>,
    #[serde(skip)]
    pub from_bot: Option<String>,
    /// The prompt as it was when the run was queued, so a retry repeats the same task.
    #[serde(skip)]
    pub prompt: String,
    /// queued | claimed | done | failed | cancelled
    pub state: String,
    pub attempts: i64,
    #[serde(skip)]
    pub not_before: i64,
    /// Session to resume; set at claim time from the Bot, then kept across retries.
    #[serde(skip)]
    pub session: Option<String>,
    /// The harness accepted the prompt at least once.
    #[serde(skip)]
    pub started: bool,
    pub replies: i64,
    pub error: Option<String>,
    pub finished_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFact {
    pub id: i64,
    pub run_id: i64,
    pub at: i64,
    pub kind: String,
    pub detail: String,
}

/// What to insert when a run is queued.
pub struct NewRun {
    pub bot_id: String,
    pub thread: String,
    pub kind: String,
    pub message_id: Option<i64>,
    pub from_bot: Option<String>,
    pub prompt: String,
}

#[derive(Default)]
pub struct BotPatch {
    pub name: Option<String>,
    pub color: Option<String>,
    pub shape: Option<String>,
    pub harness: Option<String>,
    pub model: Option<Option<String>>,
    pub effort: Option<Option<String>>,
    pub session_id: Option<Option<String>>,
    pub hidden: Option<bool>,
    pub unread: Option<bool>,
}

pub struct Db(Mutex<Connection>);

pub fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn bot_row(r: &Row) -> rusqlite::Result<Bot> {
    Ok(Bot {
        id: r.get("id")?,
        name: r.get("name")?,
        color: r.get("color")?,
        shape: r.get("shape")?,
        harness: r.get("harness")?,
        model: r.get("model")?,
        effort: r.get("effort")?,
        session_id: r.get("session_id")?,
        primary: r.get("primary_bot")?,
        hidden: r.get("hidden")?,
        unread: r.get("unread")?,
        created_at: r.get("created_at")?,
    })
}

fn message_row(r: &Row) -> rusqlite::Result<Message> {
    let q: Option<String> = r.get("question")?;
    let a: Option<String> = r.get("attachments").unwrap_or(None);
    Ok(Message {
        id: r.get("id")?,
        bot_id: r.get("bot_id")?,
        role: r.get("role")?,
        text: r.get("text")?,
        question: q.and_then(|s| serde_json::from_str(&s).ok()),
        error: r.get("error")?,
        status: r.get("status")?,
        from_bot: r.get("from_bot")?,
        attachments: a.and_then(|s| serde_json::from_str(&s).ok()),
        created_at: r.get("created_at")?,
    })
}

fn run_row(r: &Row) -> rusqlite::Result<Run> {
    Ok(Run {
        id: r.get("id")?,
        bot_id: r.get("bot_id")?,
        thread: r.get("thread")?,
        kind: r.get("kind")?,
        message_id: r.get("message_id")?,
        from_bot: r.get("from_bot")?,
        prompt: r.get("prompt")?,
        state: r.get("state")?,
        attempts: r.get("attempts")?,
        not_before: r.get("not_before")?,
        session: r.get("session")?,
        started: r.get::<_, i64>("started")? != 0,
        replies: r.get("replies")?,
        error: r.get("error")?,
        finished_at: r.get("finished_at")?,
        created_at: r.get("created_at")?,
    })
}

fn fact_row(r: &Row) -> rusqlite::Result<RunFact> {
    Ok(RunFact { id: r.get("id")?, run_id: r.get("run_id")?, at: r.get("at")?, kind: r.get("kind")?, detail: r.get("detail")? })
}

/// Schema changes after the first release, tracked with PRAGMA user_version.
const MIGRATIONS: &[&str] = &[
    // 1: durable runs and their timeline. The queue used to live in memory,
    // so a restart lost whatever was in flight.
    "CREATE TABLE IF NOT EXISTS runs (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       bot_id TEXT NOT NULL,
       thread TEXT NOT NULL DEFAULT 'chat',
       kind TEXT NOT NULL,
       message_id INTEGER,
       from_bot TEXT,
       prompt TEXT NOT NULL,
       state TEXT NOT NULL DEFAULT 'queued',
       attempts INTEGER NOT NULL DEFAULT 0,
       not_before INTEGER NOT NULL DEFAULT 0,
       session TEXT,
       started INTEGER NOT NULL DEFAULT 0,
       replies INTEGER NOT NULL DEFAULT 0,
       receipt TEXT,
       error TEXT,
       claimed_at INTEGER,
       finished_at INTEGER,
       created_at INTEGER NOT NULL
     );
     CREATE INDEX IF NOT EXISTS runs_todo ON runs(state, not_before, id);
     CREATE INDEX IF NOT EXISTS runs_bot ON runs(bot_id, id);
     CREATE TABLE IF NOT EXISTS run_facts (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       run_id INTEGER NOT NULL,
       at INTEGER NOT NULL,
       kind TEXT NOT NULL,
       detail TEXT NOT NULL DEFAULT ''
     );
     CREATE INDEX IF NOT EXISTS run_facts_run ON run_facts(run_id, id);",
];

fn migrate(c: &Connection) -> rusqlite::Result<()> {
    let mut version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    while (version as usize) < MIGRATIONS.len() {
        c.execute_batch(MIGRATIONS[version as usize])?;
        version += 1;
        c.pragma_update(None, "user_version", version)?;
    }
    Ok(())
}

fn routine_row(r: &Row) -> rusqlite::Result<Routine> {
    Ok(Routine {
        id: r.get("id")?,
        bot_id: r.get("bot_id")?,
        name: r.get("name")?,
        cron: r.get("cron")?,
        instruction: r.get("instruction")?,
        enabled: r.get("enabled")?,
        last_run_at: r.get("last_run_at")?,
        created_at: r.get("created_at")?,
    })
}

impl Db {
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Db> {
        let c = Connection::open(path)?;
        c.execute_batch(
            "PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS bots (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              color TEXT NOT NULL,
              shape TEXT NOT NULL,
              harness TEXT NOT NULL,
              model TEXT,
              session_id TEXT,
              primary_bot INTEGER NOT NULL DEFAULT 0,
              hidden INTEGER NOT NULL DEFAULT 0,
              unread INTEGER NOT NULL DEFAULT 0,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
              role TEXT NOT NULL,
              text TEXT NOT NULL,
              question TEXT,
              error INTEGER NOT NULL DEFAULT 0,
              status TEXT,
              attachments TEXT,
              created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS messages_bot ON messages(bot_id, id);
            CREATE TABLE IF NOT EXISTS routines (
              id TEXT PRIMARY KEY,
              bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
              name TEXT NOT NULL,
              cron TEXT NOT NULL,
              instruction TEXT NOT NULL,
              enabled INTEGER NOT NULL DEFAULT 1,
              last_run_at INTEGER,
              created_at INTEGER NOT NULL
            );",
        )?;
        // Added after the first release; fails harmlessly if it already exists.
        let _ = c.execute("ALTER TABLE messages ADD COLUMN from_bot TEXT", []);
        let _ = c.execute("ALTER TABLE messages ADD COLUMN attachments TEXT", []);
        let _ = c.execute("ALTER TABLE bots ADD COLUMN effort TEXT", []);
        migrate(&c)?;
        Ok(Db(Mutex::new(c)))
    }

    fn c(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    // Bots

    pub fn bots(&self) -> Vec<Bot> {
        let c = self.c();
        let mut s = c.prepare("SELECT * FROM bots ORDER BY primary_bot DESC, created_at").unwrap();
        s.query_map([], bot_row).unwrap().filter_map(Result::ok).collect()
    }

    pub fn bot(&self, id: &str) -> Option<Bot> {
        self.c().query_row("SELECT * FROM bots WHERE id = ?", [id], bot_row).optional().ok().flatten()
    }

    pub fn create_bot(&self, name: &str, color: &str, shape: &str, harness: &str, model: Option<&str>, primary: bool) -> Bot {
        let id = crate::short_id();
        self.c()
            .execute(
                "INSERT INTO bots (id, name, color, shape, harness, model, primary_bot, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![id, name, color, shape, harness, model, primary, now()],
            )
            .unwrap();
        self.bot(&id).unwrap()
    }

    pub fn update_bot(&self, id: &str, p: BotPatch) {
        let c = self.c();
        let set = |col: &str, v: &dyn rusqlite::ToSql| {
            c.execute(&format!("UPDATE bots SET {col} = ? WHERE id = ?"), params![v, id]).unwrap();
        };
        if let Some(v) = p.name { set("name", &v) }
        if let Some(v) = p.color { set("color", &v) }
        if let Some(v) = p.shape { set("shape", &v) }
        if let Some(v) = p.harness { set("harness", &v) }
        if let Some(v) = p.model { set("model", &v) }
        if let Some(v) = p.effort { set("effort", &v) }
        if let Some(v) = p.session_id { set("session_id", &v) }
        if let Some(v) = p.hidden { set("hidden", &v) }
        if let Some(v) = p.unread { set("unread", &v) }
    }

    pub fn delete_bot(&self, id: &str) {
        self.c().execute("DELETE FROM bots WHERE id = ?", [id]).unwrap();
    }

    // Messages

    pub fn messages(&self, bot_id: &str, limit: i64) -> Vec<Message> {
        let c = self.c();
        let mut s = c
            .prepare("SELECT * FROM (SELECT * FROM messages WHERE bot_id = ? ORDER BY id DESC LIMIT ?) ORDER BY id")
            .unwrap();
        s.query_map(params![bot_id, limit], message_row).unwrap().filter_map(Result::ok).collect()
    }

    pub fn last_message(&self, bot_id: &str) -> Option<Message> {
        self.c()
            .query_row(
                "SELECT * FROM messages WHERE bot_id = ? AND role != 'routine' ORDER BY id DESC LIMIT 1",
                [bot_id],
                message_row,
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn message(&self, id: i64) -> Option<Message> {
        self.c().query_row("SELECT * FROM messages WHERE id = ?", [id], message_row).optional().ok().flatten()
    }

    pub fn add_message(&self, bot_id: &str, role: &str, text: &str, question: Option<&Value>, error: bool, status: Option<&str>) -> Message {
        self.add_message_from(bot_id, role, text, question, error, status, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_message_from(&self, bot_id: &str, role: &str, text: &str, question: Option<&Value>, error: bool, status: Option<&str>, from_bot: Option<&str>) -> Message {
        self.add_message_full(bot_id, role, text, question, error, status, from_bot, None)
    }

    /// Full insert including attachments (images/videos/files).
    #[allow(clippy::too_many_arguments)]
    pub fn add_message_full(&self, bot_id: &str, role: &str, text: &str, question: Option<&Value>, error: bool, status: Option<&str>, from_bot: Option<&str>, attachments: Option<&Value>) -> Message {
        let c = self.c();
        c.execute(
            "INSERT INTO messages (bot_id, role, text, question, error, status, from_bot, attachments, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![bot_id, role, text, question.map(|q| q.to_string()), error, status, from_bot, attachments.map(|a| a.to_string()), now()],
        )
        .unwrap();
        let id = c.last_insert_rowid();
        drop(c);
        self.message(id).unwrap()
    }

    pub fn last_id(&self) -> i64 {
        self.c().query_row("SELECT COALESCE(MAX(id), 0) FROM messages", [], |r| r.get(0)).unwrap_or(0)
    }

    /// The Bot's chat bubbles written after message `after`.
    pub fn replies_after(&self, bot_id: &str, after: i64) -> Vec<Message> {
        let c = self.c();
        let mut s = c.prepare("SELECT * FROM messages WHERE bot_id = ? AND id > ? AND role = 'bot' ORDER BY id").unwrap();
        s.query_map(params![bot_id, after], message_row).unwrap().filter_map(Result::ok).collect()
    }

    pub fn set_status(&self, id: i64, status: &str) {
        self.c().execute("UPDATE messages SET status = ? WHERE id = ?", params![status, id]).unwrap();
    }

    pub fn set_question(&self, id: i64, q: &Value) {
        self.c().execute("UPDATE messages SET question = ? WHERE id = ?", params![q.to_string(), id]).unwrap();
    }

    // Routines

    pub fn routines(&self, bot_id: Option<&str>) -> Vec<Routine> {
        let c = self.c();
        let mut s = c
            .prepare("SELECT * FROM routines WHERE ?1 IS NULL OR bot_id = ?1 ORDER BY created_at")
            .unwrap();
        s.query_map([bot_id], routine_row).unwrap().filter_map(Result::ok).collect()
    }

    pub fn routine(&self, id: &str) -> Option<Routine> {
        self.c().query_row("SELECT * FROM routines WHERE id = ?", [id], routine_row).optional().ok().flatten()
    }

    pub fn add_routine(&self, bot_id: &str, name: &str, cron: &str, instruction: &str) -> Routine {
        let id = crate::short_id();
        self.c()
            .execute(
                "INSERT INTO routines (id, bot_id, name, cron, instruction, created_at) VALUES (?, ?, ?, ?, ?, ?)",
                params![id, bot_id, name, cron, instruction, now()],
            )
            .unwrap();
        self.routine(&id).unwrap()
    }

    pub fn update_routine(&self, id: &str, name: Option<&str>, cron: Option<&str>, instruction: Option<&str>, enabled: Option<bool>) {
        let c = self.c();
        let set = |col: &str, v: &dyn rusqlite::ToSql| {
            c.execute(&format!("UPDATE routines SET {col} = ? WHERE id = ?"), params![v, id]).unwrap();
        };
        if let Some(v) = name { set("name", &v) }
        if let Some(v) = cron { set("cron", &v) }
        if let Some(v) = instruction { set("instruction", &v) }
        if let Some(v) = enabled { set("enabled", &v) }
    }

    pub fn routine_ran(&self, id: &str, at: i64) {
        self.c().execute("UPDATE routines SET last_run_at = ? WHERE id = ?", params![at, id]).unwrap();
    }

    pub fn delete_routine(&self, id: &str) {
        self.c().execute("DELETE FROM routines WHERE id = ?", [id]).unwrap();
    }

    // Runs

    /// Messages that were waiting when the server ran the version before durable
    /// runs existed (no run row points at them). They are queued once, on the
    /// first start after the upgrade.
    pub fn orphan_messages(&self) -> Vec<Message> {
        let c = self.c();
        let mut s = c
            .prepare(
                "SELECT * FROM messages m
                 WHERE m.status IN ('queued', 'running')
                   AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.message_id = m.id)
                 ORDER BY m.id",
            )
            .unwrap();
        s.query_map([], message_row).unwrap().filter_map(Result::ok).collect()
    }

    /// Queues work. Returns the run id.
    pub fn enqueue_run(&self, r: NewRun) -> i64 {
        let c = self.c();
        c.execute(
            "INSERT INTO runs (bot_id, thread, kind, message_id, from_bot, prompt, state, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 'queued', ?)",
            params![r.bot_id, r.thread, r.kind, r.message_id, r.from_bot, r.prompt, now()],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    /// Claims the oldest due run for one Bot, unless it already has one running.
    /// A single statement, so two loops cannot claim the same row.
    pub fn claim_for(&self, bot_id: &str, at: i64) -> Option<Run> {
        self.c()
            .query_row(
                "UPDATE runs SET state = 'claimed', claimed_at = ?2, attempts = attempts + 1,
                    session = COALESCE(session, (SELECT session_id FROM bots WHERE bots.id = runs.bot_id))
                 WHERE id = (SELECT r.id FROM runs r
                             WHERE r.bot_id = ?1 AND r.state = 'queued' AND r.not_before <= ?2
                               AND NOT EXISTS (SELECT 1 FROM runs c WHERE c.bot_id = r.bot_id AND c.state = 'claimed')
                             ORDER BY r.id LIMIT 1)
                 RETURNING *",
                params![bot_id, at],
                run_row,
            )
            .optional()
            .ok()
            .flatten()
    }

    /// Bots with queued work that is due now. Drives the scheduler loop.
    pub fn due_bots(&self, at: i64) -> Vec<String> {
        let c = self.c();
        let mut s = c
            .prepare(
                "SELECT DISTINCT bot_id FROM runs
                 WHERE state = 'queued' AND not_before <= ?1
                   AND EXISTS (SELECT 1 FROM bots b WHERE b.id = runs.bot_id)",
            )
            .unwrap();
        s.query_map([at], |r| r.get::<_, String>(0)).unwrap().filter_map(Result::ok).collect()
    }

    /// The harness accepted the prompt and named a session. From here a crash is
    /// a retry in that session, not a fresh start.
    pub fn started_run(&self, id: i64, session: &str) {
        let _ = self.c().execute("UPDATE runs SET started = 1, session = ?2 WHERE id = ?1", params![id, session]);
    }

    /// A finished run also finishes its message, so the UI shows it as handled.
    pub fn finish_run(&self, id: i64, state: &str, replies: i64, error: Option<&str>) {
        let c = self.c();
        let _ = c.execute(
            "UPDATE runs SET state = ?2, replies = ?3, error = ?4, finished_at = ?5 WHERE id = ?1",
            params![id, state, replies, error, now()],
        );
        let _ = c.execute(
            "UPDATE messages SET status = 'done' WHERE id = (SELECT message_id FROM runs WHERE id = ?1)",
            [id],
        );
    }

    /// Puts a run back in the queue after a failure, keeping its session.
    pub fn requeue_run(&self, id: i64, at: i64, error: &str) {
        let _ = self.c().execute(
            "UPDATE runs SET state = 'queued', not_before = ?2, error = ?3, claimed_at = NULL WHERE id = ?1",
            params![id, at, error],
        );
    }

    /// After a restart, anything still claimed has no process behind it. A run that
    /// got started resumes its session; one that never did starts clean. Returns
    /// (run id, whether it had started).
    pub fn sweep(&self, at: i64) -> Vec<(i64, bool)> {
        let c = self.c();
        let rows: Vec<(i64, bool)> = {
            let mut s = c
                .prepare("SELECT id, started FROM runs WHERE state = 'claimed'")
                .unwrap();
            s.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0))).unwrap().filter_map(Result::ok).collect()
        };
        c
            .execute(
                "UPDATE runs SET state = 'queued', claimed_at = NULL, not_before = ?1,
                    session = CASE WHEN started = 1 THEN session ELSE NULL END
                 WHERE state = 'claimed'",
                [at],
            )
            .unwrap_or(0);
        let _ = c.execute(
            "UPDATE runs SET state = 'cancelled', finished_at = ?1, error = 'Bot was deleted'
             WHERE state IN ('queued', 'claimed') AND bot_id NOT IN (SELECT id FROM bots)",
            [at],
        );
        rows
    }

    /// Claims older than the cutoff belong to a task that died without finishing.
    pub fn sweep_stale(&self, at: i64, cutoff: i64) -> Vec<(i64, bool)> {
        let c = self.c();
        let rows: Vec<(i64, bool)> = {
            let mut s = c
                .prepare("SELECT id, started FROM runs WHERE state = 'claimed' AND claimed_at IS NOT NULL AND claimed_at < ?1")
                .unwrap();
            s.query_map([cutoff], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0))).unwrap().filter_map(Result::ok).collect()
        };
        c.execute(
            "UPDATE runs SET state = 'queued', claimed_at = NULL, not_before = ?1,
                session = CASE WHEN started = 1 THEN session ELSE NULL END
             WHERE state = 'claimed' AND claimed_at IS NOT NULL AND claimed_at < ?2",
            params![at, cutoff],
        )
        .unwrap_or(0);
        rows
    }

    /// Drops queued work for a Bot (the user pressed Stop).
    pub fn cancel_queued(&self, bot_id: &str) -> usize {
        let c = self.c();
        let n = c
            .execute(
                "UPDATE runs SET state = 'cancelled', finished_at = ?2, error = 'Stopped by the user'
                 WHERE bot_id = ?1 AND state = 'queued'",
                params![bot_id, now()],
            )
            .unwrap_or(0);
        let _ = c.execute(
            "UPDATE messages SET status = 'done' WHERE id IN
               (SELECT message_id FROM runs WHERE bot_id = ?1 AND state = 'cancelled' AND message_id IS NOT NULL)",
            [bot_id],
        );
        n
    }

    pub fn runs(&self, bot_id: &str, limit: i64) -> Vec<Run> {
        let c = self.c();
        let mut s = c
            .prepare("SELECT * FROM (SELECT * FROM runs WHERE bot_id = ?1 ORDER BY id DESC LIMIT ?2) ORDER BY id DESC")
            .unwrap();
        s.query_map(params![bot_id, limit], run_row).unwrap().filter_map(Result::ok).collect()
    }

    pub fn run(&self, id: i64) -> Option<Run> {
        self.c().query_row("SELECT * FROM runs WHERE id = ?", [id], run_row).optional().ok().flatten()
    }

    pub fn fact(&self, run_id: i64, kind: &str, detail: &str) {
        let _ = self.c().execute(
            "INSERT INTO run_facts (run_id, at, kind, detail) VALUES (?, ?, ?, ?)",
            params![run_id, now(), kind, detail],
        );
    }

    pub fn facts(&self, run_id: i64) -> Vec<RunFact> {
        let c = self.c();
        let mut s = c.prepare("SELECT * FROM run_facts WHERE run_id = ? ORDER BY id").unwrap();
        s.query_map([run_id], fact_row).unwrap().filter_map(Result::ok).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> Db {
        let path = std::env::temp_dir().join(format!("botd-{name}-{}.sqlite", crate::short_id()));
        let _ = std::fs::remove_file(&path);
        Db::open(&path).unwrap()
    }

    fn queued(db: &Db, bot_id: &str) -> i64 {
        db.enqueue_run(NewRun {
            bot_id: bot_id.into(),
            thread: "chat".into(),
            kind: "user".into(),
            message_id: None,
            from_bot: None,
            prompt: "hi".into(),
        })
    }

    #[test]
    fn one_run_per_bot_and_in_order() {
        let db = temp_db("order");
        let bot = db.create_bot("A", "#fff", "waddle", "claude", None, false);
        let first = queued(&db, &bot.id);
        let second = queued(&db, &bot.id);

        let run = db.claim_for(&bot.id, now()).expect("first claim");
        assert_eq!(run.id, first);
        assert_eq!(run.state, "claimed");
        assert_eq!(run.attempts, 1);
        // The Bot already has one, so the next claim waits.
        assert!(db.claim_for(&bot.id, now()).is_none());

        db.finish_run(run.id, "done", 1, None);
        let next = db.claim_for(&bot.id, now()).expect("second claim");
        assert_eq!(next.id, second);
    }

    #[test]
    fn claim_resumes_the_bots_session() {
        let db = temp_db("session");
        let bot = db.create_bot("A", "#fff", "waddle", "claude", None, false);
        db.update_bot(&bot.id, BotPatch { session_id: Some(Some("ses_1".into())), ..Default::default() });
        queued(&db, &bot.id);
        let run = db.claim_for(&bot.id, now()).unwrap();
        assert_eq!(run.session.as_deref(), Some("ses_1"));
    }

    #[test]
    fn restart_keeps_a_started_session_and_drops_an_unstarted_one() {
        let db = temp_db("sweep");
        let a = db.create_bot("A", "#fff", "waddle", "claude", None, false);
        let b = db.create_bot("B", "#fff", "cloud", "claude", None, false);
        let started = queued(&db, &a.id);
        let unstarted = queued(&db, &b.id);

        let run = db.claim_for(&a.id, now()).unwrap();
        assert_eq!(run.id, started);
        db.started_run(run.id, "ses_9");
        let run = db.claim_for(&b.id, now()).unwrap();
        assert_eq!(run.id, unstarted);

        // A restart: the run that got going keeps its session, the other starts clean.
        assert_eq!(db.sweep(now()).len(), 2);
        let a_run = db.run(started).unwrap();
        let b_run = db.run(unstarted).unwrap();
        assert_eq!(a_run.state, "queued");
        assert_eq!(b_run.state, "queued");
        assert_eq!(a_run.session.as_deref(), Some("ses_9"));
        assert_eq!(b_run.session, None);
    }

    #[test]
    fn a_retry_waits_for_its_backoff() {
        let db = temp_db("backoff");
        let bot = db.create_bot("A", "#fff", "waddle", "claude", None, false);
        let id = queued(&db, &bot.id);
        let run = db.claim_for(&bot.id, now()).unwrap();
        let later = now() + 30_000;
        db.requeue_run(id, later, "rate limited");

        assert!(db.claim_for(&bot.id, now()).is_none());
        let run = db.claim_for(&bot.id, later).expect("due after backoff");
        assert_eq!(run.id, id);
        assert_eq!(run.attempts, 2);
    }

    #[test]
    fn stopping_cancels_queued_work() {
        let db = temp_db("stop");
        let bot = db.create_bot("A", "#fff", "waddle", "claude", None, false);
        let id = queued(&db, &bot.id);
        assert_eq!(db.cancel_queued(&bot.id), 1);
        assert_eq!(db.run(id).unwrap().state, "cancelled");
        assert!(db.claim_for(&bot.id, now()).is_none());
    }

    #[test]
    fn deleted_bots_leave_no_work_behind() {
        let db = temp_db("gone");
        let bot = db.create_bot("A", "#fff", "waddle", "claude", None, false);
        let id = queued(&db, &bot.id);
        db.delete_bot(&bot.id);
        db.sweep(now());
        assert_eq!(db.run(id).unwrap().state, "cancelled");
    }

    #[test]
    fn facts_keep_the_timeline() {
        let db = temp_db("facts");
        let bot = db.create_bot("A", "#fff", "waddle", "claude", None, false);
        let id = queued(&db, &bot.id);
        db.fact(id, "queued", "user message 1");
        db.fact(id, "retry", "rate limited");
        let kinds: Vec<String> = db.facts(id).into_iter().map(|f| f.kind).collect();
        assert_eq!(kinds, ["queued", "retry"]);
    }
}
