# Header v2 — fade + mobile grid (notes)

## Files changed
- `web/theme.css` — appended one section at the very end: `/* ===== header v2 (fade + mobile grid) ===== */`. It is placed last so it wins over all earlier header/mobile rules (including the "review fixes" block). No upstream file (`styles.css`, `app.js`, `index.html`) was touched.
- `HEADER_NOTES.md` — this file (new).

## What changed

### 1. Transparent, scrolling-under header (all widths)
- `.chat-header` is now `position: absolute; top/left/right: 0; z-index: 6; height: var(--header-h)` instead of an in-flow 72px bar. It overlays `.messages`, which is back to filling the whole chat column.
- No strip: `background: transparent; border: 0; box-shadow: none`. The pill and chips keep their existing glass recipe (section 3 of theme.css) and remain the only visible chrome.
- `pointer-events: none` on the header; `pointer-events: auto` on its children (`.chat-header > *`); both pseudos are `pointer-events: none`. Only the buttons/pill are clickable; drags scroll the list beneath.

### 2. Fade + masked blur
- `.chat-header::before` carries the required gradient:
  `linear-gradient(to bottom, var(--bg) 0%, … 45%, transparent 100%)`.
  - With `color-mix` support: `color-mix(in srgb, var(--bg) 85%, transparent)` (wrapped in `@supports (background: color-mix(...))`).
  - Fallback: `var(--bg-fade-mid)` token — `rgba(238,241,246,.85)` light / `rgba(12,15,22,.85)` dark.
- `.chat-header::after` adds `backdrop-filter: blur(8px)` (with `-webkit-` prefix) masked by `mask-image: linear-gradient(to bottom, #000 40%, transparent)` (with `-webkit-` prefix), so the blur fades out with the gradient.
- Header height tokens: `--header-h: 96px` desktop, `68px` mobile.
- `.messages` gets `padding-top: calc(var(--header-h) + 16px)` (desktop) and `calc(var(--header-h) + env(safe-area-inset-top,0px) + 10px)` (mobile) so the first message is never hidden.

### 3. Desktop (min-width: 769px)
- Pill stays centered (`justify-content: center`); chips stay on the right: `.share-btn { right: 16px }`, `.share-btn.screen-btn { right: 116px }`; menu hidden; pill `max-width: calc(100% - 280px)` to avoid chip collisions.

### 4. Mobile header (max-width: 768px) — 3 logical groups
- `.chat-header` becomes a grid: `grid-template-columns: 40px minmax(0,1fr) 40px 40px` with `column-gap: 6px`. Tracks map to `[menu] [pill] [screen] [share]` — the two action buttons occupy the last two tracks (the "actions" group).
- Safe areas: `padding: max(8px, env(safe-area-inset-top,0px)) max(8px, env(safe-area-inset-right,0px)) 8px max(8px, env(safe-area-inset-left,0px))`; header height `calc(var(--header-h) + env(safe-area-inset-top,0px))`.
- All legacy absolute offsets are reset: `.chat-header .menu-btn`, `.chat-header .share-btn`, `.chat-header .share-btn.screen-btn` → `position: relative; inset: auto; right/left/top/bottom: auto`. The old `right: 116px / 84px / 16px / 8px` and `top: 14px / 12px`, `left: 12px / 8px` rules no longer apply.
- Menu + both share buttons are 40px round icon-only buttons (`width/height/min-width/min-height: 40px; border-radius: 50%; font-size: 0; padding: 0`); `<span>` labels hidden, SVG 18px. They are placed explicitly into grid columns 1, 3 and 4 (Share via `.share-btn:not(.screen-btn)`).
- Pill: `grid-column: 2; justify-self: center; min-width: 0; max-width: 100%`; name span (`.bot-pill > span:not(.harness)`) truncates with ellipsis; harness chip is `flex: none`.
- At `max-width: 400px` the `.harness` chip is `display: none`, leaving more room for the name. Verified no overlap at 360 / 390 / 430 px (pill gets ~206px at 360px after menu, actions, gaps and safe padding).

### 5. Fallbacks / a11y
- `@supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px)))` hides `.chat-header::after` while the gradient fade remains. (The pill/chips keep their existing opaque `--glass-solid` fallback from theme.css section 14.)
- `@media (prefers-reduced-motion: reduce)` disables transitions/animations on the header and both pseudos.

## Selectors touched
`.chat-header`, `.chat-header::before`, `.chat-header::after`, `.chat-header > *`, `.messages`, `.chat-header .menu-btn`, `.chat-header .bot-pill`, `.chat-header .bot-pill > span:not(.harness)`, `.chat-header .bot-pill .harness`, `.chat-header .share-btn`, `.chat-header .share-btn.screen-btn`, `.chat-header .share-btn:not(.screen-btn)`, `.chat-header .share-btn span`, `.chat-header .share-btn svg`, plus tokens `--header-h`, `--bg-fade-mid`.
