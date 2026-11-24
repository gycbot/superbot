# Theme pass — "calm control room"

## What changed and why

- Added `web/theme.css` as a pure override layer loaded after `styles.css`. No existing rule, class name, id, or `app.js` logic was touched; the app's structure and behaviour are unchanged.
- Replaced the flat #171717 canvas with a deep graphite background: layered radial glows (indigo top-right, cyan bottom-left) plus an inline SVG-noise grain. This gives depth without an image download or extra element in the DOM.
- Turned the sidebar, header pill, Share button, composer, messages, right-hand panels and modal into frosted glass surfaces (translucent fills, `backdrop-filter` blur, 1px low-alpha white hairlines, soft depth shadows). The existing translucent surfaces let the background glow show through instead of sitting on opaque black.
- Introduced one accent: an indigo→cyan gradient (`--accent-gradient`). It is used only on high-signal elements — active bot row, unread/working dots, primary/send buttons, user bubbles, selected options, toggles and focus rings — so it stays meaningful.
- Bot bubbles are elevated dark-glass cards with 15px/1.6 type, `overflow-wrap: anywhere` for the long URLs in this console, and cyan links. Error bubbles became a muted red-tinted card with a 3px left bar instead of a bright all-around border. A distinct tint was kept for peer/reply bubbles.
- Bot rows now have 12px corners, a hover lift (background + `translateY(-1px)`) and a glowing active state. The working/unread dot and the inline dot before the "Working" status pulse with a 2.4s keyframe.
- The composer is a floating glass pill with an indigo focus ring via `:focus-within`. Scrollbars were slimmed and restyled for both Firefox and WebKit.
- Messages and timestamps fade/slide in using only transform and opacity (200ms). All animation and transition is switched off under `prefers-reduced-motion`, including the hover lifts.
- Mobile refinements at `<=768px` only adjust spacing and type so content fits a 390px column; the existing drawer, full-screen panels and breakpoint at 820px are untouched.
- Text colours were raised to light grey/near-white on the dark glass to keep body copy, previews and labels at WCAG AA contrast; white-on-gradient is used for user bubbles and primary buttons.

## Selectors overridden

Base / tokens: `:root`, `html`, `body`, `body::before`, `*`, `::-webkit-scrollbar`, `::-webkit-scrollbar-track`, `::-webkit-scrollbar-thumb` (+ `:hover`)

Sidebar: `.sidebar`, `.icon-btn` (+ `:hover`), `.badge`, `.search` (+ `::placeholder`, `:focus`), `.primary-bot` (+ `:hover`, `.active`, `.active > span`), `.bot-row` (+ `:hover`, `.active`, `.name`, `.preview`, `.unread`), `.bot-row .preview em::before`, `.primary-bot .avatar-wrap .unread`, `.hidden-bots` (+ `:hover`), `.me`, `.me-avatar`

Header: `.bot-pill` (+ `:hover`, `.harness`), `.share-btn` (+ `:hover`)

Messages: `@keyframes msgIn`, `.msg-row`, `.timestamp` (+ `.routine`), `.bubble` (+ `strong`, `a`, `code`, `pre`, `pre code`), `.msg-row.user .bubble` (+ `strong`, `a`), `.bubble.error`, `.msg-row.peer .bubble`, `.actions button` (+ `:hover`), `.question` (+ `h4`, `.option .key`, `.option:hover .key`, `.option.picked .key`, `.open-screen`), `.typing span`, `.chat-media`, `.file-att` (+ `:hover`), `.scroll-down` (+ `:hover`), `@keyframes dotPulse`

Composer / chrome: `.composer` (+ `:focus-within`, `input`, `input::placeholder`), `.composer-plus`, `.composer-mic`, `.composer-voice` (+ `:hover`), `.footnote` (+ `b`), `.pending-chip` (+ `b`)

Panels / modal: `.screen`, `.screen-take` (+ `:hover`, `.on`), `.screen-view`, `.details`, `.d-close` (+ `:hover`), `.d-name:focus`, `.d-tabs button.on`, `.d-card`, `.d-link:hover`, `.d-icon` (+ `:hover`), `.d-input:focus`, `.d-btn` (+ `:hover`, `.primary`), `.switch.on`, `.d-danger` (+ `svg`), `.modal-backdrop`, `.modal`, `.modal-head`, `.modal-foot`, `.modal-head h3`, `.modal-close` (+ `:hover`), `.nb-input` (+ `:focus`, `::placeholder`), `.nb-color.on`, `.nb-error`, `.nb-btn` (+ `:hover`, `.primary`, `.primary:hover`, `.primary:disabled`), `.nb-runs select`, `.model-bar select:hover`, `.model-bar select:focus-visible`

Media queries: `@media (max-width: 768px)` — `.chat-header`, `.messages`, `.bubble`, `.msg-row.user .bubble`, `.timestamp`, `.bot-pill`, `.question` (+ `h4`), `.option`, `.composer input`, `.footnote`, `.sidebar`, `.bot-row` (+ `.name`, `.preview`). `@media (prefers-reduced-motion: reduce)` — all elements plus explicit resets for `.msg-row`, `.timestamp`, hover lifts and pulse dots.
