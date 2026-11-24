# SuperBot UI v2 "Clear Glass" — implementation notes

## Scope of this run
Part 1 only: `web/theme.css` was fully replaced and this note written. `web/styles.css` and
`web/app.js` were left untouched; `web/skin.js` / `web/index.html` are handled in part 2.

## What was built
A single override stylesheet loaded after `styles.css`.
- **Token-first**: full light + dark palettes under `:root[data-theme="light"]` and
  `:root[data-theme="dark"]` (`:root` carries the light defaults to avoid a flash before
  skin.js sets the attribute). Raw design numbers from DESIGN.md only — no acid colours,
  one accent (`--accent`), 4-pt spacing, four radii, two shadow tiers.
- **One glass recipe, reused**: a shared grouped rule applies translucent fill,
  `backdrop-filter: blur(var(--blur)) saturate(180%)`, 1px inner rim (`--rim`), outer
  hairline, two-layer shadow, and a `::before` 135deg specular sheen to the six fixed
  chrome surfaces (sidebar, header pills, composer, details panel, modal, screen panel)
  plus the scroll-down chip. Content (bubbles, rows) stays solid and calm.
- **Aurora canvas**: three radial-gradient blobs in `body` plus two drifting
  transform-only pseudo-element blobs; soft, quiet, no `backdrop-filter` animation.
- **Components**: sidebar floating island, glass icon buttons, primary hero + bot rows,
  segmented tabs, accent send circle, error/peer/user bubbles with 6px tails, glass
  timestamp chips, routines as cards, 56px shape tiles, and the Create Bot glass sheet.
- **Theme toggle**: `.sb-theme-toggle` styled as a 36px glass circle in `.sidebar-bottom`;
  `.sidebar-bottom .me` uses `margin-right:auto` so the toggle and Apps button group at right.
- **Share chips**: `.share-btn` is inline-flex and `.share-btn svg` is a 16px flex-none
  icon, so skin.js can inject `<svg> + <span>Label</span>` and keep alignment.
- **Avatars**: `.avatar` is styled as a glass orb SVG — drop-shadow, no global stroke,
  overflow visible; skin.js supplies the gradient/highlight/rim/glyph inside it.
- **Mobile ≤768px**: sidebar recomposed as a glass drawer (right-corner radius only),
  composer pinned with `env(safe-area-inset-bottom)`, bubbles max-width 86%, blur reduced
  to 14px, aurora animation dropped, and 44px minimum touch targets on all controls.
- **Fallbacks & a11y**: `@supports not (backdrop-filter: blur(1px))` swaps to opaque
  `--glass-solid` fills with no blur; `prefers-reduced-motion` disables all animation;
  accent `:focus-visible` rings and AA text tokens in both themes.

## Selectors overridden (theme.css wins by source order)
`.sidebar`, `.sidebar-top`, `.icon-btn`, `.search`, `.primary-bot` (+`.active`, `>span`),
`.badge`, `.bot-list`, `.bot-row` (+`.active`, `.name`, `.preview`, `.unread`), `.hidden-bots`,
`.sidebar-bottom`, `.me`, `.me-avatar`, `.sb-theme-toggle`; `.chat-header`, `.bot-pill`
(+`.harness`), `.share-btn` (+`.screen-btn`, `svg`), `.menu-btn`; `.messages`, `.msg-row`,
`.timestamp` (+`.routine`), `.bubble` (+`pre/code/a/error/peer`), `.actions`, `.question`,
`.option`, `.typing`, `.chat-media`, `.file-att`, `.scroll-down`, `.pending-chip`;
`.composer` (+`:focus-within`, `input`, `-plus`, `-mic`, `-voice`), `.footnote`; `.screen*`;
`.details`, `.d-*` (tabs/cards/items/routines/forms/buttons/switch); `.modal*` and `.nb-*`.
Legacy token aliases (`--accent-a/-b/-gradient`) are kept for any inline skin code.

## Avatar / icon override (skin.js, part 2)
`skin.js` is loaded after `app.js`. It replaces `window.avatar` with a same-signature function
that returns `<svg class="avatar" viewBox="0 0 48 48" width=… height=…>`: a rounded-squircle
tile filled with a unique-id linear gradient (lightened tint → bot colour), a white specular
ellipse, a 1px inner rim, a white line glyph per shape key (12 keys, legacy names mapped via
`OLD_SHAPES`), and an accent selection ring when `ring=true`. Unique gradient/clip ids avoid
collisions across calls. Static SVGs in `index.html` are swapped from an `ICONS` object
(24×24, stroke `currentColor`, 1.75, round caps/joins, fill none). No DOM structure or
`styles.css` changes are needed for either override.

## Performance notes
- Blur is limited to fixed chrome; never on list rows, bubbles, or the aurora blobs.
- Aurora motion is transform-only on two pseudo-elements (disabled on mobile/reduced-motion).
- Hover/active transitions touch only background, colour, border, and transform/opacity.
- The grouped glass rule keeps the recipe in one place; the no-blur fallback is a single block.
- File length is ~740 lines, within the ~900-line budget.
