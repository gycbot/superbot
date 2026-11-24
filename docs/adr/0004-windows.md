# ADR 0004: Windows-native build

## Status
Accepted (2026-11-23)

## Context
Upstream SuperBot targets Linux. The server hardcodes `/dev/urandom`, Unix signals, Unix process groups, an external `kill`, and `sh -c "command -v"`. Screens require Xvfb, openbox, and voxl (X11). We want a local Windows build without remote screen control.

## Decision
- Replace `/dev/urandom` with the cross-platform `getrandom` crate.
- Keep Unix signal handling under `cfg(unix)`; use `tokio::signal::ctrl_c()` on Windows.
- Keep `process_group(0)` under `cfg(unix)`; use `taskkill /T /F /PID` on Windows to stop a run.
- Resolve harness binaries with a PATH + `PATHEXT` lookup; run `.cmd`/`.bat` shims through `cmd /C`.
- Join the Bots' `PATH` with `std::env::join_paths`; refer to `botctl.exe` in prompts.
- Disable screens on Windows. No X11, no remote control. The related APIs return "screens are off".

## Consequences
- The Windows build has no live screen / remote takeover. Everything else (chat, memory, routines, Bot-to-Bot, attachments) works.
- Routines use the Windows local time zone; `TZ` is not honored.
- Upstream sync touches a small set of files: `main.rs`, `runner.rs`, `adapters/mod.rs`, `screen.rs`, `server/Cargo.toml`.
