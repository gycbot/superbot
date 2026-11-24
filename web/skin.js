/* SuperBot UI v2 "Clear Glass" skin — theme bootstrap, icon swap, avatar redraw. */
(function () {
  "use strict";

  /* ---------- theme ---------- */
  var KEY = "sb-theme";
  function readTheme() {
    var t;
    try { t = localStorage.getItem(KEY); } catch (e) {}
    if (t === "light" || t === "dark") return t;
    try {
      if (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) return "dark";
    } catch (e) {}
    return "light";
  }
  function setTheme(t) {
    if (t !== "light" && t !== "dark") t = "light";
    document.documentElement.dataset.theme = t;
    try { localStorage.setItem(KEY, t); } catch (e) {}
    syncThemeBtn();
  }
  window.sbSetTheme = setTheme;
  document.documentElement.dataset.theme = readTheme();

  /* ---------- icons ---------- */
  var ICONS = {
    search: '<circle cx="11" cy="11" r="6.5"/><path d="M20 20l-4.2-4.2"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    apps: '<rect x="4" y="4" width="6" height="6" rx="1.5"/><rect x="14" y="4" width="6" height="6" rx="1.5"/><rect x="4" y="14" width="6" height="6" rx="1.5"/><path d="M17 14v6M14 17h6"/>',
    menu: '<path d="M4 7h16M4 12h16M4 17h16"/>',
    share: '<circle cx="18" cy="5" r="2.5"/><circle cx="6" cy="12" r="2.5"/><circle cx="18" cy="19" r="2.5"/><path d="M8.3 10.8l7.4-4.2M8.3 13.2l7.4 4.2"/>',
    screen: '<rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8M12 17v4"/>',
    close: '<path d="M6 6l12 12M18 6L6 18"/>',
    arrowDown: '<path d="M12 5v14M6 13l6 6 6-6"/>',
    attach: '<path d="M20 11.5l-8 8a5 5 0 0 1-7-7l8.5-8.5a3.3 3.3 0 0 1 4.7 4.7L9.8 17a1.6 1.6 0 0 1-2.3-2.3l7.8-7.8"/>',
    mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21"/>',
    send: '<path d="M12 20V5M6 11l6-6 6 6"/>',
    copy: '<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M15 9V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h3"/>',
    clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
    briefcase: '<rect x="3" y="7" width="18" height="13" rx="2"/><path d="M9 7V5a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2M3 12h18"/>',
    chevron: '<path d="M9 6l6 6-6 6"/>',
    trash: '<path d="M4 7h16M9 7V5h6v2M6 7l1 13h10l1-13M10 11v5M14 11v5"/>',
    sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M19.1 4.9l-1.4 1.4M6.3 17.7l-1.4 1.4"/>',
    moon: '<path d="M20 14.5A8.5 8.5 0 0 1 9.5 4 8.5 8.5 0 1 0 20 14.5z"/>',
    sparkle: '<path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z"/><path d="M18 16l.8 2.2L21 19l-2.2.8L18 22l-.8-2.2L15 19l2.2-.8z"/>',
    settings: '<path d="M4 7h10M18 7h2M4 17h2M10 17h10"/><circle cx="16" cy="7" r="2.2"/><circle cx="8" cy="17" r="2.2"/>'
  };
  function icon(name) {
    return '<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">' + (ICONS[name] || "") + '</svg>';
  }
  function setIcon(el, name) { if (el) el.innerHTML = icon(name); }
  function byId(id) { return document.getElementById(id); }

  setIcon(byId("search-btn"), "search");
  setIcon(byId("new-btn"), "plus");
  setIcon(byId("menu-btn"), "menu");
  setIcon(byId("scroll-down"), "arrowDown");
  setIcon(byId("attach-btn"), "attach");
  setIcon(document.querySelector(".composer-mic"), "mic");
  setIcon(document.querySelector(".composer-voice"), "send");
  setIcon(byId("screen-close"), "close");
  setIcon(byId("nb-close"), "close");
  setIcon(document.querySelector(".sidebar-bottom .icon-btn"), "apps");

  var screenBtn = byId("screen-btn");
  if (screenBtn) screenBtn.innerHTML = icon("screen") + "<span>Screen</span>";
  var shareBtns = document.querySelectorAll(".chat-header .share-btn");
  for (var i = 0; i < shareBtns.length; i++) {
    if (shareBtns[i] === screenBtn) continue;
    var label = (shareBtns[i].textContent || "Share").trim() || "Share";
    shareBtns[i].innerHTML = icon("share") + "<span>" + label + "</span>";
  }

  /* ---------- theme toggle ---------- */
  var themeBtn = null;
  function syncThemeBtn() {
    if (!themeBtn) return;
    var dark = document.documentElement.dataset.theme === "dark";
    setIcon(themeBtn, dark ? "sun" : "moon");
    themeBtn.title = dark ? "Switch to light" : "Switch to dark";
    themeBtn.setAttribute("aria-label", themeBtn.title);
  }
  (function addToggle() {
    var bottom = document.querySelector(".sidebar-bottom");
    if (!bottom) return;
    themeBtn = document.createElement("button");
    themeBtn.type = "button";
    themeBtn.className = "icon-btn theme-btn sb-theme-toggle";
    themeBtn.id = "theme-btn";
    bottom.insertBefore(themeBtn, bottom.querySelector(".icon-btn"));
    themeBtn.addEventListener("click", function () {
      setTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark");
    });
  })();
  syncThemeBtn();

  /* ---------- avatars ---------- */
  var GLYPHS = {
    waddle: '<circle cx="19" cy="21" r="1.7" fill="#fff" stroke="none"/><circle cx="29" cy="21" r="1.7" fill="#fff" stroke="none"/><path d="M18 27q6 6 12 0"/>',
    drop: '<path d="M24 14c4 5 7 9.5 7 13a7 7 0 0 1-14 0c0-3.5 3-8 7-13z"/>',
    cloud: '<path d="M17 30h14a5 5 0 0 0 .4-10 7 7 0 0 0-13-1.6A5.8 5.8 0 0 0 17 30z"/>',
    bean: '<path d="M31 16c-10 0-16 6-16 13 0 2 .8 3.6.8 3.6S22 30 27 26c4-3 5-7 4-10z"/><path d="M16 32c4-5 8-8 13-10"/>',
    onigiri: '<path d="M24 15l10 18H14z"/><path d="M20 27h8v6h-8z"/>',
    peanut: '<path d="M24 24c-3-5-9.5-5-9.5 0S21 29 24 24c3-5 9.5-5 9.5 0S27 29 24 24z"/>',
    pebble: '<g fill="#fff" stroke="none"><circle cx="19" cy="20" r="2"/><circle cx="29" cy="19" r="1.5"/><circle cx="25" cy="28" r="2.4"/><circle cx="16.5" cy="28.5" r="1.2"/></g>',
    square: '<rect x="17" y="17" width="14" height="14" rx="2.5"/><path d="M24 17v14M17 24h14"/>',
    jelly: '<path d="M16 28c3-4 5 4 8 0s5 4 8 0"/><path d="M16 21c3-4 5 4 8 0s5 4 8 0"/>',
    gem: '<path d="M24 14l9 8-9 12-9-12z"/><path d="M15 22h18M24 14v20"/>',
    gear: '<circle cx="24" cy="24" r="6"/><path d="M24 15v-3M24 36v-3M15 24h-3M36 24h-3M17.6 17.6l-2.1-2.1M32.5 32.5l-2.1-2.1M30.4 17.6l2.1-2.1M15.5 32.5l2.1-2.1"/>',
    heart: '<path d="M24 32l-8-8a5 5 0 0 1 8-6 5 5 0 0 1 8 6z"/>'
  };
  var OLD = { round: "waddle", star: "gear", flower: "gear", scallop: "gear", splat: "gear", blob: "pebble", egg: "drop", wide: "bean", triangle: "onigiri", pentagon: "gem", hexagon: "gem" };
  if (typeof OLD_SHAPES !== "undefined" && OLD_SHAPES) OLD = OLD_SHAPES;
  var uid = 0;
  function lighten(hex, amt) {
    var m = /^#?([a-f\d]{3}|[a-f\d]{6})$/i.exec(String(hex).trim());
    if (!m) return hex;
    var h = m[1];
    if (h.length === 3) h = h.charAt(0) + h.charAt(0) + h.charAt(1) + h.charAt(1) + h.charAt(2) + h.charAt(2);
    var mix = function (v) { return ("0" + Math.round(v + (255 - v) * amt).toString(16)).slice(-2); };
    return "#" + mix(parseInt(h.slice(0, 2), 16)) + mix(parseInt(h.slice(2, 4), 16)) + mix(parseInt(h.slice(4, 6), 16));
  }
  window.avatar = function avatarV2(color, shape, size, ring) {
    size = size || 48;
    var key = (typeof shape === "string" && GLYPHS[shape]) ? shape
            : (typeof shape === "string" && GLYPHS[OLD[shape]] ? OLD[shape] : "waddle");
    var gid = "svg-g-" + (++uid), cid = "svg-c-" + uid;
    var light = lighten(color, 0.42);
    var lum = (function (c) { var m = /^#?([a-f\d]{6})$/i.exec(lighten(c, 0)); if (!m) return 0.5; var n = parseInt(m[1], 16);
      return (0.2126 * (n >> 16) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255)) / 255; })(color);
    var pale = lum > 0.8;                       // white / very light bots: tint the tile, draw a dark glyph
    if (pale) { light = "#ffffff"; color = "#dfe5ef"; }
    var ink = pale ? "#334155" : "#fff";
    var sid = "svg-s-" + uid;
    var ringSvg = ring
      ? '<rect x="1.3" y="1.3" width="45.4" height="45.4" rx="13.4" fill="none" stroke="rgba(255,255,255,.95)" stroke-width="3.4"/>' +
        '<rect x="1.3" y="1.3" width="45.4" height="45.4" rx="13.4" fill="none" stroke="#3b6cff" stroke-width="1.8"/>'
      : "";
    return '<svg class="avatar" viewBox="0 0 48 48" width="' + size + '" height="' + size +
      '" style="stroke:none;width:' + size + 'px;height:' + size + 'px;overflow:visible" aria-hidden="true">' +
      '<defs>' +
        '<linearGradient id="' + gid + '" x1="0" y1="0" x2="0" y2="1">' +
          '<stop offset="0" stop-color="' + light + '"/><stop offset="1" stop-color="' + color + '"/>' +
        '</linearGradient>' +
        '<linearGradient id="' + sid + '" x1="0" y1="0" x2="0" y2="1">' +
          '<stop offset="0" stop-color="#fff" stop-opacity=".55"/><stop offset=".48" stop-color="#fff" stop-opacity=".08"/>' +
          '<stop offset=".5" stop-color="#fff" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity=".10"/>' +
        '</linearGradient>' +
        '<clipPath id="' + cid + '"><rect x="0" y="0" width="48" height="48" rx="14"/></clipPath>' +
      '</defs>' +
      '<rect x="0" y="0" width="48" height="48" rx="14" fill="url(#' + gid + ')"/>' +
      '<g clip-path="url(#' + cid + ')">' +
        '<rect x="0" y="0" width="48" height="48" fill="url(#' + sid + ')"/>' +
      '</g>' +
      '<rect x=".75" y=".75" width="46.5" height="46.5" rx="13.4" fill="none" stroke="rgba(255,255,255,.6)" stroke-width="1.5"/>' +
      '<rect x=".5" y=".5" width="47" height="47" rx="13.8" fill="none" stroke="rgba(15,23,42,.1)" stroke-width="1"/>' +
      '<g fill="none" stroke="' + ink + '" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">' + GLYPHS[key] + '</g>' +
      ringSvg +
      '</svg>';
  };
  try { if (typeof render === "function") render(); } catch (e) {}
})();
