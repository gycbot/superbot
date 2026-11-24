//! Runs Bots: one task at a time per Bot, in order. Builds the system prompt
//! (identity, rules, memory), streams harness events to the web app, and saves replies.

use crate::adapters::{self, AbortReason, AgentEvent, RunInput};
use crate::db::{self, Bot, BotPatch, Db, Message, NewRun, Run};
use crate::screen::{self, Screens};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, oneshot, watch};

pub struct Config {
    pub data_dir: PathBuf,
    pub workspace: PathBuf,
    pub user_name: String,
    /// Added to PATH so Bots can run `botctl`.
    pub bin_dir: PathBuf,
    pub api_url: String,
    pub bot_token: String,
    pub run_timeout: Duration,
    pub default_harness: String,
    /// Model assigned to new Bots when none is given explicitly.
    pub default_model: Option<String>,
}

/// A run in progress: how to stop it, and what to show in the UI meanwhile.
struct Live {
    abort: watch::Sender<AbortReason>,
    status: String,
    draft: String,
}

#[derive(Default)]
struct BotState {
    /// Advisory only: the database is the queue. Prevents two loops for one Bot.
    draining: bool,
    run: Option<Live>,
}

/// A run retried this many times in total, then it is left failed.
/// Each attempt spends the owner's real tokens, so the number stays small.
const MAX_ATTEMPTS: i64 = 4;

pub struct App {
    pub db: Db,
    pub cfg: Config,
    /// Every live update for the web app, as JSON text.
    pub tx: broadcast::Sender<String>,
    /// Each Bot's screen. None when this server has no display tools.
    pub screens: Option<Screens>,
    bots: Mutex<HashMap<String, BotState>>,
    /// Bots waiting on `botctl ask` or `botctl handoff`, by the message's id.
    waiters: Mutex<HashMap<i64, oneshot::Sender<String>>>,
    closing: AtomicBool,
}

impl App {
    pub fn new(db: Db, cfg: Config, screens: Option<Screens>) -> Arc<App> {
        Arc::new(App { db, cfg, tx: broadcast::channel(1024).0, screens, bots: Mutex::default(), waiters: Mutex::default(), closing: AtomicBool::new(false) })
    }

    pub fn is_running(&self, bot_id: &str) -> bool {
        self.state().get(bot_id).is_some_and(|s| s.run.is_some())
    }

    pub fn send(&self, v: Value) {
        let _ = self.tx.send(v.to_string());
    }

    fn state(&self) -> std::sync::MutexGuard<'_, HashMap<String, BotState>> {
        self.bots.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// What the web app sees for a Bot.
    pub fn bot_view(&self, b: &Bot) -> Value {
        let preview = self.db.last_message(&b.id).map(|m| {
            let t: String = m.text.chars().filter(|c| !"#*_`>[]".contains(*c)).collect();
            let t = adapters::short(&t);
            if t.is_empty() { m.question.and_then(|q| q["title"].as_str().map(String::from)).unwrap_or_default() } else { t }
        });
        let mut v = json!({
            "id": b.id, "name": b.name, "color": b.color, "shape": b.shape, "harness": b.harness, "model": b.model, "effort": b.effort,
            "primary": b.primary, "hidden": b.hidden, "unread": b.unread, "preview": preview.unwrap_or_default(), "working": false,
        });
        if let Some(run) = self.state().get(&b.id).and_then(|s| s.run.as_ref()) {
            v["working"] = json!(true);
            v["status"] = json!(run.status);
            v["draft"] = json!(run.draft);
        }
        v
    }

    pub fn push_bot(&self, id: &str) {
        if let Some(b) = self.db.bot(id) {
            self.send(json!({ "type": "bot", "bot": self.bot_view(&b) }));
        }
    }

    pub fn push_message(&self, m: &Message) {
        let bot = self.db.bot(&m.bot_id).map(|b| self.bot_view(&b));
        self.send(json!({ "type": "message", "message": m, "bot": bot }));
    }

    pub fn bot_dir(&self, id: &str) -> PathBuf {
        self.cfg.data_dir.join("bots").join(id)
    }
    fn memory_file(&self, id: &str) -> PathBuf {
        self.bot_dir(id).join("MEMORY.md")
    }
    fn user_file(&self) -> PathBuf {
        self.cfg.data_dir.join("USER.md")
    }

    pub fn init_bot_files(&self, bot: &Bot) {
        let _ = std::fs::create_dir_all(self.bot_dir(&bot.id));
        let mem = self.memory_file(&bot.id);
        if !mem.exists() {
            let _ = std::fs::write(&mem, format!("# {}\n\n## Job\n\nNot set yet. Ask the user.\n\n## Rules\n\n## Work log\n", bot.name));
        }
        let user = self.user_file();
        if !user.exists() {
            let _ = std::fs::write(&user, format!("# About the user\n\n- Name: {}\n", self.cfg.user_name));
        }
    }

    /// Queues a message as a durable run, then wakes the Bot's loop.
    pub fn enqueue(self: &Arc<Self>, msg: Message) {
        let id = self.db.enqueue_run(NewRun {
            bot_id: msg.bot_id.clone(),
            thread: "chat".into(),
            kind: msg.role.clone(),
            message_id: Some(msg.id),
            from_bot: msg.from_bot.clone(),
            prompt: prompt_for(self, &msg),
        });
        let from = msg.from_bot.as_deref().unwrap_or("the user");
        self.db.fact(id, "queued", &format!("{} message {} from {from}", msg.role, msg.id));
        self.wake(&msg.bot_id);
    }

    /// Starts the Bot's loop unless it is already running. The database decides
    /// what to run, so a missed wake-up only costs the scheduler's next tick.
    pub fn wake(self: &Arc<Self>, bot_id: &str) {
        let mut st = self.state();
        let s = st.entry(bot_id.to_string()).or_default();
        if s.draining {
            return;
        }
        s.draining = true;
        drop(st);
        tokio::spawn(self.clone().drain(bot_id.to_string()));
    }

    async fn drain(self: Arc<Self>, bot_id: String) {
        loop {
            if self.closing.load(Ordering::SeqCst) {
                return; // the claim stays; the next start sweeps it back into the queue
            }
            let Some(run) = self.db.claim_for(&bot_id, db::now()) else { break };
            match self.db.bot(&bot_id) {
                Some(bot) => self.run_one(bot, run).await,
                None => self.db.finish_run(run.id, "cancelled", 0, Some("Bot was deleted")),
            }
        }
        {
            let mut st = self.state();
            if let Some(s) = st.get_mut(&bot_id) {
                s.draining = false;
                if s.run.is_none() {
                    st.remove(&bot_id);
                }
            }
        }
        self.send(json!({ "type": "status", "botId": bot_id, "working": false }));
    }

    async fn run_one(self: &Arc<Self>, mut bot: Bot, run: Run) {
        if adapters::get(&bot.harness).is_none() {
            self.db.finish_run(run.id, "failed", 0, Some("Unknown harness"));
            return self.say(&bot.id, &format!("Unknown harness \"{}\".", bot.harness), true);
        }
        self.init_bot_files(&bot);
        self.db.fact(run.id, "claim", &format!("attempt {} on {}", run.attempts, bot.harness));
        if let Some(mid) = run.message_id {
            self.db.set_status(mid, "running");
        }
        let mut session = run.session.clone();
        let mut prompt = run.prompt.clone();
        if run.attempts > 1 {
            prompt.push_str(
                "\n\n[Retry: an earlier attempt was cut off. Check what is already done before repeating \
                 anything with side effects.]",
            );
        }

        for pass in 0..2 {
            let (abort_tx, abort_rx) = watch::channel(None);
            self.state().entry(bot.id.clone()).or_default().run =
                Some(Live { abort: abort_tx.clone(), status: "Thinking".into(), draft: String::new() });
            self.send(json!({ "type": "status", "botId": bot.id, "working": true, "status": "Thinking" }));
            let timeout = self.cfg.run_timeout;
            let timer = tokio::spawn(async move {
                tokio::time::sleep(timeout).await;
                abort_tx.send_if_modified(|r| r.get_or_insert("timeout") == &"timeout");
            });

            let mut env = HashMap::from([
                ("BOT_ID".to_string(), bot.id.clone()),
                ("BOT_API".to_string(), self.cfg.api_url.clone()),
                ("BOT_TOKEN".to_string(), self.cfg.bot_token.clone()),
            ]);
            let path = std::env::var("PATH").unwrap_or_default();
            env.insert("PATH".into(), format!("{}:{path}", self.cfg.bin_dir.display()));
            if let Some(screens) = &self.screens
                && let Some(info) = screens.ensure(&bot.id, |id| self.is_running(id)).await
            {
                env.extend(screen::bot_env(&info, &bot.id, &self.bot_dir(&bot.id)));
            }
            let input = RunInput {
                prompt: prompt.clone(),
                system: self.system_prompt(&bot),
                session_id: session.clone(),
                cwd: self.cfg.workspace.clone(),
                model: bot.model.clone(),
                effort: bot.effort.clone(),
                env,
                state_dir: self.bot_dir(&bot.id),
            };

            let before = self.db.last_id();
            let run_id = run.id;
            let mut errors: Vec<String> = Vec::new();
            let mut replies = 0;
            let id = bot.id.clone();
            adapters::run(adapters::get(&bot.harness).unwrap(), input, abort_rx.clone(), |e| match e {
                AgentEvent::Session(s) => {
                    self.db.started_run(run_id, &s);
                    if self.db.bot(&id).and_then(|b| b.session_id).as_deref() != Some(&s) {
                        self.db.update_bot(&id, BotPatch { session_id: Some(Some(s)), ..Default::default() });
                    }
                }
                AgentEvent::TextDelta(t) => {
                    let draft = self.with_run(&id, |r| { r.draft.push_str(&t); r.draft.clone() });
                    self.send(json!({ "type": "draft", "botId": id, "text": draft }));
                }
                AgentEvent::Text(t) => {
                    self.with_run(&id, |r| r.draft.clear());
                    replies += 1;
                    self.say(&id, &t, false);
                }
                AgentEvent::Tool { name, detail } => {
                    let status = tool_label(&name, &detail);
                    self.with_run(&id, |r| r.status = status.clone());
                    self.send(json!({ "type": "status", "botId": id, "working": true, "status": status }));
                }
                AgentEvent::Error(m) => errors.push(m),
            })
            .await;
            timer.abort();
            // Done running. The stream may have ended without a final block: keep what was streamed.
            let draft = self.state().get_mut(&id).and_then(|s| s.run.take()).map(|r| r.draft).unwrap_or_default();
            if !draft.trim().is_empty() {
                replies += 1;
                self.say(&id, &draft, false);
            }

            let reason = *abort_rx.borrow();
            // A shutdown leaves the run claimed on purpose: the next start sweeps
            // it back into the queue and resumes the same session.
            if reason == Some("shutdown") {
                return;
            }

            // The harness lost its session (e.g. its files were wiped): start fresh once.
            let lost = errors.iter().any(|m| {
                let m = m.to_lowercase();
                m.contains("no conversation found")
                    || ((m.contains("session") || m.contains("conversation"))
                        && (m.contains("not found") || m.contains("no such") || m.contains("does not exist")))
            });
            if lost && session.is_some() && replies == 0 && pass == 0 {
                self.db.update_bot(&id, BotPatch { session_id: Some(None), ..Default::default() });
                self.db.fact(run_id, "session-lost", &first_line(&errors.join("; ")));
                bot.session_id = None;
                session = None;
                continue;
            }

            if reason == Some("stopped") {
                self.say(&id, "Stopped.", true);
                self.db.fact(run_id, "cancelled", "stopped by the user");
                self.db.finish_run(run_id, "cancelled", replies, Some("Stopped."));
            } else if let Some("timeout") = reason {
                let mins = self.cfg.run_timeout.as_secs() / 60;
                let msg = format!("I hit the {mins} minute limit for one task and stopped.");
                self.say(&id, &msg, true);
                self.db.fact(run_id, "timeout", &msg);
                self.db.finish_run(run_id, "failed", replies, Some(&msg));
                run_end_hook(&bot, &run, "failed", &msg);
            } else if !errors.is_empty() {
                let msg = errors.join("\n\n");
                if replies == 0 && run.attempts < MAX_ATTEMPTS && !is_fatal(&msg) {
                    let delay = backoff(run.attempts);
                    self.db.requeue_run(run.id, db::now() + delay, &msg);
                    self.db.fact(
                        run_id,
                        "retry",
                        &format!("attempt {}: {}; next in {}s", run.attempts, first_line(&msg), delay / 1000),
                    );
                    if run.attempts == 1 {
                        self.say(
                            &id,
                            &format!("Hit an error. Retrying in {} — no need to resend.\n\n{}", human_delay(delay), first_line(&msg)),
                            true,
                        );
                    }
                    return;
                }
                self.say(&id, &msg, true);
                self.db.fact(run_id, "failed", &first_line(&msg));
                self.db.finish_run(run_id, "failed", replies, Some(&msg));
                run_end_hook(&bot, &run, "failed", &msg);
            } else {
                if replies == 0 {
                    self.say(&id, "Done.", false);
                }
                self.db.fact(run_id, "done", &format!("{replies} repl{}", if replies == 1 { "y" } else { "ies" }));
                self.db.finish_run(run_id, "done", replies, None);
                run_end_hook(&bot, &run, "done", "");
            }
            // Whoever asked this Bot waits for these bubbles as the answer, even when
            // the run ended badly.
            if run.kind == "peer"
                && let Some(m) = run.message_id.and_then(|id| self.db.message(id))
            {
                self.answer_peer(&m, before);
            }
            return;
        }
    }

    fn with_run<T: Default>(&self, bot_id: &str, f: impl FnOnce(&mut Live) -> T) -> T {
        self.state().get_mut(bot_id).and_then(|s| s.run.as_mut()).map(f).unwrap_or_default()
    }

    /// Another Bot asks this one something. Resolves with the reply, or None if it
    /// takes longer than `wait`; then the reply is delivered to the asker later.
    pub async fn ask(self: &Arc<Self>, from: &Bot, to: &Bot, text: &str, wait: Duration) -> Option<String> {
        let msg = self.db.add_message_from(&to.id, "peer", text, None, false, Some("queued"), Some(&from.id));
        let (tx, rx) = oneshot::channel();
        self.waiters.lock().unwrap().insert(msg.id, tx);
        self.push_message(&msg);
        self.enqueue(msg.clone());
        let reply = tokio::time::timeout(wait, rx).await.ok().and_then(Result::ok);
        if reply.is_none() {
            self.waiters.lock().unwrap().remove(&msg.id);
        }
        reply
    }

    /// The Bot needs the user on its screen (a login, 2FA, a CAPTCHA). Posts a card
    /// with "Done" and waits for the answer, or None after `wait`; then the answer
    /// reaches the Bot later as a normal message.
    pub async fn handoff(&self, bot: &Bot, text: &str, wait: Duration) -> Option<String> {
        let q = json!({ "title": text, "options": ["Done", "I can't right now"], "handoff": true });
        let m = self.db.add_message(&bot.id, "bot", "", Some(&q), false, None);
        self.db.update_bot(&bot.id, BotPatch { unread: Some(true), ..Default::default() });
        let (tx, rx) = oneshot::channel();
        self.waiters.lock().unwrap().insert(m.id, tx);
        self.push_message(&m);
        let answer = tokio::time::timeout(wait, rx).await.ok().and_then(Result::ok);
        if answer.is_none() {
            self.waiters.lock().unwrap().remove(&m.id);
        }
        answer
    }

    /// Takes the waiter for a handoff card, if its Bot is still waiting.
    pub fn take_handoff(&self, message_id: i64) -> Option<oneshot::Sender<String>> {
        let is_handoff = self.db.message(message_id).and_then(|m| m.question).is_some_and(|q| q["handoff"] == json!(true));
        if is_handoff { self.waiters.lock().unwrap().remove(&message_id) } else { None }
    }

    /// Sends the asked Bot's reply to whoever asked: the waiting `botctl ask`,
    /// or, if it gave up waiting, as a new message in the asker's queue.
    fn answer_peer(self: &Arc<Self>, msg: &Message, before: i64) {
        let texts: Vec<String> = self.db.replies_after(&msg.bot_id, before).into_iter().map(|m| m.text).collect();
        let reply = if texts.is_empty() { "(No reply.)".to_string() } else { texts.join("\n\n") };
        let waiter = self.waiters.lock().unwrap().remove(&msg.id);
        let unsent = match waiter {
            Some(tx) => tx.send(reply).err(),
            None => Some(reply),
        };
        if let Some(reply) = unsent {
            let Some(asker) = msg.from_bot.as_deref().filter(|id| self.db.bot(id).is_some()) else { return };
            let m = self.db.add_message_from(asker, "reply", &reply, None, false, Some("queued"), Some(&msg.bot_id));
            self.push_message(&m);
            self.enqueue(m);
        }
    }

    /// Saves one Bot chat bubble. A trailing ```question block becomes a choice card.
    pub fn say(&self, bot_id: &str, text: &str, error: bool) {
        let (body, question) = split_question(text);
        let (body, atts) = collect_media(self, body);
        let attachments = (!atts.is_empty()).then(|| Value::Array(atts));
        let m = self.db.add_message_full(bot_id, "bot", body.trim(), question.as_ref(), error, None, None, attachments.as_ref());
        self.db.update_bot(bot_id, BotPatch { unread: Some(true), ..Default::default() });
        self.push_message(&m);
    }

    /// Stops the current task and cancels queued ones.
    pub fn stop(&self, bot_id: &str) {
        self.db.cancel_queued(bot_id);
        let mut st = self.state();
        if let Some(s) = st.get_mut(bot_id)
            && let Some(r) = &s.run
        {
            r.abort.send_if_modified(|v| v.get_or_insert("stopped") == &"stopped");
        }
    }

    /// Kills running agents. Queued messages run after the restart.
    pub fn shutdown(&self) {
        self.closing.store(true, Ordering::SeqCst);
        for s in self.state().values() {
            if let Some(r) = &s.run {
                r.abort.send_if_modified(|v| v.get_or_insert("shutdown") == &"shutdown");
            }
        }
    }

    pub fn is_idle(&self) -> bool {
        self.state().values().all(|s| s.run.is_none())
    }

    /// After a restart, runs that were in flight go back in the queue. One that had
    /// started resumes its session; the prompt tells it to check its work first.
    pub fn recover(self: &Arc<Self>) {
        // First start after the upgrade from the in-memory queue: messages that
        // were waiting then have no run yet, so requeue them once.
        for m in self.db.orphan_messages() {
            if self.db.bot(&m.bot_id).is_some() {
                self.enqueue(m);
            }
        }
        let now = db::now();
        for (id, started) in self.db.sweep(now) {
            let what = if started { "server restart; resuming the same session" } else { "server restart; starting fresh" };
            self.db.fact(id, "recovered", what);
        }
    }

    /// Wakes Bots with due work, and requeues claims whose process died.
    pub fn start_scheduler(self: &Arc<Self>) {
        let app = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut n = 0u64;
            loop {
                tick.tick().await;
                for bot in app.db.due_bots(db::now()) {
                    app.wake(&bot);
                }
                n += 1;
                if n % 30 == 0 {
                    let now = db::now();
                    // A claim can only reach this age if nothing is watching it: a live
                    // run is aborted by its own timeout first.
                    let cutoff = now - app.cfg.run_timeout.as_millis() as i64 - 300_000;
                    for (id, started) in app.db.sweep_stale(now, cutoff) {
                        let what = if started { "no longer running; resuming the same session" } else { "no longer running; starting fresh" };
                        app.db.fact(id, "recovered", what);
                    }
                }
            }
        });
    }

    fn system_prompt(&self, bot: &Bot) -> String {
        let read = |p: PathBuf| std::fs::read_to_string(p).unwrap_or_default().trim().to_string();
        let others: Vec<String> =
            self.db.bots().iter().filter(|b| b.id != bot.id).map(|b| format!("- {} (id {})", b.name, b.id)).collect();
        let primary = if bot.primary {
            " You are their primary Bot: their main point of contact. You help them decide which Bot should own which job."
        } else {
            ""
        };
        format!(
            r#"You are {name}, an AI teammate of {user}.{primary}
They talk to you in a chat app, like messaging a coworker. Finish work end to end.

## Your computer
- You run on a Linux computer shared with the user's other Bots. Your working folder is {ws}.
- Files, browser sign-ins, and credentials on it are shared by all Bots. Keep durable work in clear project folders.

## Memory
- Your memory file: {mem_path}. It holds your job, the user's rules and preferences for you, and a short dated log of important work.
- Shared facts about the user, used by every Bot: {user_path}. Name, time zone, and similar.
- Update these files yourself when you learn something lasting, without asking. Keep them short. Never store passwords, keys, or card numbers.
- Current contents:
<memory>
{mem}
</memory>
<user>
{user_facts}
</user>

## Routines
- For recurring or scheduled work, use `{botctl}`. Run `{botctl} help` to see how.
- Your recent runs: `{botctl} runs`. One run's timeline: `{botctl} why <id>`.
{screen}{toolbox}
## Style
- Reply like a teammate in chat: short, plain English. Markdown is fine.
- Each separate text block you write is shown as its own chat bubble. Before long work, send a one-line heads-up.
- Ask before anything you can't undo: sending messages, publishing, deleting, paying.
- If you are resuming after an interruption, check what is already done before repeating anything with side effects.
- To ask a multiple-choice question, end your reply with this block, and nothing after it:
```question
{{"title": "Your question?", "options": ["First", "Second", "Third"]}}
```

## Other Bots
{others}
- To ask another Bot something or hand it a task, run: {botctl} ask "<Bot name or id>" "<message>". It prints their reply. It can take minutes, so give the command a long timeout (10 minutes). If they take longer, their reply arrives later as a new message.
- Only loop in another Bot when the user asks or it clearly helps. Never ask the Bot that is asking you.

Current time: {now}."#,
            name = bot.name,
            // Full path: some harnesses run commands in a login shell that resets PATH.
            botctl = self.cfg.bin_dir.join("botctl").display(),
            screen = if self.screens.is_some() { screen_prompt(&self.cfg.bin_dir.join("botctl").display().to_string()) } else { String::new() },
            user = self.cfg.user_name,
            ws = self.cfg.workspace.display(),
            mem_path = self.memory_file(&bot.id).display(),
            user_path = self.user_file().display(),
            mem = read(self.memory_file(&bot.id)),
            user_facts = read(self.user_file()),
            others = if others.is_empty() { "None yet.".into() } else { others.join("\n") },
            toolbox = {
                let t = read(self.cfg.data_dir.join("TOOLS.md"));
                if t.is_empty() { String::new() } else { format!("\n## Toolbox (tools and conventions on this computer)\n{t}\n") }
            },
            now = chrono::Local::now().format("%A, %B %-d, %Y, %H:%M (UTC%:z)"),
        )
    }
}

/// Copy a local file referenced by a Bot into the uploads dir and describe it.
fn store_local_file(app: &App, path: &str) -> Option<Value> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.starts_with("http://") || trimmed.starts_with("https://")
        || trimmed.starts_with("/attachments/") || trimmed.starts_with("data:") { return None; }
    let p = if std::path::Path::new(trimmed).is_absolute() {
        std::path::PathBuf::from(trimmed)
    } else {
        app.cfg.workspace.join(trimmed)
    };
    if !p.is_file() { return None; }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let (mime, kind) = match ext.as_str() {
        "png" => ("image/png", "image"), "jpg" | "jpeg" => ("image/jpeg", "image"),
        "gif" => ("image/gif", "image"), "webp" => ("image/webp", "image"),
        "svg" => ("image/svg+xml", "image"), "ico" => ("image/x-icon", "image"),
        "mp4" => ("video/mp4", "video"), "webm" => ("video/webm", "video"),
        "mov" => ("video/quicktime", "video"), "mkv" => ("video/x-matroska", "video"),
        _ => return None,
    };
    let id: String = (0..16).map(|_| format!("{:02x}", fastrand_byte())).collect();
    let dir = app.cfg.data_dir.join("uploads");
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::copy(&p, dir.join(format!("{id}.{ext}"))).ok()?;
    Some(json!({ "id": id, "name": p.file_name().unwrap_or_default().to_string_lossy(), "mime": mime, "kind": kind }))
}

fn fastrand_byte() -> u8 {
    use std::io::Read;
    let mut b = [0u8; 1];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).map(|_| b[0])
        .unwrap_or_else(|_| (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().subsec_nanos() & 0xff) as u8)
}

/// Rewrite markdown media links that point at local files into /attachments URLs,
/// returning the rewritten text and the attachment descriptors to save on the message.
fn collect_media(app: &App, text: &str) -> (String, Vec<Value>) {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut atts = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let is_img = bytes[i..].starts_with(b"![");
        let is_vid = bytes[i..].starts_with(b"[video](");
        if is_img || is_vid {
            let search_from = if is_img { i + 2 } else { i + 8 };
            let (path_start, prefix_end) = if is_img {
                match text[search_from..].find("](") {
                    Some(p) => (search_from + p + 2, search_from + p + 2),
                    None => (0, 0),
                }
            } else {
                (search_from, search_from)
            };
            if path_start > 0 {
                if let Some(end) = text[path_start..].find(')') {
                    let path = &text[path_start..path_start + end];
                    let whole_end = path_start + end + 1;
                    if let Some(att) = store_local_file(app, path) {
                        out.push_str(&text[i..prefix_end]);
                        out.push_str(&format!("/attachments/{}", att["id"].as_str().unwrap_or("")));
                        out.push(')');
                        atts.push(att);
                        i = whole_end;
                        continue;
                    }
                }
            }
        }
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    (out, atts)
}

/// Splits off a trailing ```question {json}``` block.
/// The prompt for one message, built once when the run is queued so a retry
/// repeats the same task even if the chat has moved on.
fn prompt_for(app: &App, msg: &Message) -> String {
    let from = || {
        msg.from_bot
            .as_deref()
            .and_then(|id| app.db.bot(id))
            .map(|b| format!("{} (id {})", b.name, b.id))
            .unwrap_or("another Bot".into())
    };
    let mut prompt = match msg.role.as_str() {
        "routine" => format!("(Scheduled routine) {}", msg.text),
        "peer" => format!("(Message from {}, another Bot. Your reply goes back to them, not the user.)\n\n{}", from(), msg.text),
        "reply" => format!("(Reply from {} to what you asked earlier. Tell the user what matters.)\n\n{}", from(), msg.text),
        _ => msg.text.clone(),
    };
    // Tell the harness about files the user attached so it can inspect them with its tools.
    if let Some(arr) = msg.attachments.as_ref().and_then(|a| a.as_array())
        && !arr.is_empty()
    {
        let lines: Vec<String> = arr
            .iter()
            .map(|a| {
                let kind = a["kind"].as_str().unwrap_or("file");
                let name = a["name"].as_str().unwrap_or("file");
                let id = a["id"].as_str().unwrap_or("");
                format!("- [{}] {} — {}", kind, name, app.cfg.data_dir.join("uploads").join(id).display())
            })
            .collect();
        prompt.push_str("\n\n[User attached files — you may read/inspect these paths with your tools]\n");
        prompt.push_str(&lines.join("\n"));
    }
    prompt
}

/// Errors that retrying cannot fix, so the run stops and tells the user.
fn is_fatal(msg: &str) -> bool {
    let m = msg.to_lowercase();
    [
        "api key",
        "invalid_api_key",
        "unauthorized",
        "authentication",
        "model not found",
        "no such model",
        "invalid model",
        "does not have access",
        "unknown harness",
    ]
    .iter()
    .any(|p| m.contains(p))
}

/// Wait before the next attempt: 30 s, 1 min, 2 min, 5 min.
fn backoff(attempts: i64) -> i64 {
    const STEPS: [i64; 4] = [30_000, 60_000, 120_000, 300_000];
    STEPS[(attempts.clamp(1, MAX_ATTEMPTS) as usize) - 1]
}

fn human_delay(ms: i64) -> String {
    let s = ms / 1000;
    if s < 60 { format!("{s} seconds") } else { format!("{} minutes", s / 60) }
}

fn first_line(msg: &str) -> String {
    let line = msg.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    line.chars().take(200).collect()
}

fn split_question(text: &str) -> (&str, Option<Value>) {
    let t = text.trim_end();
    let Some(start) = t.rfind("```question") else { return (text, None) };
    let Some(body) = t[start + 11..].strip_suffix("```") else { return (text, None) };
    let Ok(q) = serde_json::from_str::<Value>(body.trim()) else { return (text, None) };
    let (Some(title), Some(opts)) = (q["title"].as_str(), q["options"].as_array()) else { return (text, None) };
    let options: Vec<String> = opts.iter().take(6).map(|o| o.as_str().map(String::from).unwrap_or(o.to_string())).collect();
    (&t[..start], Some(json!({ "title": title, "options": options })))
}

fn tool_label(name: &str, detail: &str) -> String {
    let n = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    let verb = if has(&["bash", "shell", "command"]) {
        "Running".to_string()
    } else if has(&["read", "view"]) {
        "Reading".into()
    } else if has(&["write", "edit", "patch", "file_change"]) {
        "Editing".into()
    } else if has(&["search", "grep", "glob", "find"]) {
        "Searching".into()
    } else if has(&["web", "fetch", "browse"]) {
        "Browsing".into()
    } else {
        format!("Using {name}")
    };
    if detail.is_empty() { verb } else { format!("{verb}: {detail}") }
}

/// How to use the Bot's screen. Tools are plain CLIs, so this works on every harness.
fn screen_prompt(botctl: &str) -> String {
    format!(
        r#"
## Your screen
- You have your own screen with a browser. The user can watch it live and take over.
- Prefer APIs and command-line tools. Use the browser for sites that have no API.
- Websites: `agent-browser`. Before your first use in a task, run `agent-browser skills get core` and follow it, with one exception: your session and browser profile are already set for you, so never pass `--session`, `--profile`, or `--headed`, and never create other sessions. Your sign-ins persist between tasks.
- Desktop apps: `agent-desktop`. Before your first use, run `agent-desktop skills get desktop`.
- When you need the user on your screen (a login, 2FA code, CAPTCHA, or payment check), run: {botctl} handoff "<what you need, in one line>". It waits until they finish, so give the command a 10 minute timeout. Never ask for passwords in chat.
"#
    )
}

/// t3083: event-driven notice. When a run ends, run `RUN_END_HOOK` (if set) with
/// state/bot/run info in env, so failures (and routine runs with no delivery) email the captain.
fn run_end_hook(bot: &Bot, run: &Run, state: &str, msg: &str) {
    let Ok(hook) = std::env::var("RUN_END_HOOK") else { return };
    if hook.is_empty() {
        return;
    }
    let mut cmd = tokio::process::Command::new(hook);
    cmd.env("RUN_STATE", state)
        .env("RUN_ID", run.id.to_string())
        .env("RUN_KIND", &run.kind)
        .env("RUN_CREATED_AT", run.created_at.to_string())
        .env("RUN_BOT_ID", &bot.id)
        .env("RUN_BOT_NAME", &bot.name)
        .env("RUN_MSG", msg.chars().take(1500).collect::<String>())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    tokio::spawn(async move {
        if let Ok(mut child) = cmd.spawn() {
            let _ = child.wait().await;
        }
    });
}
