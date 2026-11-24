# SuperBot UI v2 — "Clear Glass" design spec

Owner ask (t3154u): clean, fresh, frosted + liquid glass, better icons, more change than v1.

## References (and what we take from each)
1. **ClawHub `steipete/swiftui-liquid-glass`** (Apple iOS 26 Liquid Glass guidance) — glass only on the
   *control layer* (sidebar, header, composer, pills, sheets), never on content; one consistent shape family;
   grouped glass elements share one container; "interactive" glass only on tappable things; always a non-glass fallback.
2. **ClawHub `promisexvwsc/minimalist-design-system`** — token-first: one accent, 4-pt spacing, small type scale,
   few radii, neutral surfaces; every value comes from a CSS variable.
3. **Team `surface-web-craft-kit` (liquid-glass craft) + vendored `ui-aesthetics` skill** — bright "bone/paper" option,
   single accent, no acid colours, mobile-first (≥44px targets, safe-area), specular rim highlights, restrained motion.

## Principles
- Content is solid and calm; chrome floats as glass above a soft, colourful-but-quiet backdrop (aurora blobs).
- Two themes: **Light (default "Clear")** and **Dark ("Night glass")**, toggle in sidebar footer, remembered in
  localStorage, initial value follows `prefers-color-scheme`.
- Liquid glass recipe (one class, reused): translucent fill + `backdrop-filter: blur(var(--blur)) saturate(180%)`
  + 1px inner rim (`inset 0 1px 0 rgba(255,255,255,.55)`) + outer hairline + soft two-layer shadow
  + a `::before` specular sheen (linear-gradient 135deg white→transparent at low opacity).
- Fallback: `@supports not (backdrop-filter: blur(1px))` → same surfaces with an opaque fill (no blur).
- Performance: blur only on ≤6 fixed surfaces (sidebar, header, composer, modal, details panel, pills); never on
  list rows or bubbles; on `max-width:768px` reduce blur to 14px and drop the aurora animation; no animated
  backdrop-filter; motion = transform/opacity only; `prefers-reduced-motion` disables all animation.

## Tokens
| token | Light | Dark |
|---|---|---|
| --bg | #eef1f6 | #0c0f16 |
| --aurora-1/2/3 | #c7d7ff / #ffd6e8 / #c9f2ec (blurred blobs, 40% alpha) | #3046a8 / #6b2a6e / #0f5e5a (25%) |
| --glass | rgba(255,255,255,.58) | rgba(22,26,36,.55) |
| --glass-strong | rgba(255,255,255,.78) | rgba(28,33,46,.78) |
| --rim | rgba(255,255,255,.75) | rgba(255,255,255,.10) |
| --hairline | rgba(15,23,42,.08) | rgba(255,255,255,.08) |
| --surface (bubbles) | #ffffff | #171b25 |
| --text / --text-2 / --text-3 | #0f172a / #475569 / #94a3b8 | #e8ecf4 / #a3adc2 / #6b7489 |
| --accent | #3b6cff (single accent) | #6b8cff |
| --accent-soft | rgba(59,108,255,.12) | rgba(107,140,255,.18) |
| --danger | #e5484d | #ff6b70 |
| --blur | 24px (mobile 14px) | same |
| --radius-s/m/l/xl | 10 / 14 / 20 / 28 px | same |
| --shadow-1 | 0 1px 2px rgba(15,23,42,.06), 0 8px 24px -12px rgba(15,23,42,.18) | 0 1px 2px rgba(0,0,0,.4), 0 12px 32px -14px rgba(0,0,0,.7) |
| --shadow-2 (floating) | 0 2px 6px rgba(15,23,42,.06), 0 24px 48px -20px rgba(15,23,42,.28) | darker equivalent |
| spacing | 4-pt scale: 4 8 12 16 20 24 32 | |

Type: system stack `-apple-system, "SF Pro Text", "Segoe UI Variable", "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif`.
Scale 12 / 13 / 15 (body) / 17 (titles) / 22 (hero name); weights 400/500/600; letter-spacing -0.01em on ≥17px;
tabular numbers for times.

## Components
- **Sidebar**: full-height glass panel with 12px inset margin and --radius-xl (desktop "floating island"); top row icon
  buttons as 36px glass circles; primary bot as hero (avatar 72 + name + small "Primary" chip); bot list rows 56px,
  --radius-m, hover = --accent-soft fill, active = glass-strong + accent left dot; unread = accent dot; working = soft
  pulsing ring around avatar; preview text --text-3 13px single line.
- **Header**: floating glass pill bar (centered bot pill with avatar 26 + name + harness chip), buttons as glass chips.
- **Chat bubbles**: bot = --surface card with hairline + --shadow-1, radius 18 with 6px corner at the tail side;
  user = accent fill (solid, not gradient) white text; error = danger-tinted surface with small alert icon, no thick border;
  timestamps as small centered glass chips; code blocks with subtle mono background.
- **Composer**: floating liquid-glass capsule (radius 28, height 56) with specular sheen; icon buttons 40px;
  send button = accent circle with arrow-up icon (not the waveform), mic = ghost.
- **Details / Settings panel**: glass sheet, segmented-control tabs (glass track + white thumb), form fields with
  --surface fill, 44px targets, routines as cards.
- **Modal (Create Bot)**: centered glass sheet radius 28, shape/colour pickers as 56px tiles.
- **Mobile ≤768px**: sidebar becomes a glass drawer (100% height, radius on right corners only), header pill compact,
  composer pinned with `env(safe-area-inset-bottom)`, bubbles max-width 86%.
- **Icon set** (stroke 1.75, 24 grid, round caps — Lucide-like but drawn by us, inline SVG, no CDN): search, plus,
  apps(grid), menu, share, screen(monitor), close, arrow-down, attach(paperclip), mic, send(arrow-up), copy, clock,
  briefcase, chevron, trash, sun, moon, sparkle(primary badge), settings(sliders).
- **Bot avatars v2**: keep the 12 shape keys (waddle, drop, cloud, bean, onigiri, peanut, pebble, square, jelly, gem,
  gear, heart) so existing bots keep identity, but redraw as **glass "orb" glyphs**: rounded-squircle tile in the bot
  colour (gradient light→colour), inner white specular highlight, and a simple white line glyph per shape
  (e.g. waddle=smile face, drop=droplet, cloud=cloud, bean=leaf, onigiri=triangle, peanut=infinity, pebble=dot-cluster,
  square=grid, jelly=wave, gem=diamond, gear=cog, heart=heart). Clean, flat, consistent at 26–150px.
- **Login**: browser-native HTTP Basic dialog (not ours) — no change; the 30-day cookie means it rarely appears.
