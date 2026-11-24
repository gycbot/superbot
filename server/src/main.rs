//! botd: the server. Owns Bots, chats, memory files, and routines; runs each Bot
//! on its harness (Claude Code, Codex, or Pi); serves the web app and its API.

mod adapters;
mod auth_session;
mod cron;
mod db;
mod runner;
mod screen;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use db::{BotPatch, Db};
use runner::{App, Config};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

type AppState = Arc<App>;

static PASSWORD: OnceLock<String> = OnceLock::new();
static WEB_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

const COLORS: [&str; 11] = ["#f5f5f5", "#9a6b3f", "#e5484d", "#e8702e", "#e9a23b", "#4cc06a", "#4fb5a5", "#3b82f6", "#8b5cf6", "#e64a9c", "#7a7a7a"];
const SHAPES: [&str; 12] = ["waddle", "drop", "cloud", "bean", "onigiri", "peanut", "pebble", "square", "jelly", "gem", "gear", "heart"];

fn env(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.into())
}

pub fn random_bytes(n: usize) -> Vec<u8> {
    use std::io::Read;
    let mut b = vec![0u8; n];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).expect("read /dev/urandom");
    b
}

pub fn short_id() -> String {
    random_bytes(4).iter().map(|x| format!("{x:02x}")).collect()
}

fn pick<'a>(a: &[&'a str]) -> &'a str {
    a[random_bytes(1)[0] as usize % a.len()]
}

#[tokio::main]
async fn main() {
    let port: u16 = env("PORT", "3000").parse().expect("PORT must be a number");
    let host = env("HOST", "127.0.0.1");
    let password = env("AUTH_PASSWORD", "");
    let default_harness = env("DEFAULT_HARNESS", "claude");
    let default_model = env("DEFAULT_MODEL", "");
    if password.is_empty() && !["127.0.0.1", "localhost", "::1"].contains(&host.as_str()) {
        eprintln!("Refusing to listen on a public address without AUTH_PASSWORD.");
        std::process::exit(1);
    }
    if adapters::get(&default_harness).is_none() {
        eprintln!("DEFAULT_HARNESS must be one of: {}", adapters::NAMES.join(", "));
        std::process::exit(1);
    }
    PASSWORD.set(password).unwrap();
    WEB_DIR.set(std::env::var("WEB_DIR").ok().map(PathBuf::from)).unwrap();

    let abs = |p: String| std::path::absolute(p).expect("bad path");
    let data_dir = abs(env("DATA_DIR", "data"));
    let workspace = abs(env("WORKSPACE_DIR", "workspace"));
    std::fs::create_dir_all(&data_dir).expect("create DATA_DIR");
    std::fs::create_dir_all(&workspace).expect("create WORKSPACE_DIR");
    auth_session::init(&data_dir);
    let db = Db::open(&data_dir.join("app.sqlite")).expect("open database");

    let screens = screen::Screens::from_env(data_dir.join("screens"));
    let app = App::new(db, Config {
        data_dir,
        workspace,
        user_name: env("USER_NAME", "the user"),
        // `botctl` ships next to `botd`.
        bin_dir: std::env::current_exe().unwrap().parent().unwrap().to_path_buf(),
        api_url: format!("http://127.0.0.1:{port}"),
        bot_token: random_bytes(24).iter().map(|x| format!("{x:02x}")).collect(),
        run_timeout: Duration::from_secs(60 * env("RUN_TIMEOUT_MINUTES", "60").parse::<u64>().unwrap_or(60)),
        default_harness,
        default_model: if default_model.is_empty() { None } else { Some(default_model) },
    }, screens);

    // First start: create the primary Bot.
    if app.db.bots().is_empty() {
        let name = env("PRIMARY_BOT_NAME", "Assistant");
        let bot = app.db.create_bot(&name, "#f5f5f5", "waddle", &app.cfg.default_harness, app.cfg.default_model.as_deref(), true);
        app.init_bot_files(&bot);
    }

    let api = Router::new()
        .route("/state", get(state))
        .route("/events", get(events))
        .route("/harnesses/{name}/models", get(list_models))
        .route("/bots", get(list_bots).post(create_bot))
        .route("/bots/{id}", patch(update_bot).delete(delete_bot))
        .route("/bots/{id}/messages", get(list_messages).post(send_message))
        .route("/bots/{id}/read", post(mark_read))
        .route("/bots/{id}/stop", post(stop_bot))
        .route("/bots/{id}/routines", get(list_routines).post(create_routine))
        .route("/routines/{id}", patch(update_routine).delete(delete_routine))
        .route("/routines/{id}/run", post(run_routine))
        .route("/bots/{id}/runs", get(list_runs))
        .route("/runs/{id}", get(run_detail))
        .route("/bots/{id}/screen", get(screen_info))
        .route("/upload", post(upload))
        .route("/ask", post(ask))
        .route("/handoff", post(handoff));
    let router = Router::new()
        .route("/screen/{*path}", get(screen_proxy))
        .route("/attachments/{id}", get(attachment))
        .nest("/api", api)
        .fallback(get(web))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), auth))
        .route("/healthz", get(|| async { Json(json!({ "ok": true })) }))
        .with_state(app.clone());

    app.recover();
    app.start_scheduler();
    cron::start(app.clone());
    if app.screens.is_some() {
        let app = app.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                app.screens.as_ref().unwrap().reap(|id| app.is_running(id)).await;
            }
        });
    }

    let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await.expect("bind");
    println!("Listening on http://{host}:{port}");
    println!("Harnesses on PATH: {}. Default: {}.", adapters::installed().join(", "), app.cfg.default_harness);
    println!("Screens: {}.", if app.screens.is_some() { "on" } else { "off (install Xvfb and openbox to turn them on)" });

    // On SIGTERM/SIGINT: stop agents (queued work runs after restart), then exit.
    let shutdown_app = app.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let (mut term, mut int) = (signal(SignalKind::terminate()).unwrap(), signal(SignalKind::interrupt()).unwrap());
        tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
        shutdown_app.shutdown();
        for _ in 0..60 {
            if shutdown_app.is_idle() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        std::process::exit(0);
    });

    axum::serve(listener, router).await.unwrap();
}

// Errors

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

fn not_found(what: &str) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, format!("{what} not found"))
}

type Res<T = Json<Value>> = Result<T, ApiError>;

// Auth: browser login with AUTH_PASSWORD (any username) issues a 30-day signed
// cookie; `botctl` uses the Bots' Bearer token.

async fn auth(State(app): State<AppState>, req: Request, next: Next) -> Response {
    let h = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let cookie = req.headers().get(header::COOKIE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let password = PASSWORD.get().unwrap();
    let basic_ok = h.strip_prefix("Basic ").and_then(base64_decode).is_some_and(|creds| {
        let pass = creds.splitn(2, |b| *b == b':').nth(1).unwrap_or_default();
        same(pass, password.as_bytes())
    });
    let ok = password.is_empty()
        || h.strip_prefix("Bearer ").is_some_and(|t| same(t.as_bytes(), app.cfg.bot_token.as_bytes()))
        || basic_ok
        || auth_session::cookie_ok(&cookie, password);
    if ok {
        let mut res = next.run(req).await;
        if basic_ok {
            if let Ok(v) = header::HeaderValue::from_str(&auth_session::set_cookie_header(password)) {
                res.headers_mut().append(header::SET_COOKIE, v);
            }
        }
        res
    } else {
        (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Basic realm=\"Bots\"")], "Login required").into_response()
    }
}

/// Constant-time comparison.
pub(crate) fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut out, mut buf, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.trim().bytes().filter(|c| *c != b'=') {
        buf = (buf << 6) | T.iter().position(|x| *x == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

// Web app: embedded in the binary. Set WEB_DIR to serve from disk while editing it.

// Screens: the live view is served from a local voxl port. Proxy it here so the
// web app works without a reverse proxy in front, on whatever port or tunnel it
// is reached through. Only the screen port range is proxied, and the connection
// is upgraded in place for voxl's WebSocket.
async fn screen_proxy(Path(path): Path<String>, mut req: Request) -> Response {
    let (port, rest) = match path.split_once('/') {
        Some((p, r)) => (p, r),
        None => (path.as_str(), ""),
    };
    let port: u16 = match port.parse() {
        Ok(p) if (6800..7000).contains(&p) => p,
        _ => return (StatusCode::NOT_FOUND, "Not found").into_response(),
    };
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let uri = format!("http://127.0.0.1:{port}/{rest}{query}");

    let stream = match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_GATEWAY, "That screen is not running.").into_response(),
    };
    let (mut sender, conn) = match hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream)).await {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_GATEWAY, "That screen is not running.").into_response(),
    };
    tokio::spawn(async move {
        let _ = conn.await;
    });

    // Held before the body is taken so a WebSocket upgrade can be relayed.
    let down = hyper::upgrade::on(&mut req);
    let mut up = Request::builder().method(req.method()).uri(&uri);
    for (k, v) in req.headers() {
        up = up.header(k, v);
    }
    let up = match up.body(req.into_body()) {
        Ok(r) => r,
        Err(_) => return (StatusCode::BAD_REQUEST, "Bad request").into_response(),
    };
    let mut res = match sender.send_request(up).await {
        Ok(r) => r,
        Err(_) => return (StatusCode::BAD_GATEWAY, "That screen is not running.").into_response(),
    };
    if res.status() == StatusCode::SWITCHING_PROTOCOLS {
        let upg = hyper::upgrade::on(&mut res);
        tokio::spawn(async move {
            if let (Ok(a), Ok(b)) = tokio::join!(down, upg) {
                let (mut a, mut b) = (hyper_util::rt::TokioIo::new(a), hyper_util::rt::TokioIo::new(b));
                let _ = tokio::io::copy_bidirectional(&mut a, &mut b).await;
            }
        });
    }
    let (parts, body) = res.into_parts();
    Response::from_parts(parts, axum::body::Body::new(body))
}

async fn web(uri: axum::http::Uri) -> Response {
    let path = uri.path();
    let file = match path {
        "/" | "/index.html" => "index.html",
        "/app.js" => "app.js",
        "/styles.css" => "styles.css",
        _ => return (StatusCode::NOT_FOUND, "Not found").into_response(),
    };
    let kind = match file.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        _ => "text/css; charset=utf-8",
    };
    let body = match WEB_DIR.get().unwrap() {
        Some(dir) => std::fs::read_to_string(dir.join(file)).unwrap_or_default(),
        None => match file {
            "index.html" => include_str!("../../web/index.html"),
            "app.js" => include_str!("../../web/app.js"),
            _ => include_str!("../../web/styles.css"),
        }
        .to_string(),
    };
    ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}

// Live updates: one Server-Sent Events stream with everything.

async fn events(State(app): State<AppState>) -> impl IntoResponse {
    let stream = BroadcastStream::new(app.tx.subscribe()).map(|r| {
        // A slow client that missed events is told to reload its state.
        Ok::<_, Infallible>(Event::default().data(r.unwrap_or_else(|_| r#"{"type":"resync"}"#.into())))
    });
    ([("x-accel-buffering", "no")], Sse::new(stream).keep_alive(KeepAlive::default()))
}

// Bots

async fn state(State(app): State<AppState>) -> Json<Value> {
    let bots: Vec<Value> = app.db.bots().iter().map(|b| app.bot_view(b)).collect();
    Json(json!({
        "me": { "name": env("USER_NAME", "You") },
        "harnesses": adapters::installed(),
        "defaultHarness": app.cfg.default_harness,
        "bots": bots,
    }))
}

/// Models and reasoning efforts the harness offers on this server.
async fn list_models(Path(name): Path<String>) -> Res {
    if adapters::get(&name).is_none() {
        return Err(not_found("Harness"));
    }
    let list = tokio::task::spawn_blocking(move || adapters::models(&name)).await.unwrap_or_default();
    Ok(Json(json!(list)))
}

async fn list_bots(State(app): State<AppState>) -> Json<Value> {
    Json(app.db.bots().iter().map(|b| app.bot_view(b)).collect())
}

fn need_bot(app: &App, id: &str) -> Res<db::Bot> {
    app.db.bot(id).ok_or_else(|| not_found("Bot"))
}

fn bot_patch(v: &Value) -> Res<BotPatch> {
    let mut p = BotPatch::default();
    if let Some(n) = v["name"].as_str().map(str::trim).filter(|n| !n.is_empty()) {
        p.name = Some(n.chars().take(60).collect());
    }
    if let Some(c) = v["color"].as_str().filter(|c| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())) {
        p.color = Some(c.into());
    }
    if let Some(s) = v["shape"].as_str().filter(|s| SHAPES.contains(s)) {
        p.shape = Some(s.into());
    }
    if let Some(h) = v["hidden"].as_bool() {
        p.hidden = Some(h);
    }
    match &v["model"] {
        Value::Null if v.get("model").is_some() => p.model = Some(None),
        Value::String(m) => p.model = Some(Some(m.trim().to_string()).filter(|m| !m.is_empty())),
        _ => {}
    }
    // Effort words like "high" or "xhigh". Null = the harness default.
    match &v["effort"] {
        Value::Null if v.get("effort").is_some() => p.effort = Some(None),
        Value::String(e) if e.is_empty() => p.effort = Some(None),
        Value::String(e) if e.len() <= 20 && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => {
            p.effort = Some(Some(e.clone()))
        }
        Value::String(_) => return Err(bad("Invalid effort")),
        _ => {}
    }
    if let Some(h) = v.get("harness") {
        let h = h.as_str().filter(|h| adapters::get(h).is_some());
        let h = h.ok_or_else(|| bad(format!("Harness must be one of: {}", adapters::NAMES.join(", "))))?;
        p.harness = Some(h.into());
    }
    Ok(p)
}

async fn create_bot(State(app): State<AppState>, Json(v): Json<Value>) -> Res<(StatusCode, Json<Value>)> {
    let p = bot_patch(&v)?;
    let name = p.name.ok_or_else(|| bad("Name is required"))?;
    let harness = p.harness.unwrap_or_else(|| app.cfg.default_harness.clone());
    let model = p.model.flatten().or_else(|| app.cfg.default_model.clone());
    let bot = app.db.create_bot(
        &name,
        p.color.as_deref().unwrap_or(pick(&COLORS)),
        p.shape.as_deref().unwrap_or(pick(&SHAPES)),
        &harness,
        model.as_deref(),
        false,
    );
    app.init_bot_files(&bot);
    app.push_bot(&bot.id);
    Ok((StatusCode::CREATED, Json(app.bot_view(&bot))))
}

async fn update_bot(State(app): State<AppState>, Path(id): Path<String>, Json(v): Json<Value>) -> Res {
    let bot = need_bot(&app, &id)?;
    let mut p = bot_patch(&v)?;
    if p.harness.as_ref().is_some_and(|h| *h != bot.harness) {
        p.session_id = Some(None); // sessions don't carry across harnesses
    }
    app.db.update_bot(&id, p);
    app.push_bot(&id);
    Ok(Json(app.bot_view(&need_bot(&app, &id)?)))
}

async fn delete_bot(State(app): State<AppState>, Path(id): Path<String>) -> Res {
    if need_bot(&app, &id)?.primary {
        return Err(bad("The primary Bot can't be deleted"));
    }
    app.stop(&id);
    if let Some(screens) = &app.screens {
        screens.remove(&id).await;
    }
    app.db.delete_bot(&id);
    app.send(json!({ "type": "botDeleted", "botId": id }));
    Ok(Json(json!({ "ok": true })))
}

async fn mark_read(State(app): State<AppState>, Path(id): Path<String>) -> Res {
    need_bot(&app, &id)?;
    app.db.update_bot(&id, BotPatch { unread: Some(false), ..Default::default() });
    app.push_bot(&id);
    Ok(Json(json!({ "ok": true })))
}

async fn stop_bot(State(app): State<AppState>, Path(id): Path<String>) -> Res {
    need_bot(&app, &id)?;
    app.stop(&id);
    Ok(Json(json!({ "ok": true })))
}

// Messages

async fn list_messages(State(app): State<AppState>, Path(id): Path<String>) -> Res {
    need_bot(&app, &id)?;
    Ok(Json(json!(app.db.messages(&id, 200))))
}

async fn send_message(State(app): State<AppState>, Path(id): Path<String>, Json(v): Json<Value>) -> Res<(StatusCode, Json<Value>)> {
    need_bot(&app, &id)?;
    let text = v["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| bad("Text is required"))?;
    let text: String = text.chars().take(100_000).collect();
    // Answering a choice card: remember which option was picked.
    if let (Some(mid), Some(opt)) = (v["answer"]["messageId"].as_i64(), v["answer"]["option"].as_u64()) {
        if let Some(mut q) = app.db.message(mid).filter(|m| m.bot_id == id).and_then(|m| m.question) {
            q["picked"] = json!(opt);
            app.db.set_question(mid, &q);
        }
    }
    // Answering a handoff card the Bot is still waiting on: hand the answer to it directly.
    if let Some(tx) = v["answer"]["messageId"].as_i64().and_then(|mid| app.take_handoff(mid)) {
        let msg = app.db.add_message(&id, "user", &text, None, false, Some("done"));
        app.push_message(&msg);
        let _ = tx.send(text);
        return Ok((StatusCode::ACCEPTED, Json(json!(msg))));
    }
    // Optional attachments uploaded beforehand via POST /api/upload.
    let attachments = v["attachments"].as_array().filter(|a| !a.is_empty()).map(|a| Value::Array(a.clone()));
    let msg = app.db.add_message_full(&id, "user", &text, None, false, Some("queued"), None, attachments.as_ref());
    app.push_message(&msg);
    app.enqueue(msg.clone());
    Ok((StatusCode::ACCEPTED, Json(json!(msg))))
}

/// Upload one image/video/file. Raw body with X-File-Name (base64) + Content-Type.
/// Stored under DATA_DIR/uploads and returns attachment descriptors for messages.
async fn upload(State(app): State<AppState>, headers: HeaderMap, body: Bytes) -> Res {
    if body.is_empty() { return Err(bad("Empty file")); }
    if body.len() > 50 * 1024 * 1024 { return Err(bad("File too large (max 50 MB)")); }
    let name = headers.get("x-file-name")
        .and_then(|v| v.to_str().ok())
        .and_then(base64_decode)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_else(|| "file".into());
    let mime = headers.get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .split(';').next().unwrap_or("application/octet-stream").trim().to_string();
    let kind = if mime.starts_with("image/") { "image" }
        else if mime.starts_with("video/") { "video" } else { "file" };
    let ext = match mime.as_str() {
        "image/png" => "png", "image/jpeg" => "jpg", "image/gif" => "gif",
        "image/webp" => "webp", "image/svg+xml" => "svg", "image/x-icon" => "ico",
        "video/mp4" => "mp4", "video/webm" => "webm", "video/quicktime" => "mov",
        _ => "bin",
    };
    let id: String = random_bytes(16).iter().map(|b| format!("{b:02x}")).collect();
    let dir = app.cfg.data_dir.join("uploads");
    std::fs::create_dir_all(&dir).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    std::fs::write(dir.join(format!("{id}.{ext}")), &body)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!([{ "id": id, "name": name, "mime": mime, "kind": kind }])))
}

/// Serve an uploaded attachment. No SNI needed; Basic auth applies (same as the app).
async fn attachment(State(app): State<AppState>, Path(id): Path<String>) -> Response {
    if !id.chars().all(|c| c.is_ascii_hexdigit()) || id.is_empty() {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    let dir = app.cfg.data_dir.join("uploads");
    let found = std::fs::read_dir(&dir).ok().and_then(|rd| {
        rd.flatten().find(|e| e.file_name().to_string_lossy().starts_with(&format!("{id}."))).map(|e| e.path())
    });
    let Some(path) = found else { return (StatusCode::NOT_FOUND, "Not found").into_response() };
    let bytes = match std::fs::read(&path) { Ok(b) => b, Err(_) => return (StatusCode::NOT_FOUND, "Not found").into_response() };
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("bin");
    let mime = match ext {
        "png" => "image/png", "jpg" => "image/jpeg", "gif" => "image/gif",
        "webp" => "image/webp", "svg" => "image/svg+xml", "ico" => "image/x-icon",
        "mp4" => "video/mp4", "webm" => "video/webm", "mov" => "video/quicktime",
        _ => "application/octet-stream",
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(axum::body::Body::from(bytes))
        .unwrap()
}

// Bot to Bot: `botctl ask`. Waits for the reply and returns it as plain text.

async fn ask(State(app): State<AppState>, Json(v): Json<Value>) -> Result<String, ApiError> {
    let from = v["from"].as_str().and_then(|id| app.db.bot(id)).ok_or_else(|| bad("Unknown asking Bot (BOT_ID)"))?;
    let to_key = v["to"].as_str().map(str::trim).unwrap_or("");
    let bots = app.db.bots();
    let to = bots
        .iter()
        .find(|b| b.id == to_key)
        .or_else(|| bots.iter().find(|b| b.name.eq_ignore_ascii_case(to_key)))
        .ok_or_else(|| bad(format!("No Bot named \"{to_key}\". Run `botctl bots` to list them.")))?;
    if to.id == from.id {
        return Err(bad("You can't ask yourself"));
    }
    let text = v["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| bad("Message is required"))?;
    let wait = Duration::from_secs(9 * 60); // under the 10 minute max most harnesses allow a command
    Ok(match app.ask(&from, to, text, wait).await {
        Some(reply) => reply,
        None => format!("{} is still working on it. Their reply will arrive later as a new message.", to.name),
    })
}

// Screens: the live view of a Bot's screen. Opening it starts the screen.

async fn screen_info(State(app): State<AppState>, Path(id): Path<String>) -> Res {
    need_bot(&app, &id)?;
    let off = || ApiError(StatusCode::NOT_FOUND, "Screens are off on this server. Install Xvfb, openbox, and voxl.".into());
    let screens = app.screens.as_ref().filter(|_| screen::on_path("voxl")).ok_or_else(off)?;
    let info = screens.ensure(&id, |b| app.is_running(b)).await;
    let info = info.ok_or_else(|| ApiError(StatusCode::SERVICE_UNAVAILABLE, "All screens are in use. Try again when a Bot finishes.".into()))?;
    // The proxy in front serves /screen/<port>/ from that local voxl port.
    Ok(Json(json!({ "url": format!("/screen/{}/?token={}", info.port, info.token) })))
}

// A Bot hands its screen to the user: `botctl handoff`. Waits for "Done".

async fn handoff(State(app): State<AppState>, Json(v): Json<Value>) -> Result<String, ApiError> {
    let bot = v["from"].as_str().and_then(|id| app.db.bot(id)).ok_or_else(|| bad("Unknown Bot (BOT_ID)"))?;
    let text = v["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| bad("Say what you need"))?;
    let text: String = text.chars().take(300).collect();
    let wait = Duration::from_secs(9 * 60); // under the 10 minute max most harnesses allow a command
    Ok(match app.handoff(&bot, &text, wait).await {
        Some(answer) => format!("The user answered: {answer}"),
        None => "The user hasn't answered yet. Stop here and tell them in one line what you need. You'll get their answer as a new message.".into(),
    })
}

// Runs: what each Bot did, and why. `botctl runs` and `botctl why` read these.

fn wants_text(h: &HeaderMap) -> bool {
    h.get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|a| a.contains("text/plain"))
}

fn stamp(ms: Option<i64>) -> String {
    ms.and_then(chrono::DateTime::from_timestamp_millis)
        .map(|t| t.with_timezone(&chrono::Local).format("%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

async fn list_runs(State(app): State<AppState>, Path(id): Path<String>, headers: HeaderMap) -> Res<Response> {
    need_bot(&app, &id)?;
    let runs = app.db.runs(&id, 50);
    if !wants_text(&headers) {
        return Ok(Json(json!(runs)).into_response());
    }
    let mut out = String::new();
    for r in &runs {
        let note = r.error.as_deref().map(|e| e.lines().next().unwrap_or("")).unwrap_or("");
        out.push_str(&format!(
            "#{id} {state:<10} {kind:<8} attempt {n}  {at}  {note}\n",
            id = r.id,
            state = r.state,
            kind = r.kind,
            n = r.attempts,
            at = stamp(Some(r.created_at)),
        ));
    }
    if out.is_empty() {
        out.push_str("No runs yet.\n");
    }
    Ok(out.into_response())
}

async fn run_detail(State(app): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> Res<Response> {
    let run = app.db.run(id).ok_or_else(|| not_found("Run"))?;
    let facts = app.db.facts(id);
    if !wants_text(&headers) {
        return Ok(Json(json!({ "run": run, "facts": facts })).into_response());
    }
    let mut out = format!(
        "Run #{id} · {state} · {kind} · attempt {n} · created {at}\n",
        id = run.id,
        state = run.state,
        kind = run.kind,
        n = run.attempts,
        at = stamp(Some(run.created_at)),
    );
    if let Some(e) = &run.error {
        out.push_str(&format!("  error: {}\n", first_line(e)));
    }
    for f in &facts {
        out.push_str(&format!("{}  {:<12} {}\n", stamp(Some(f.at)), f.kind, f.detail));
    }
    Ok(out.into_response())
}

fn first_line(s: &str) -> String {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().chars().take(300).collect()
}

// Routines

async fn list_routines(State(app): State<AppState>, Path(id): Path<String>) -> Res {
    need_bot(&app, &id)?;
    Ok(Json(json!(app.db.routines(Some(&id)))))
}

async fn create_routine(State(app): State<AppState>, Path(id): Path<String>, Json(v): Json<Value>) -> Res<(StatusCode, Json<Value>)> {
    need_bot(&app, &id)?;
    let field = |k: &str| v[k].as_str().map(str::trim).filter(|s| !s.is_empty());
    let (Some(name), Some(cron_expr), Some(instruction)) = (field("name"), field("cron"), field("instruction")) else {
        return Err(bad("name, cron, and instruction are required"));
    };
    cron::parse(cron_expr).map_err(bad)?;
    Ok((StatusCode::CREATED, Json(json!(app.db.add_routine(&id, name, cron_expr, instruction)))))
}

async fn update_routine(State(app): State<AppState>, Path(id): Path<String>, Json(v): Json<Value>) -> Res {
    app.db.routine(&id).ok_or_else(|| not_found("Routine"))?;
    let cron_expr = v["cron"].as_str();
    if let Some(c) = cron_expr {
        cron::parse(c).map_err(bad)?;
    }
    app.db.update_routine(&id, v["name"].as_str(), cron_expr, v["instruction"].as_str(), v["enabled"].as_bool());
    Ok(Json(json!(app.db.routine(&id))))
}

/// Runs a routine now, the same way the scheduler does (the run shows in the Bot's chat).
async fn run_routine(State(app): State<AppState>, Path(id): Path<String>) -> Res {
    let r = app.db.routine(&id).ok_or_else(|| not_found("Routine"))?;
    let text = format!("{}: {}", r.name, r.instruction);
    let msg = app.db.add_message(&r.bot_id, "routine", &text, None, false, Some("queued"));
    app.push_message(&msg);
    app.enqueue(msg);
    Ok(Json(json!({ "ok": true })))
}

async fn delete_routine(State(app): State<AppState>, Path(id): Path<String>) -> Res {
    app.db.routine(&id).ok_or_else(|| not_found("Routine"))?;
    app.db.delete_routine(&id);
    Ok(Json(json!({ "ok": true })))
}
