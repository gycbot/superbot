# Computer: each Bot's screen

Each Bot gets its own screen on the server, with a real browser. You can watch it live in the web app and take over.

## How it works

- **Screen:** a virtual display (Xvfb) with a window manager (Openbox) and its own D-Bus session. `botd` starts it on a Bot's first task and stops it after `SCREEN_IDLE_MINUTES` without use. Code: `server/src/screen.rs`.
- **Browser:** [agent-browser](https://github.com/vercel-labs/agent-browser). Each Bot has its own session and Chrome profile in `DATA_DIR/bots/<id>/browser`, so sign-ins persist. Chrome runs headed on the Bot's screen.
- **Desktop apps:** [agent-desktop](https://github.com/vercel-labs/agent-desktop) reads apps through the screen's accessibility bus.
- **Live view:** [voxl](https://github.com/tembo/voxl) streams each screen. `GET /api/bots/<id>/screen` returns `/screen/<port>/?token=...`. The page is view-only until you click **Take over**.
- **Handoff:** for logins, 2FA, or CAPTCHAs, a Bot runs `botctl handoff "<what it needs>"`. You get a card with **Open screen** and **Done**. The Bot's command waits for your answer.

All tools are plain CLIs, so this works the same on every harness. The Bot's system prompt explains them.

At most `MAX_SCREENS` run at once. When all are in use, the least recently used screen of an idle Bot is stopped.

Screens are work surfaces, not security boundaries. All Bots run as one Linux user.

## Install

On Ubuntu 24.04 or later, as root:

```sh
deploy/install-computer.sh <user that runs botd>
```

Then add `AGENT_BROWSER_SKILLS_DIR=/opt/computer/share/agent-browser-skills` to botd's environment and restart it. It prints `Screens: on.`

The script also adds 4 GB of swap and earlyoom, which kills the biggest Chrome before the server runs out of memory.

## Proxy

`botd` proxies `/screen/<port>/` to that local voxl port itself, WebSocket upgrade
included, so the live view works through the same port or tunnel as the web app.
Only ports 6800 to 6999 are proxied. Ports are `6800 + SCREEN_BASE + slot`: 6901,
6902, ... by default. voxl checks its own token, and the app's login still guards
the proxy route.

No Caddy or nginx rule is needed. If you do put one in front, pass the path through
unchanged, and don't buffer or rewrite `/screen/`.

## Sizing

One Bot with Chrome open uses about 0.5 to 1 GB of RAM. Streaming a busy screen uses about one CPU core. A 2 CPU, 8 GB server handles about 3 screens at once.
