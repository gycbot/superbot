// Thin client: all state lives on the server. Live updates come over /api/events.

// Bot avatars: soft "Squish" robots. One body color, plus a dark visor with two eyes.
// Each shape is (color, face) => SVG. face is the visor function, or none() for the bare silhouette.
const DARK = "#141414";
const rect = (x, y, w, h, rx, f) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${rx}" fill="${f}"/>`;
const circ = (cx, cy, r, f) => `<circle cx="${cx}" cy="${cy}" r="${r}" fill="${f}"/>`;
const ell = (cx, cy, rx, ry, f, rot = 0) => `<ellipse cx="${cx}" cy="${cy}" rx="${rx}" ry="${ry}" fill="${f}" transform="rotate(${rot} ${cx} ${cy})"/>`;
const path = (d, f, round = 0) => `<path d="${d}" fill="${f}"${round ? ` stroke="${f}" stroke-width="${round}" stroke-linejoin="round"` : ""}/>`;
const tilt = (deg, s) => `<g transform="rotate(${deg} 24 24)">${s}</g>`;
const nub = (x, y, h, f) => rect(x - 1.3, y - h, 2.6, h + 2, 1.3, f) + circ(x, y - h, 3, f);
const visor = (k) => (x, y, w, h) => {
  const s = h / 12, g = w * 0.22, eye = (ex) => rect(ex - 1.5 * s, y - 2.9 * s, 3 * s, 5.8 * s, 1.5 * s, k);
  return rect(x - w / 2, y - h / 2, w, h, h / 2, DARK) + eye(x - g) + eye(x + g);
};
const none = () => "";
const GEAR = Array.from({ length: 97 }, (_, i) => {
  const a = (i / 96) * Math.PI * 2, r = 18.5 + 2.6 * Math.cos(8 * a);
  return `${i ? "L" : "M"}${(24 + r * Math.sin(a)).toFixed(2)} ${(24 - r * Math.cos(a)).toFixed(2)}`;
}).join("") + "Z";

// Order matters: the New Bot window shows them in this order.
const SHAPES = {
  waddle: (k, f) => nub(24, 9, 3, k) + ell(16.5, 41, 5.5, 3, k) + ell(31.5, 41, 5.5, 3, k) + circ(24, 24, 16.5, k) + f(24, 24, 23, 12),
  drop: (k, f) => path("M24 6c9 0 15 12 17 22 2 10-6 15-17 15S5 38 7 28C9 18 15 6 24 6z", k) + f(24, 27, 22, 12),
  cloud: (k, f) => nub(26, 12, 6, k) + circ(15, 27, 10, k) + circ(26, 22, 12, k) + circ(35, 29, 9, k) + rect(5, 27, 38, 15, 7.5, k) + f(24, 30, 22, 11),
  bean: (k, f) => nub(20, 9, 5, k) + ell(24, 26, 20, 14, k, -12) + f(24, 26, 23, 12),
  onigiri: (k, f) => path("M24 9L41 38H7Z", k, 12) + f(24, 30, 20, 11),
  peanut: (k, f) => nub(24, 11, 4, k) + circ(15, 26, 12, k) + circ(33, 26, 12, k) + rect(15, 17, 18, 18, 0, k) + f(24, 25, 26, 12),
  pebble: (k, f) => path("M9 16C13 8 30 4 38 11s7 22-3 29-26 5-29-7c-1-6 1-13 3-17z", k) + f(25, 25, 23, 12),
  square: (k, f) => tilt(12, nub(24, 9, 4, k) + rect(8, 9, 32, 32, 12, k) + f(24, 25, 22, 12)),
  jelly: (k, f) => tilt(18, ell(24, 25, 13.5, 19, k) + f(24, 22, 19, 11)),
  gem: (k, f) => path("M24 7L40.5 18.5L34.5 40H13.5L7.5 18.5Z", k, 9) + f(24, 26, 22, 12),
  gear: (k, f) => path(GEAR, k) + f(24, 24, 24, 12),
  heart: (k, f) => path("M24 42L7 25A9.5 9.5 0 0 1 24 13A9.5 9.5 0 0 1 41 25Z", k, 6) + f(24, 25, 22, 11),
};
// Shapes from before the robots, so existing Bots keep a similar look.
const OLD_SHAPES = { round: "waddle", star: "gear", flower: "gear", scallop: "gear", splat: "gear", blob: "pebble", egg: "drop", wide: "bean", triangle: "onigiri", pentagon: "gem", hexagon: "gem" };

// ring: draw a selection outline that follows the shape.
function avatar(color, shape = "waddle", size = 48, ring = false) {
  const draw = SHAPES[shape] ?? SHAPES[OLD_SHAPES[shape]] ?? SHAPES.waddle;
  const scaled = (k, s) => `<g transform="translate(24 24) scale(${k}) translate(-24 -24)">${s}</g>`;
  const body = draw(color, visor(color));
  return `<svg class="avatar" viewBox="0 0 48 48" width="${size}" height="${size}" style="stroke:none;width:${size}px;height:${size}px;overflow:visible">
    ${ring ? scaled(1.1, draw("#4a4a4a", none)) + scaled(1.02, draw("#1f1f1f", none)) + scaled(.86, body) : body}
  </svg>`;
}

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

// Small, safe Markdown: escape first, then add formatting.
function md(src) {
  const blocks = [];
  let s = esc(src).replace(/```[^\n]*\n([\s\S]*?)```/g, (_, code) => `\u0000${blocks.push(code) - 1}\u0000`);
  const inline = (t) => t
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*\n]+)\*/g, "$1<em>$2</em>")
    .replace(/!\[([^\]]*)\]\((\/attachments\/[^)\s]+)\)/g, '<img src="$2" alt="$1" class="chat-media" />')
    .replace(/\[video\]\((\/attachments\/[^)\s]+)\)/g, '<video src="$1" controls class="chat-media" />')
    .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>')
    .replace(/(^|[\s(])(https?:\/\/[^\s<)]+)/g, '$1<a href="$2" target="_blank" rel="noopener">$2</a>');
  return s.split(/\n{2,}/).map((para) => {
    const lines = para.split("\n");
    if (/^\u0000\d+\u0000$/.test(para.trim())) return para.trim();
    if (lines.every((l) => /^\s*[-*] /.test(l))) return `<ul>${lines.map((l) => `<li>${inline(l.replace(/^\s*[-*] /, ""))}</li>`).join("")}</ul>`;
    if (lines.every((l) => /^\s*\d+[.)] /.test(l))) return `<ol>${lines.map((l) => `<li>${inline(l.replace(/^\s*\d+[.)] /, ""))}</li>`).join("")}</ol>`;
    const h = para.match(/^#{1,4} (.*)$/);
    if (h && lines.length === 1) return `<p><strong>${inline(h[1])}</strong></p>`;
    return `<p>${lines.map(inline).join("<br>")}</p>`;
  }).join("").replace(/\u0000(\d+)\u0000/g, (_, i) => `<pre><code>${blocks[i]}</code></pre>`);
}

async function api(method, path, body) {
  const res = await fetch(path, {
    method,
    headers: body ? { "content-type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
  return data;
}

// State
let me = { name: "" };
let harnesses = [];
let defaultHarness = "claude";
let bots = [];
const messages = {}; // botId -> array, loaded on first open
const drafts = {}; // botId -> streaming text
let activeId = localStorage.getItem("activeBot");
let filter = "";
let showHidden = false;

const bot = (id) => bots.find((b) => b.id === id);
const primary = () => bots.find((b) => b.primary);

function upsertBot(b) {
  const i = bots.findIndex((x) => x.id === b.id);
  if (i >= 0) bots[i] = { ...bots[i], ...b }; else bots.push(b);
}

// Sidebar
function renderSidebar() {
  const p = primary();
  $("primary-bot").innerHTML = p ? `<div class="avatar-wrap">${avatar(p.color, p.shape, 82)}
    <span class="badge"><svg viewBox="0 0 24 24"><path d="M12 2l3 7 7 .6-5.3 4.7 1.6 7.2L12 17.8 5.7 21.5l1.6-7.2L2 9.6 9 9z"/></svg></span>
    ${p.unread && p.id !== activeId ? '<span class="unread"></span>' : ""}</div>
    <span>${esc(p.name)}</span>` : "";
  $("primary-bot").classList.toggle("active", p?.id === activeId);

  const q = filter.toLowerCase();
  const list = bots.filter((b) => !b.primary && b.hidden === showHidden)
    .filter((b) => !q || b.name.toLowerCase().includes(q) || (b.preview || "").toLowerCase().includes(q));
  $("bot-list").innerHTML = list.map((b) => `<button class="bot-row ${b.id === activeId ? "active" : ""}" data-id="${b.id}">
      ${avatar(b.color, b.shape, 52)}
      <div class="meta"><div class="name">${esc(b.name)}</div>
        <div class="preview">${b.working ? `<em>${esc(b.status || "Working")}…</em>` : esc(b.preview || "")}</div></div>
      ${b.unread && b.id !== activeId ? '<span class="unread"></span>' : ""}
    </button>`).join("") || (showHidden ? '<div class="empty">No hidden Bots.</div>' : "");

  const hidden = bots.filter((b) => b.hidden).length;
  $("hidden-label").textContent = showHidden ? "Back to Bots" : "Hidden Bots";
  $("hidden-count").textContent = showHidden ? "" : hidden;
  $("me-name").textContent = me.name;
  $("me-avatar").textContent = (me.name || "?")[0].toUpperCase();
}

// Chat
const ACTIONS = `<div class="actions">
  <button title="Copy" data-act="copy"><svg viewBox="0 0 24 24"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3"/></svg></button>
</div>`;

function when(ts) {
  const d = new Date(ts);
  const today = new Date(); today.setHours(0, 0, 0, 0);
  const day = new Date(d); day.setHours(0, 0, 0, 0);
  const diff = (today - day) / 86400000;
  const time = d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  if (diff === 0) return `Today ${time}`;
  if (diff === 1) return `Yesterday ${time}`;
  return `${d.toLocaleDateString([], { month: "short", day: "numeric" })} ${time}`;
}

function renderAtts(atts) {
  if (!atts?.length) return "";
  return atts.map((a) => {
    const url = `/attachments/${encodeURIComponent(a.id)}`;
    if (a.kind === "image") return `<img src="${url}" alt="${esc(a.name || "image")}" class="chat-media" />`;
    if (a.kind === "video") return `<video src="${url}" controls class="chat-media" />`;
    return `<a class="file-att" href="${url}" target="_blank" rel="noopener">${esc(a.name || "file")}</a>`;
  }).join("");
}

function renderMessage(m, prev) {
  let out = !prev || m.at - prev.at > 30 * 60000 ? `<div class="timestamp">${when(m.at)}</div>` : "";
  if (m.role === "routine") return out + `<div class="timestamp routine">Routine · ${esc(m.text)}</div>`;
  if (m.role === "peer" || m.role === "reply") {
    const who = bot(m.fromBot)?.name || "another Bot";
    const label = m.role === "peer" ? `From ${esc(who)}` : `Reply from ${esc(who)}`;
    return out + `<div class="msg-row peer"><div class="bubble"><div class="from">${label}</div>${md(m.text)}</div></div>`;
  }
  if (m.role === "user") return out + `<div class="msg-row user"><div class="bubble">${esc(m.text).replace(/\n/g, "<br>")}${renderAtts(m.attachments)}</div></div>`;
  if (m.text) {
    out += `<div class="msg-row bot" data-id="${m.id}"><div class="bubble ${m.error ? "error" : ""}">${md(m.text)}${renderAtts(m.attachments)}</div>${ACTIONS}</div>`;
  }
  if (m.question) {
    const q = m.question;
    out += `<div class="msg-row bot"><div class="bubble question ${q.picked != null ? "answered" : ""}" data-id="${m.id}">
      <h4>${esc(q.title)}</h4>
      ${q.handoff && q.picked == null ? `<button class="open-screen">Open screen</button>` : ""}
      ${q.options.map((o, k) => `<button class="option ${q.picked === k ? "picked" : ""}" data-k="${k}">
        <span class="key">${"ABCDEF"[k]}</span>${esc(o)}</button>`).join("")}
    </div></div>`;
  }
  return out;
}

function renderChat(keepScroll) {
  const b = bot(activeId);
  if (!b) { $("messages").innerHTML = ""; return; }
  const el = $("messages");
  const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
  $("bot-pill").innerHTML = `${avatar(b.color, b.shape, 30)}<span>${esc(b.name)}</span><span class="harness">${esc(b.harness)}</span>`;
  $("input").placeholder = `Message ${b.name}`;
  const list = messages[b.id] || [];
  let html = list.map((m, i) => renderMessage(m, list[i - 1])).join("");
  if (b.working) {
    const draft = drafts[b.id];
    if (draft) html += `<div class="msg-row bot"><div class="bubble">${md(draft)}</div></div>`;
    html += `<div class="msg-row bot"><div class="bubble typing"><span></span><span></span><span></span>
      <em class="status">${esc(b.status || "Working")}</em><button class="stop" id="stop">Stop</button></div></div>`;
  }
  el.innerHTML = html;
  if (!keepScroll || atBottom) scrollToBottom(); else updateScrollBtn();
}

function scrollToBottom() {
  const el = $("messages");
  el.scrollTop = el.scrollHeight;
  updateScrollBtn();
}

function updateScrollBtn() {
  const el = $("messages");
  $("scroll-down").classList.toggle("show", el.scrollHeight - el.scrollTop - el.clientHeight > 80);
}

async function open(id) {
  if (!bot(id)) id = primary()?.id;
  if (!id) return;
  activeId = id;
  localStorage.setItem("activeBot", id);
  if (!messages[id]) messages[id] = await api("GET", `/api/bots/${id}/messages`);
  markRead();
  render();
  if (details.open) openDetails(); // load the new Bot's routines
  if (!$("screen").hidden) loadScreen();
}

function markRead() {
  const b = bot(activeId);
  if (b?.unread && document.hasFocus()) {
    b.unread = false;
    api("POST", `/api/bots/${b.id}/read`).catch(() => {});
  }
}

// Files selected for the next message (uploaded immediately, attached on send).
const pendingAtts = [];

function renderPending() {
  const el = $("pending-files");
  if (!el) return;
  el.innerHTML = pendingAtts.length
    ? pendingAtts.map((a, i) => `<span class="pending-chip">${a.kind === "image" ? "🖼" : a.kind === "video" ? "🎬" : "📎"} ${esc(a.name)} <b data-rm="${i}">×</b></span>`).join("")
    : "";
}

$("attach-btn")?.addEventListener("click", () => $("file-input").click());
$("file-input")?.addEventListener("change", async (e) => {
  const files = [...e.target.files];
  e.target.value = "";
  for (const file of files) {
    try {
      const res = await fetch("/api/upload", {
        method: "POST",
        headers: {
          "Content-Type": file.type || "application/octet-stream",
          "X-File-Name": btoa(unescape(encodeURIComponent(file.name))),
        },
        body: file,
      });
      if (!res.ok) {
        let err = {};
        try { err = await res.json(); } catch {}
        throw new Error(err.error || `Upload failed (${res.status})`);
      }
      pendingAtts.push(...(await res.json()));
    } catch (err) {
      alert(err.message);
    }
  }
  renderPending();
});
$("pending-files")?.addEventListener("click", (e) => {
  const rm = e.target.closest("[data-rm]");
  if (!rm) return;
  pendingAtts.splice(+rm.dataset.rm, 1);
  renderPending();
});

async function sendText(text, answer) {
  const b = bot(activeId);
  if (!b || !text) return;
  const atts = pendingAtts.splice(0);
  renderPending();
  try {
    await api("POST", `/api/bots/${b.id}/messages`, { text, answer, ...(atts.length ? { attachments: atts } : {}) });
  } catch (err) {
    pendingAtts.unshift(...atts);
    renderPending();
    alert(err.message);
  }
}

function render(keepScroll) {
  renderSidebar();
  renderChat(keepScroll);
  renderModelBar();
  if (details.open && document.activeElement?.id !== "d-name" && !document.activeElement?.closest?.("#d-routine-form")) renderDetails();
}

// Model and effort picker above the input. Saved on the Bot, so each Bot keeps its own.
const modelLists = {}; // harness -> Promise<[{id, label, efforts}]>
let modelBarKey = "";
const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1);

async function renderModelBar() {
  const first = bot(activeId);
  if (!first) return;
  modelLists[first.harness] ??= api("GET", `/api/harnesses/${first.harness}/models`).catch(() => []);
  const list = await modelLists[first.harness];
  const b = bot(activeId); // re-read: the Bot may have changed while loading
  if (!b || b.id !== first.id || b.harness !== first.harness) return;
  const key = [b.id, b.harness, b.model, b.effort, list.length].join("|");
  if (key === modelBarKey) return;
  modelBarKey = key;

  const all = [...new Set(list.flatMap((m) => m.efforts))];
  const models = [{ id: "", label: "Default model", efforts: all }, ...list];
  // A Bot saved with an alias (e.g. "opus") shows as its exact model.
  const isCurrent = (m) => m.id === (b.model || "") || (b.model && m.aliases?.includes(b.model));
  if (b.model && !list.some(isCurrent)) models.push({ id: b.model, label: b.model, efforts: all });
  const current = models.find(isCurrent);
  $("model-select").innerHTML = models.map((m) =>
    `<option value="${esc(m.id)}" ${m === current ? "selected" : ""}>${esc(m.label)}</option>`).join("");
  $("effort-select").innerHTML = ["", ...current.efforts].map((e) =>
    `<option value="${esc(e)}" ${e === (b.effort || "") ? "selected" : ""}>${e ? cap(e) : "Default effort"}</option>`).join("");
  $("effort-select").hidden = !current.efforts.length;
  $("model-bar").hidden = false;
}

async function saveModel(patch) {
  const b = bot(activeId);
  if (!b) return;
  try { upsertBot(await api("PATCH", `/api/bots/${b.id}`, patch)); } catch (err) { alert(err.message); }
  renderModelBar();
}

$("model-select").addEventListener("change", async (e) => {
  const b = bot(activeId);
  const list = await modelLists[b.harness];
  const m = list.find((x) => x.id === e.target.value);
  // Keep the effort only if the new model supports it.
  const effort = m && b.effort && !m.efforts.includes(b.effort) ? null : b.effort;
  saveModel({ model: e.target.value || null, effort });
});
$("effort-select").addEventListener("change", (e) => saveModel({ effort: e.target.value || null }));

// Live screen: the Bot's own desktop, streamed by voxl. View only until you take over.
let screenTimer = null;
function setTakeover(on) {
  $("screen-shield").hidden = on;
  $("screen-take").classList.toggle("on", on);
  $("screen-take").textContent = on ? "Give back" : "Take over";
}
async function loadScreen() {
  const id = activeId;
  $("screen-title").textContent = `${bot(id)?.name || "Bot"}'s screen`;
  const frame = $("screen-frame");
  try {
    const { url } = await api("GET", `/api/bots/${id}/screen`); // also keeps it running while you watch
    if (id !== activeId || $("screen").hidden) return;
    if (frame.dataset.url !== url) { frame.dataset.url = url; frame.src = url; setTakeover(true); }
    $("screen-msg").hidden = true;
  } catch (err) {
    frame.removeAttribute("src"); delete frame.dataset.url;
    $("screen-msg").textContent = err.message;
    $("screen-msg").hidden = false;
  }
}
function openScreen() {
  if (details.open) closeDetails(); // one right-hand panel at a time
  $("screen").hidden = false;
  loadScreen();
  clearInterval(screenTimer);
  screenTimer = setInterval(loadScreen, 60000);
}
function closeScreen() {
  $("screen").hidden = true;
  clearInterval(screenTimer);
  $("screen-frame").removeAttribute("src");
  delete $("screen-frame").dataset.url;
}
$("screen-btn").addEventListener("click", () => ($("screen").hidden ? openScreen() : closeScreen()));
$("screen-close").addEventListener("click", closeScreen);
$("screen-take").addEventListener("click", () => {
  const on = !$("screen-shield").hidden;
  setTakeover(on);
  if (on) $("screen-frame").focus();
});
// 点击遮罩也直接接管，省去先找按钮
$("screen-shield").addEventListener("click", () => {
  setTakeover(true);
  $("screen-frame").focus();
});

// Live updates
function connect() {
  const es = new EventSource("/api/events");
  es.onmessage = (ev) => {
    const e = JSON.parse(ev.data);
    if (e.type === "resync") return void load(true);
    if (e.type === "message") {
      const list = messages[e.message.botId];
      if (list && !list.some((m) => m.id === e.message.id)) list.push(e.message);
      if (e.message.role === "bot") delete drafts[e.message.botId];
      if (e.bot) upsertBot(e.bot);
      if (e.message.botId === activeId) markRead();
    } else if (e.type === "bot") {
      upsertBot(e.bot);
    } else if (e.type === "botDeleted") {
      bots = bots.filter((b) => b.id !== e.botId);
      if (activeId === e.botId) return void open(primary()?.id);
    } else if (e.type === "draft") {
      drafts[e.botId] = e.text;
    } else if (e.type === "status") {
      const b = bot(e.botId);
      if (b) { b.working = e.working; b.status = e.status; }
      if (!e.working) delete drafts[e.botId];
    }
    render(true);
  };
  // Resync after a reconnect, in case events were missed.
  let opened = false;
  es.onopen = () => { if (opened) load(true); opened = true; };
}

async function load(refresh) {
  const state = await api("GET", "/api/state");
  me = state.me;
  harnesses = state.harnesses;
  defaultHarness = state.defaultHarness;
  bots = state.bots;
  if (refresh) for (const id of Object.keys(messages)) messages[id] = await api("GET", `/api/bots/${id}/messages`);
  await open(activeId);
}

// Events
$("bot-list").addEventListener("click", (e) => {
  const row = e.target.closest(".bot-row");
  if (row) open(row.dataset.id);
});
$("primary-bot").addEventListener("click", () => open(primary()?.id));

$("messages").addEventListener("click", (e) => {
  if (e.target.closest("#stop")) return void api("POST", `/api/bots/${activeId}/stop`).catch(() => {});
  const copy = e.target.closest('[data-act="copy"]');
  if (copy) {
    const m = messages[activeId]?.find((x) => x.id === +copy.closest(".msg-row").dataset.id);
    if (m) navigator.clipboard.writeText(m.text);
    return;
  }
  if (e.target.closest(".open-screen")) return void openScreen();
  const opt = e.target.closest(".option");
  if (!opt) return;
  const card = opt.closest(".question");
  const m = messages[activeId]?.find((x) => x.id === +card.dataset.id);
  if (!m || m.question.picked != null) return;
  m.question.picked = +opt.dataset.k;
  render(true);
  sendText(m.question.options[m.question.picked], { messageId: m.id, option: m.question.picked });
});

$("composer").addEventListener("submit", (e) => {
  e.preventDefault();
  const text = $("input").value.trim();
  if (!text) return;
  $("input").value = "";
  sendText(text);
});

// New Bot window
const COLORS = ["#f5f5f5", "#9a6b3f", "#e5484d", "#e8702e", "#e9a23b", "#4cc06a", "#4fb5a5", "#3b82f6", "#8b5cf6", "#e64a9c", "#7a7a7a"];
const HARNESS_NAMES = { claude: "Claude Code", codex: "Codex", pi: "Pi" };
const nb = { color: COLORS[0], shape: "waddle", harness: "claude" };
const pickOne = (a) => a[Math.floor(Math.random() * a.length)];

function renderNewBot() {
  $("nb-preview").innerHTML = avatar(nb.color, nb.shape, 150);
  $("nb-shapes").innerHTML = Object.keys(SHAPES).map((s) =>
    `<button type="button" class="nb-shape" data-shape="${s}" title="${s}">${avatar(nb.color, s, 60, s === nb.shape)}</button>`).join("");
  $("nb-colors").innerHTML = COLORS.map((c) =>
    `<button type="button" class="nb-color ${c === nb.color ? "on" : ""}" data-color="${c}" style="background:${c}" title="${c}"></button>`).join("");
  const list = harnesses.length ? harnesses : Object.keys(HARNESS_NAMES);
  $("nb-harness").innerHTML = list.map((h) =>
    `<option value="${h}" ${h === nb.harness ? "selected" : ""}>${HARNESS_NAMES[h] || h}</option>`).join("");
  $("nb-create").disabled = !$("nb-name").value.trim();
}

function openNewBot() {
  Object.assign(nb, { color: pickOne(COLORS), shape: pickOne(Object.keys(SHAPES)), harness: defaultHarness });
  $("nb-name").value = "";
  $("nb-error").hidden = true;
  renderNewBot();
  $("nb").hidden = false;
  $("nb-name").focus();
}
const closeNewBot = () => { $("nb").hidden = true; };

$("new-btn").addEventListener("click", openNewBot);
$("nb-close").addEventListener("click", closeNewBot);
$("nb-cancel").addEventListener("click", closeNewBot);
$("nb").addEventListener("mousedown", (e) => { if (e.target === $("nb")) closeNewBot(); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !$("nb").hidden) closeNewBot(); });
$("nb-name").addEventListener("input", () => { $("nb-create").disabled = !$("nb-name").value.trim(); });
$("nb-form").addEventListener("click", (e) => {
  const t = e.target.closest("[data-shape], [data-color]");
  if (!t) return;
  if (t.dataset.shape) nb.shape = t.dataset.shape;
  if (t.dataset.color) nb.color = t.dataset.color;
  renderNewBot();
});
$("nb-harness").addEventListener("change", (e) => { nb.harness = e.target.value; });
$("nb-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const name = $("nb-name").value.trim();
  if (!name) return;
  $("nb-create").disabled = true;
  try {
    const b = await api("POST", "/api/bots", { name, color: nb.color, shape: nb.shape, harness: nb.harness });
    upsertBot(b);
    closeNewBot();
    open(b.id);
  } catch (err) {
    $("nb-error").textContent = err.message;
    $("nb-error").hidden = false;
    $("nb-create").disabled = false;
  }
});

// Bot details: a right-hand panel, opened by clicking the name at the top of the chat.
const ICONS = {
  clock: '<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/></svg>',
  job: '<svg viewBox="0 0 24 24"><rect x="4" y="7" width="16" height="12" rx="2"/><path d="M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7"/></svg>',
  chevron: '<svg viewBox="0 0 24 24"><path d="M9 6l6 6-6 6"/></svg>',
  trash: '<svg viewBox="0 0 24 24"><path d="M5 7h14M10 7V5h4v2M7 7l1 12h8l1-12"/></svg>',
  plus: '<svg viewBox="0 0 24 24"><path d="M12 5v14M5 12h14"/></svg>',
};
const details = { open: false, tab: "activity", routines: [], form: null };

// Human hint for the simplest cron shapes; returns "" when it isn't sure.
const CRON_DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
function cronHint(cron) {
  const p = String(cron || "").trim().split(/\s+/);
  if (p.length !== 5) return "";
  const [mi, hr, dom, mon, dow] = p;
  const num = (s) => (/^\d{1,2}$/.test(s) ? +s : null);
  const m = num(mi), h = num(hr), d = num(dom), w = num(dow);
  const time = (M, H) => `${String(H).padStart(2, "0")}:${String(M).padStart(2, "0")}`;
  if (mi === "*" && hr === "*" && dom === "*" && mon === "*" && dow === "*") return "Every minute";
  const every = mi.match(/^\*\/(\d{1,2})$/);
  if (every && hr === "*" && dom === "*" && mon === "*" && dow === "*") return `Every ${every[1]} min`;
  if (mi === "0" && hr === "*" && dom === "*" && mon === "*" && dow === "*") return "Hourly";
  if (m != null && h != null && m <= 59 && h <= 23 && dom === "*" && mon === "*" && dow === "*") return `Daily at ${time(m, h)}`;
  if (m != null && h != null && m <= 59 && h <= 23 && dom === "*" && mon === "*" && w != null && w <= 6) return `${CRON_DAYS[w]}s at ${time(m, h)}`;
  if (m != null && h != null && m <= 59 && h <= 23 && d != null && d >= 1 && d <= 31 && mon === "*" && dow === "*") return `Monthly on day ${d} at ${time(m, h)}`;
  return "";
}

function lastRunText(ms) {
  if (ms == null || !ms) return "Never run";
  const d = new Date(ms);
  const p = (n) => String(n).padStart(2, "0");
  return `Last run ${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function routineFormHtml() {
  const f = details.form;
  const editing = f.mode === "edit";
  return `<form class="d-routine-form" id="d-routine-form" autocomplete="off">
    <div class="d-form-title">${editing ? "Edit routine" : "New routine"}</div>
    <label class="d-field"><span>Name</span>
      <input class="d-input" data-f="name" value="${esc(f.name)}" placeholder="Morning brief" maxlength="80" /></label>
    <label class="d-field"><span>Cron</span>
      <input class="d-input" data-f="cron" value="${esc(f.cron)}" placeholder="0 9 * * *" spellcheck="false" /></label>
    <label class="d-field"><span>Instruction</span>
      <textarea class="d-input" data-f="instruction" rows="6" placeholder="What should this Bot do when the routine runs?">${esc(f.instruction)}</textarea></label>
    <div class="d-form-actions">
      <button type="button" class="d-btn" id="d-routine-cancel">Cancel</button>
      <button type="submit" class="d-btn primary" id="d-routine-save">Save</button>
    </div>
  </form>`;
}

function routineRowHtml(r) {
  const toggle = (on, attrs) => `<button type="button" class="switch ${on ? "on" : ""}" ${attrs}><span></span></button>`;
  if (details.form?.mode === "edit" && String(details.form.id) === String(r.id)) return routineFormHtml();
  const hint = cronHint(r.cron);
  return `<div class="d-routine">
    <div class="d-item d-routine-main">
      <div class="d-text">
        <div class="d-rname">${esc(r.name)}</div>
        <div class="d-sub">${esc(r.cron)}${hint ? ` · ${esc(hint)}` : ""}</div>
        <div class="d-sub d-last">${esc(lastRunText(r.last_run_at))}</div>
      </div>
      ${toggle(r.enabled, `data-routine="${r.id}" title="${r.enabled ? "Pause" : "Resume"}" aria-label="${r.enabled ? "Pause" : "Resume"}"`)}
    </div>
    <div class="d-tools">
      <button type="button" class="d-icon" data-run="${r.id}" title="Run now" aria-label="Run now">▶</button>
      <button type="button" class="d-icon" data-edit="${r.id}" title="Edit" aria-label="Edit">✎</button>
      <button type="button" class="d-icon d-icon-danger" data-del="${r.id}" title="Delete" aria-label="Delete">${ICONS.trash}</button>
    </div>
  </div>`;
}

async function openDetails() {
  details.open = true;
  if (!$("screen").hidden) closeScreen(); // one right-hand panel at a time
  details.routines = [];
  details.form = null;
  renderDetails();
  try { details.routines = await api("GET", `/api/bots/${activeId}/routines`); } catch {}
  renderDetails();
}
function closeDetails() { details.open = false; details.form = null; renderDetails(); }

function renderDetails() {
  const el = $("details");
  const b = bot(activeId);
  el.hidden = !details.open || !b;
  if (el.hidden) return;
  const toggle = (on, attrs) => `<button type="button" class="switch ${on ? "on" : ""}" ${attrs}><span></span></button>`;
  const routines = `
      ${details.routines.length ? details.routines.map(routineRowHtml).join("") : details.form ? "" : `<div class="d-item d-empty">No routines yet.</div>`}
      ${details.form?.mode === "new" ? routineFormHtml() : ""}
      <button type="button" class="d-item d-link d-new" id="d-new-routine">${ICONS.plus}<span>New routine</span></button>`;
  const activity = `
    <div class="d-section"><span>Suggested</span></div>
    <div class="d-card">
      <button class="d-item d-link" data-prefill="Set up a routine to ">${ICONS.clock}<span>Create a routine</span>${ICONS.chevron}</button>
      <button class="d-item d-link" data-prefill="Your job: ">${ICONS.job}<span>Give it a job</span>${ICONS.chevron}</button>
    </div>
    <div class="d-section"><span>Routines</span></div>
    <div class="d-card">${routines}</div>
    <div class="d-section"><span>Computer</span></div>
    <div class="d-card"><div class="d-item d-computer">
      <div class="d-text"><div>Bot's computer</div><div class="d-sub">${b.working ? esc(b.status || "Working") : "Asleep"}</div></div>
      <div class="d-screen ${b.working ? "awake" : ""}">${avatar(b.color, b.shape, 40)}${b.working ? "" : "<i>z</i>"}</div>
    </div></div>`;
  const list = harnesses.length ? harnesses : Object.keys(HARNESS_NAMES);
  const settings = `
    <div class="d-section"><span>Bot</span></div>
    <div class="d-card">
      <label class="d-item"><span class="d-text">Runs on</span>
        <select id="d-harness">${list.map((h) => `<option value="${h}" ${h === b.harness ? "selected" : ""}>${HARNESS_NAMES[h] || h}</option>`).join("")}</select></label>
      <div class="d-item"><span class="d-text">Hide from sidebar</span>${toggle(b.hidden, 'id="d-hide"')}</div>
    </div>
    ${b.primary ? "" : `<div class="d-card"><button class="d-item d-link d-danger" id="d-delete">${ICONS.trash}<span>Delete Bot</span></button></div>`}`;
  el.innerHTML = `
    <button class="d-close" id="d-close" title="Close"><svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg></button>
    <div class="d-hero">${avatar(b.color, b.shape, 120)}
      <input class="d-name" id="d-name" value="${esc(b.name)}" maxlength="60" title="Rename" spellcheck="false" /></div>
    <div class="d-tabs">
      <button data-tab="activity" class="${details.tab === "activity" ? "on" : ""}">Activity</button>
      <button data-tab="settings" class="${details.tab === "settings" ? "on" : ""}">Settings</button>
    </div>
    ${details.tab === "activity" ? activity : settings}`;
}

async function patchBot(body) {
  try { upsertBot(await api("PATCH", `/api/bots/${activeId}`, body)); render(true); renderDetails(); }
  catch (err) { alert(err.message); }
}

$("bot-pill").addEventListener("click", () => (details.open ? closeDetails() : openDetails()));
$("details").addEventListener("click", async (e) => {
  const t = e.target.closest("button");
  if (!t) return;
  if (t.id === "d-close") return closeDetails();
  if (t.dataset.tab) { details.tab = t.dataset.tab; details.form = null; return renderDetails(); }
  if (t.dataset.prefill) { $("input").value = t.dataset.prefill; $("input").focus(); return; }
  if (t.id === "d-hide") return patchBot({ hidden: !bot(activeId).hidden });
  if (t.id === "d-new-routine") {
    details.form = { mode: "new", name: "", cron: "", instruction: "" };
    renderDetails();
    $("details").querySelector('[data-f="name"]')?.focus();
    return;
  }
  if (t.id === "d-routine-cancel") { details.form = null; return renderDetails(); }
  if (t.dataset.edit) {
    const r = details.routines.find((x) => String(x.id) === t.dataset.edit);
    if (!r) return;
    details.form = { mode: "edit", id: r.id, name: r.name, cron: r.cron, instruction: r.instruction || "" };
    renderDetails();
    $("details").querySelector('[data-f="name"]')?.focus();
    return;
  }
  if (t.dataset.del) {
    const r = details.routines.find((x) => String(x.id) === t.dataset.del);
    if (!r || !confirm(`Delete routine "${r.name}"?`)) return;
    try {
      await api("DELETE", `/api/routines/${r.id}`);
      details.routines = details.routines.filter((x) => String(x.id) !== String(r.id));
      if (String(details.form?.id) === String(r.id)) details.form = null;
    } catch (err) { alert(err.message); }
    return renderDetails();
  }
  if (t.dataset.run) {
    const r = details.routines.find((x) => String(x.id) === t.dataset.run);
    if (!r) return;
    const label = t.innerHTML;
    try {
      await api("POST", `/api/routines/${r.id}/run`);
      t.classList.add("started");
      t.textContent = "Started";
      setTimeout(() => {
        if (t.isConnected) { t.classList.remove("started"); t.innerHTML = label; }
      }, 1500);
    } catch (err) { alert(err.message); }
    return;
  }
  if (t.dataset.routine) {
    const r = details.routines.find((x) => String(x.id) === t.dataset.routine);
    if (!r) return;
    try { Object.assign(r, await api("PATCH", `/api/routines/${r.id}`, { enabled: !r.enabled })); } catch (err) { alert(err.message); }
    return renderDetails();
  }
  if (t.id === "d-delete") {
    const b = bot(activeId);
    if (confirm(`Delete ${b.name}? This erases its chat, memory, and routines. Files on the computer stay.`)) {
      try { await api("DELETE", `/api/bots/${b.id}`); closeDetails(); } catch (err) { alert(err.message); }
    }
  }
});
$("details").addEventListener("change", (e) => { if (e.target.id === "d-harness") patchBot({ harness: e.target.value }); });
$("details").addEventListener("input", (e) => {
  const fld = e.target.closest?.("[data-f]");
  if (fld && details.form) details.form[fld.dataset.f] = fld.value;
});
$("details").addEventListener("submit", async (e) => {
  if (e.target.id !== "d-routine-form") return;
  e.preventDefault();
  const f = details.form;
  if (!f) return;
  const name = f.name.trim(), cron = f.cron.trim();
  if (!name || !cron) return alert("Name and cron are required.");
  const save = $("d-routine-save");
  if (save) save.disabled = true;
  try {
    const body = { name, cron, instruction: f.instruction };
    const r = f.mode === "new"
      ? await api("POST", `/api/bots/${activeId}/routines`, body)
      : await api("PATCH", `/api/routines/${f.id}`, body);
    const i = details.routines.findIndex((x) => String(x.id) === String(r.id));
    if (i >= 0) details.routines[i] = r; else details.routines.push(r);
    details.form = null;
  } catch (err) {
    alert(err.message);
    if (save) save.disabled = false;
    return;
  }
  renderDetails();
});
$("details").addEventListener("keydown", (e) => { if (e.target.id === "d-name" && e.key === "Enter") e.target.blur(); });
$("details").addEventListener("focusout", (e) => {
  if (e.target.id !== "d-name") return;
  const name = e.target.value.trim();
  if (name && name !== bot(activeId)?.name) patchBot({ name });
  else e.target.value = bot(activeId)?.name ?? "";
});
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape" || !details.open || !$("nb").hidden) return;
  if (details.form) { details.form = null; return renderDetails(); }
  closeDetails();
});

$("hidden-btn").addEventListener("click", () => { showHidden = !showHidden; renderSidebar(); });
$("search-btn").addEventListener("click", () => {
  const s = $("search");
  s.hidden = !s.hidden;
  if (!s.hidden) s.focus(); else { filter = ""; s.value = ""; renderSidebar(); }
});
$("search").addEventListener("input", (e) => { filter = e.target.value; renderSidebar(); });
$("messages").addEventListener("scroll", updateScrollBtn);
$("scroll-down").addEventListener("click", () => $("messages").scrollTo({ top: $("messages").scrollHeight, behavior: "smooth" }));
window.addEventListener("focus", () => { markRead(); renderSidebar(); });

load().then(connect).catch((err) => { $("messages").innerHTML = `<div class="timestamp">Can't reach the server: ${esc(err.message)}</div>`; });
