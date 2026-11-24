# SuperBot on Windows

A Windows-native build of [SuperBot](../superbot) (MIT). The server, web app, adapters, and `botctl` are unchanged in behavior, except that the Linux-only screen feature is disabled. There is no remote desktop / live screen control in this build.

## Requirements

- Windows 10/11 x64
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload (C++ linker + Windows SDK)
- Rust stable MSVC (`rustup default stable-msvc`)
- One or more agent CLIs on `PATH`: `claude`, `codex`, or `pi`

## Build

```powershell
cargo build --release
```

Outputs:

- `target\release\botd.exe` - the server
- `target\release\botctl.exe` - the CLI the Bots use for routines and Bot-to-Bot asks

If crates.io is slow or blocked:

```powershell
$env:CARGO_HTTP_PROXY = "http://127.0.0.1:7897"
cargo build --release
```

## Run

```powershell
Copy-Item .env.example .env
notepad .env            # set USER_NAME, DEFAULT_HARNESS, and AUTH_PASSWORD if needed
.\run.ps1
```

Then open <http://127.0.0.1:3000>.

`run.ps1` reads `.env` into the process environment and starts `target\release\botd.exe`. Press `Ctrl+C` to stop; running tasks are marked interrupted, queued work resumes on the next start.

## Windows behavior

- **Screens / remote control: disabled.** Xvfb, openbox, and voxl are Linux/X11 only, so this build never starts them. `SCREENS` is ignored.
- **Harness resolution:** `claude`, `codex`, and `pi` are found via `PATH` and `PATHEXT`; `.cmd`/`.bat` shims are launched through `cmd /C`.
- **Process stop:** `taskkill /T /F` replaces Unix process groups.
- **Paths:** the Bots' `PATH` is joined with `;`; `botctl.exe` is used in prompts.
- **Time:** routines use the Windows local time zone. The `TZ` variable does not override it.
- **No systemd:** run it from a terminal, or wrap it with Task Scheduler / NSSM if you want it at boot.

## What was changed

See [`docs/adr/0004-windows.md`](docs/adr/0004-windows.md).

## License

MIT, same as upstream SuperBot. See [LICENSE](LICENSE).
