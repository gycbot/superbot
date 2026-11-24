# 0004: Durable runs replace the in-memory queue

Date: 2026-11-20. Status: accepted. Builds on [0001](0001-backend.md).

## Decision

Work is no longer a `VecDeque` in the server process. Each message becomes a row in `runs`, and a Bot's loop
claims one run at a time:

- `state`: `queued → claimed → done | failed | cancelled`
- `attempts`, `not_before` (backoff), `session` (resume hint), `started` (the harness accepted the prompt)
- `run_facts`: a short append-only timeline (`queued`, `claim`, `session-lost`, `retry`, `done`, `failed`,
  `recovered`), readable with `botctl why <run>`

Rules:

- **Claim is one SQL statement**, so two loops cannot pick the same run, and a Bot never runs two at once.
- **A restart requeues claims.** A run that had `started = 1` keeps its session; one that never started clears
  it. Re-running is at-least-once, so the prompt tells the Bot to check what is already done.
- **Retries are bounded**: 30 s, 1 min, 2 min, 5 min, then the run is left `failed`. No retry when the run
  already produced text, when the user stopped it, on a timeout, or on config/auth errors.
- **Messages from the previous version** (status `queued`/`running` with no run row) are queued once at the
  first start after the upgrade, so an upgrade loses nothing.

Schema changes go through `PRAGMA user_version` and an ordered list of SQL steps (`MIGRATIONS` in `db.rs`).

## Why

The old design lost in-flight work on every restart: `recover()` marked the interrupted message `done` and told
the user to send it again. With runs in SQLite the same failure becomes a retry in the same session.

We deliberately take only this slice of the design reviewed in `background/agent-autoloop.md`:

- taken: durable claim queue, attempts/backoff, session-resume rules, a per-run timeline;
- not taken yet: receipts (`ask`/`notice`/`resolve`/`later`), a `cards` state machine, an `events` intake with
  rules and a hop budget. Those are later ADRs;
- not taken at all: full event sourcing, a delivery ledger per channel, HTTP workers on several machines.

## Consequences

- `messages.status` stays as a display detail; `runs` is the truth.
- `botctl runs` / `botctl why` exist for Bots and for us.
- A run that keeps failing leaves one message in the chat, not four.
- Stopping a Bot now cancels its queued runs instead of silently dropping them.
- The queue survives `botd` being killed, including a kill during a run.
