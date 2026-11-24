# What we can take from agent-autoloop

Source: https://github.com/KKKKhazix/agent-autoloop (MIT, ~32k lines TypeScript, Node 24, **zero runtime deps**).
Read at commit `9e97a61`, 2026-11-20. Analysis, not a decision. Decisions go in `docs/adr/`.

## What it is

A headless "middle office" for CLI agents. No UI, no own agent loop. Events come in (cron, GitHub/GitLab
webhook, HTTP), rules decide which agent runs where, the agent prints one last message as its **receipt**, and
the process exits. When it needs you it opens a **card** and stops; answering the card starts a new run that
resumes the old session. Small enough to run on a laptop, no open ports.

```
        event                claim
source ────────▶ hub ◀──────── worker ── agent
                  │  ▲
            card  │  │ answer
                  ▼  │
               channel ── you
```

Layers: `kernel/` (pure decide/evolve), `host/` (Node HTTP, SQLite, clock, plugins), `protocol/` (JSON Schema +
validation), `worker/` (claim, run, heartbeat, receipt), `plugins/{agent,source,channel,delivery}`, `cli/`.

## The four mechanisms worth stealing

### 1. Suspend at zero cost

A run that needs a human decision **ends**. The open card lives in SQLite. No process, no tokens, survives
restarts. The answer is a new event that dispatches a new run carrying the old `session` as a hint.

SuperBot already behaves this way (a `question` card ends the turn; the answer is a new message that resumes
`bot.session_id`). What we lack is the hard part: the queued/waiting state is durable and retried there, and a
card can be resolved by a *later* run when the question became irrelevant.

### 2. Event sourcing, committed before you answer

Every accepted input produces facts; `decide(view, input, now, env)` is pure and returns facts + response;
`evolve(view, facts)` is the only writer of the projection; both plus the `due` index ride one SQLite
`WAL + FULL` transaction, and the HTTP reply is sent only after commit. Fingerprint of the format files decides
whether to rebuild the projection from the log at boot. Result: killed processes lose nothing acknowledged,
any run can be explained (`why <run>`), no migration scripts.

### 3. Instruction/data separation

External text (issue title/body) never enters the prompt. The prompt carries only validated identifiers: event
type, id, link. The body is written to a file and labelled as data. Which agent, which directory, which
permissions come **only** from local rules; an event that carries `cwd`, `agent`, or `access` is rejected
outright. `subject` is restricted to `[A-Za-z0-9#._:/@-]`, links to printable ASCII URLs.

This is the piece with the highest security payoff for us. Today we paste peer-Bot text, routine text, and
eventually email/issue text straight into the prompt.

### 4. Hop budget (TTL for work)

Receipt-generated events force `hops = max(hops of this run's events) + 1`; the agent cannot lower it; answers and
`later` timers inherit it. Above 8: record, don't dispatch, open a notice card. Our Bots can ping-pong
`botctl ask` forever today.

## Other design details that are cheap to copy

- **Receipt** is the agent's last message: one line of outcome, then an optional `agent-autoloop` fenced JSON
  with `ask / notice / resolve / later / events`. Bad JSON → re-prompt *only the receipt* in the same session,
  at most twice; never redo the business work. No session → keep the original outcome, drop the receipt, tell
  the owner.
- **Cards** are deduped by `(thread, key)` while open; `later` is an idempotent per-thread timer key; `resolve`
  may only close cards from the same group that existed before the run claimed work.
- **Delivery ledger** per `(card, channel)` with `version / ref / tries / given_up`: edit instead of resend,
  exponential resend for one hour, give up loudly.
- **Session semantics** are precise: resume if the harness merely errored; start fresh only when the harness says
  the session does not exist; if it resumed and died mid-turn, retry **in that session** so work is not redone.
  `started` (prompt accepted) is what makes that distinction possible.
- **Retries**: 5 min, doubling, cap 1 h; events arriving meanwhile merge into the retrying run; after the 3rd
  consecutive failure tell the owner; after the 10th stop and cancel.
- **Pause/replay**: a paused rule skips (does not queue) new events and records them; resume lists what was
  skipped; `replay --since` runs each exactly once.
- **Testing**: seeded 200k-step simulation with two workers, two sources, injected crashes/lost responses/
  duplicate submits; exhaustive 8! orderings on one scenario; 1000 real SIGKILLs; mutation testing of the kernel.
  Correctness gates the build, speed is only an observation.

## SuperBot today vs that

| Concern | SuperBot now | agent-autoloop |
| --- | --- | --- |
| Work intake | user chat, `botctl ask`, cron per Bot | events from sources + cron, matched by rules |
| Queue | `VecDeque` in memory, `messages.status` in SQLite | `runs` table with states, claim tokens, leases |
| Restart | in-flight run abandoned, message marked done, user told to resend | run stays `claimed` → `unknown`, late result accepted, retried |
| Failure | error bubble, no retry | 5 min → 1 h backoff, 10 attempts, owner told at 3 |
| Output contract | trailing ```` ```question ```` block, title + options only | receipt: ask/notice/resolve/later/events with keys |
| Cards | `messages.question`, no state, can't be closed by the Bot | `cards.state`, dedupe by key, resolve, fyi notices |
| Reasons | none | fact log, `why <run>` |
| Bot-to-Bot | `botctl ask` with timeout, no budget | events with hop budget, answer/later inherit depth |
| Prompt input | peer/routine text pasted into the prompt | identifiers in prompt, bodies in a data file |
| Tests | unit tests for cron/adapters | simulation, exhaustive ordering, SIGKILL, mutation |

## How it maps onto our architecture

Keep our shape: one Rust binary, web app, Bots with memory, harness adapters, one shared computer. Take the
*kernel semantics*, not the CQRS machinery wholesale.

| Their word | Ours | Where it lives |
| --- | --- | --- |
| event | `Event { source, key, type, subject, summary, detail, link, hops }` | new `events` table |
| rule | `Rule { when, bot, cwd, prompt, skill, parallel, resume, tell }` — generalises today's `routines` | new `rules` table (`routines` becomes one rule kind) |
| thread | `(bot_id, subject)` — the durable "one piece of work" | `threads` table or derived |
| run | `Run { state, attempts, not_before, session, claim_token, receipt }` | replaces `messages.status` as the queue |
| receipt | `Receipt { outcome, session, usage, ask, notice, resolve, later, events }` | column on `runs`, parsed from the last message |
| card | `Card { key, state, choice, resolve, fyi }` | new `cards` table (upgrade of `messages.question`) |
| timer | `Timer { rule, lane, key, due_at }` | new `timers` table |
| channel | the web app (always online), plus `botctl` | unchanged, simpler than their delivery ledger |

### Phase 1 — durable runs + receipts (biggest win per line of code)

1. `runs` table + `claim`/`finish`/`retry`: replace the in-memory `VecDeque` in `server/src/runner.rs`; keep
   one run at a time per Bot; `not_before` + attempts; on boot, `claimed` → resume or `unknown`, never "send it
   again".
2. Receipt parsing: extend `split_question` in `runner.rs` into a `receipt` module; accept
   `ask / notice / resolve / later / events` with `key`, keep the old ```` ```question ```` block as sugar.
3. `cards` table with `key`, `state`, `choice`, `resolve(fyi)`, and a `later` timer that re-dispatches to the same
   thread with the old session.
4. Session semantics: distinguish "harness started then died" from "session unknown"; retry in-session first.
   Adapters need a `started` signal (all three CLIs can give one: first JSON event).
5. Receipt repair: if the block is malformed, one cheap follow-up run in the same session telling it to re-emit
   only the block.

Ships: nothing lost on restart, automatic retries, Bots that close their own stale cards, "check back in 30 min"
without a cron.

### Phase 2 — events + rules (turns us from chat app into an ops desk)

1. `events` table with `(source, key)` dedupe; a `POST /api/events` endpoint with a per-source token; a cron
   source; a GitHub webhook source (mirror `plugins/github/source.ts`, verify HMAC).
2. Rules: match `when: {cron|source+types}` → `bot`; per-thread `parallel`; `resume: true`; `tell: needed|never`.
   Keep routines as `when: {cron}` rules so nothing breaks.
3. Routing: event → thread (`bot+subject`) → run. Events that arrive mid-run queue for the next run of that thread.
4. Hops: column on `events`, +1 on receipts, refuse > 8 with a notice card. Fixes Bot ping-pong.
5. Prompt hardening: `subject`/`link` charset checks; event bodies written to
   `data/bots/<id>/inbox/<event-id>.md` and referenced by path, not inlined.

### Phase 3 — explainability (optional, do it before we have paying users)

Either the full fact log (`facts` append-only + projection rebuild) or the cheap 80%: a `run_facts` timeline
table (`run_id, at, kind, detail`) and `botctl why <run>`. I would start with the timeline table; it answers
"why did this run happen, what did it do" for a fraction of the risk.

### Phase 4 — testing

Port the simulation harness idea, in Rust: a fake harness, fake clock, seeded fault injection
(duplicate delivery, lost response, kill mid-run), invariant checks after every step. This is what keeps phases
1–2 from rotting.

## What not to copy

- **No UI.** Our chat + live screen is the product. Their cards are the only surface; ours are chat messages.
- **`rules.json` as the only config.** We have a DB and a UI; rules should be rows that Bots can edit with
  `botctl`, with the file as an export/import format at most.
- **In-process TS plugins.** Our adapters and sources stay compiled-in Rust; a webhook source is a route, not a
  loaded module.
- **Full event sourcing on day one.** It is the right long-term answer and the wrong first step for a 2.4k-line
  server; the semantics matter more than the storage.
- **Delivery pipeline / PR plugin.** Different product.

## Notable cross-link

Their ACP bridge (`plugins/acp/agent.ts`) already drives `opencode acp`, and opencode is installed and working
headlessly here (see the local opencode notes). If we want a fourth adapter, that bridge is a working reference
for session resume and permission mapping — and this repo is where the "session is a hint, never a promise"
rule is written down.

## Recommendation

Write ADR 0004 covering Phase 1 (durable runs + receipts) and the hop budget, implement it inside
`server/src/` as a `kernel/` module with `runs`, `cards`, and `timers` tables, and leave event sourcing and
sources for a second ADR once Phase 1 is running in production.
