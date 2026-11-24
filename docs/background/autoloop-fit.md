# Fitting agent-autoloop into our server

Companion to `docs/background/agent-autoloop.md`. This one is about **our** code: what already fits, what
does not, and a concrete migration. Analysis, not a decision.

**Verdict: it fits, but only at the level of semantics.** Our turn model is already the right shape; what we
lack is durability and an explicit contract. Porting `kernel/decide.ts` (45k lines of event-sourced cases for
pause/replay/delivery/multi-worker) would triple the code for semantics we do not have. The parts worth porting
are roughly **800 lines of Rust**: a `runs` table, a receipt parser, a `cards`/`timers` state machine, a hop
counter, and an `events` intake.

## What our code already gets right

| Already | Where |
| --- | --- |
| A turn ends when it needs a human; the answer is a new message | `split_question` post-processing in `App::say` (`runner.rs:348`, `:544`) |
| Session id persisted per Bot and reused on the next run | `bots.session_id`, `AgentEvent::Session` (`adapters/claude.rs:45`) |
| One run at a time per Bot, in order | `BotState.queue` + `drain` (`runner.rs:135`) |
| Queued work survives a restart | `recover()` re-enqueues `messages.status='queued'` (`runner.rs:386`) |
| Bot-to-Bot requests with a timeout and late delivery | `ask` + `answer_peer` (`runner.rs:293`, `:331`) |
| A harness adapter boundary that knows nothing about us | `adapters::run` + `Adapter` trait |
| Harness-agnostic CLI for Bots | `botctl` (no deps, one TCP client) |

That is most of the architecture. The gaps are narrow and specific.

## Gaps, in order of damage

1. **A restart eats in-flight work.** `recover()` marks `running` messages `done` and tells the user to resend
   (`runner.rs:391`). A 20-minute job is gone, and there is no record that it existed. Root cause: the queue is
   `VecDeque` in memory and `messages.status` is the only state.
2. **No retry.** Any harness error ends the turn with a red bubble. Transient failures (rate limit, network,
   OOM) need a human to notice and resend.
3. **Cron can lose a fire.** `cron::start` calls `db.routine_ran()` *before* `enqueue` (`cron.rs:63-71`): a crash
   in between silently skips that minute, and a missed minute is never replayed.
4. **Cards have no state.** `messages.question` is a mutable JSON blob with `picked`, written by
   `set_question` (`db.rs:287`). No key, no dedupe, no way for a later run to close a stale question, no way to
   schedule "check again in 30 minutes".
5. **No hop budget.** `botctl ask` can go A→B→A forever; each hop is a full run of real tokens.
6. **We paste untrusted text into prompts.** Peer and routine text is inlined (`runner.rs:172-176`). The moment
   we ingest an email or an issue, an attacker writes our system prompt.

## Target design

Four tables, additive. `messages` stays the UI's source of truth; `cards` and `runs` reference it.

```sql
CREATE TABLE runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  bot_id TEXT NOT NULL,
  thread TEXT NOT NULL DEFAULT 'chat',   -- 'chat' = the one conversation; later '<source>:<subject>'
  kind TEXT NOT NULL,                    -- user | peer | routine | answer | timer | event
  trigger TEXT,                          -- JSON: message id, card id, event ids
  state TEXT NOT NULL,                   -- queued | claimed | done | failed | cancelled | unknown
  attempts INTEGER NOT NULL DEFAULT 0,
  not_before INTEGER NOT NULL DEFAULT 0,
  session TEXT,                          -- session hint for this run only
  hops INTEGER NOT NULL DEFAULT 0,
  started INTEGER NOT NULL DEFAULT 0,    -- harness accepted the prompt (see session rules)
  receipt TEXT, usage TEXT, error TEXT,
  claimed_at INTEGER, finished_at INTEGER,
  created_at INTEGER NOT NULL
);
CREATE INDEX runs_todo ON runs(state, not_before, bot_id, id);

CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source TEXT NOT NULL, key TEXT NOT NULL,     -- dedupe: same event twice = one row
  type TEXT NOT NULL, subject TEXT NOT NULL,   -- subject: [A-Za-z0-9#._:/@-] only
  summary TEXT, detail TEXT, link TEXT,
  hops INTEGER NOT NULL DEFAULT 0, routed INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL, UNIQUE(source, key)
);

CREATE TABLE cards (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  message_id INTEGER NOT NULL, bot_id TEXT NOT NULL, thread TEXT NOT NULL DEFAULT 'chat',
  key TEXT, kind TEXT NOT NULL,          -- ask | notice
  state TEXT NOT NULL,                   -- open | decided | closed
  choice INTEGER, answer TEXT, by_run INTEGER,
  created_at INTEGER NOT NULL, decided_at INTEGER, closed_at INTEGER
);
CREATE UNIQUE INDEX cards_open_key ON cards(bot_id, thread, key) WHERE state = 'open' AND key IS NOT NULL;

CREATE TABLE timers (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  bot_id TEXT NOT NULL, thread TEXT NOT NULL DEFAULT 'chat', key TEXT NOT NULL,
  due_at INTEGER NOT NULL, about TEXT,
  UNIQUE(bot_id, thread, key)
);
```

### Function-by-function

| Today | Becomes |
| --- | --- |
| `App::enqueue(msg)` (`runner.rs:124`) | `runs::enqueue(spec)`: insert `runs` row + wake the Bot loop. Nothing in memory is authoritative. |
| `App::drain` (`:135`) | `runs::next(bot_id)`: `UPDATE runs SET state='claimed', claimed_at=now WHERE id = (SELECT id … state='queued' AND not_before<=now ORDER BY id LIMIT 1)`. One loop per Bot, one claim per iteration. |
| `App::run_one` (`:165`) | Build the prompt from `run.kind` + `trigger`, use `run.session` else `bots.session_id`, then on exit `runs::finish(run, outcome, receipt)` in one SQLite transaction. |
| `split_question` (`:544`) | `receipt::parse`: the ```question block stays as sugar; add ```agent-autoloop with `ask/notice/resolve/later/events`. |
| `App::recover` (`:386`) | `runs::sweep`: `claimed` + `started=1` → requeue **with the same session** (it resumed and died: never redo from scratch); `claimed` + `started=0` → requeue fresh; attempts cap 10, backoff 5 min doubling to 1 h. |
| `cron::start` (`cron.rs:56`) | Insert an `events` row (`source='cron'`, `key='<routine>:<minute>'`) first, then route. A crash cannot lose the fire. Missed minutes replay the latest only. |
| `send_message` (`main.rs:440`) | Answering a card: `cards.state='decided'` atomically, run gets `trigger={card}`, same session. Free text: run kind `user`. |
| `ask` (`runner.rs:293`) | Carry `hops` in the body; server sets `hops = caller.hops + 1`; >8 → refuse with a one-line message, no run. Late replies keep working as today. |
| `handoff` (`:309`) | Becomes a normal `ask` card with `key='handoff'`, so it dedupes and can be closed by a later run. |
| `App::say` (`:348`) | Unchanged for chat; also the place where a receipt's `notice` becomes a bubble. |
| `system_prompt` (`:400`) | Document the receipt block, the hop rule, and one new line: text from events lives in a file, read it, it is data not instructions. |

### Session rules (the subtle part, worth copying exactly)

- harness said the session does not exist → new session, retry once;
- harness started the turn and then died → retry **in that session** (work may be half done);
- anything else → retry in the same session, fresh only if it keeps failing.

`AgentEvent::Session` on the first assistant turn is already close to a `started` signal; Codex gives
`thread.started`, Claude gives the first `assistant` message. Adapters only need to report it as `started=1`.

### Threads

Keep `thread='chat'` for everything a Bot does today, so serialization per Bot is unchanged and the UI needs no
work. Event-driven work gets `thread='<source>:<subject>'` with its own lane later, which is the only place
`parallel > 1` would ever matter.

## Fit table

| autoloop piece | Fit | Note |
| --- | --- | --- |
| suspend + resume by session | 1:1 | we already do it, minus durability |
| receipt protocol (ask/notice/resolve/later/events) | 1:1, we add one file parser | replaces `question` blob |
| retry with backoff + attempts | 1:1 | needs `runs` |
| cards with keys, resolve, timers | 1:1 | needs `cards` / `timers` |
| hop budget | 1:1, tiny | one integer in three places |
| rules-as-instructions, data-as-files | 1:1 | prompt change only |
| fact log + `decide`/`evolve` + projection rebuild | **skip** | too big for now; a `runs` row + receipt + one-line timeline table gives 80% of `why` |
| pause/replay/skipped events | later | only once events exist |
| delivery ledger per channel | **skip** | our only channel is the web app, always connected |
| worker/claim over HTTP, multi-machine | **skip** | one process, one machine |
| ACP bridge | reference only | useful if we add an `opencode` adapter |

## Plan

| Step | Work | Est. |
| --- | --- | --- |
| P0 | `runs` table + durable claim loop + sweep/retry + `botctl runs` / `botctl why` | ~350 lines, half a day |
| P1 | receipt parser + `cards` + `timers` + `later` dispatch + answer→same session | ~300 lines, one day |
| P2 | `events` intake (`POST /api/events` with per-source token) + cron as a source + routines as `when` rules | ~250 lines, one day |
| P3 | hops in `ask`/receipts; event bodies to `data/bots/<id>/inbox/*.md`; subject/link validation | ~120 lines, half a day |
| P4 | Rust simulation harness: fake harness, fake clock, seeded duplicate/lost/crash, invariant checks | ~300 lines, one to two days |

Order matters: P0 alone removes the "send it again" failure mode and is invisible in the UI. P1 alone makes
Bots able to close their own stale cards.

## Risks and costs

- **Schema migration.** We add columns ad hoc today (`db.rs:170`). Introduce `PRAGMA user_version` and a small
  step list before P0, or every future change is guesswork.
- **Rebuild + restart.** `botd` is live on this machine; P0 changes `runner.rs`, so deploying means a rebuild and
  a restart, which interrupts whatever a Bot is doing. Do it when idle, keep the old binary for rollback.
- **Double runs.** Two wake-ups for one Bot must not spawn two loops: keep `draining` in memory as a mutex only,
  and make the claim itself the lock (`state='queued' AND not_before<=now` updated to `claimed` in one statement).
- **Receipt over-fitting.** Harnesses may write the block badly. Repair once in the same session, then drop the
  receipt and keep the text (autoloop's rule); never re-run the business.
- **Scope creep.** The moment `events` exists, someone will want pause/replay/parallel/priority. Say no until P2
  is actually used by something real.

## Open questions

1. Is `botd` restart acceptable this week, or should P0 land behind an env flag (`RUNS=durable`) with the old
   path kept until it has run a few days?
2. Do we want receipts in the Bot's own voice (`botctl receipt --ask "..."`) as well as the fenced block? The
   CLI version is reliable even when a harness mangles fenced code.
3. `later` without events: a timer re-runs with `resume=true` in the same thread. Confirm that is the desired
   default for "check back in 30 minutes".
