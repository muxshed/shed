# DESIGN.md — Muxshed Design System

**Codename: "Amber Rack"** — a retro broadcast control-surface aesthetic.

This document is the single source of truth for all Muxshed UI across `system/web/`,
`commercial/portal/`, and `commercial/marketing/`. It is **mandatory**: every screen,
component, and form must follow it. When code and this document disagree, this document
wins — fix the code.

> **Design system version:** 2.0 (supersedes the 1.0 "modern dark" system)
> **Theme:** single, dark-only (see §2)
> **Last updated:** 2026-06-21

---

## 1. Philosophy

Muxshed is a live production switcher — a piece of broadcast equipment that happens to
run in a browser. The UI should feel like operating a real hardware mixing desk from the
golden age of broadcast: **amber phosphor readouts, labeled rack panels, segmented meters,
chunky switches.** Every screen is a control surface, not a web page.

Principles:

1. **It's a console, not a website.** Dense, labeled, immediate. Information is laid out
   on panels you scan, not paragraphs you read.
2. **Amber is the ink.** The interface glows amber on near-black. Color is reserved for
   *meaning* (live, record, warning), never decoration.
3. **Hardware honesty.** Buttons look pressable, fields look recessed, panels look
   bolted-in. Skeuomorphic but restrained.
4. **Immersive but operable.** The retro skin is full-commitment, but a real operator must
   run a live show on it for hours. Readability and accessibility are non-negotiable (§5, §10).

---

## 2. Theme: dark-only

Muxshed has **one theme.** There is no light mode. An amber-CRT control surface has no
meaningful light variant, and a broadcast environment is dark by design.

- `color-scheme: dark` is set globally.
- Do **not** add `prefers-color-scheme: light` overrides or `dark:` Tailwind variants.
- Do **not** add a light/dark toggle. The only appearance toggle is **"reduce effects"** (§5).

This is a deliberate departure from the 1.0 system. Any prior dual-theme rules are void.

---

## 3. Color

All values are exact. Use the CSS variables in §11 — never hardcode hex in components.

### 3.1 Surfaces (warm near-black, layered)

| Token | Hex | Use |
|-------|-----|-----|
| `--bg` | `#120D07` | App background, lowest layer |
| `--panel` | `#160F08` | Panel / card body |
| `--panel-raised` | `#1B140A` | Header strips, raised controls, hover |
| `--well` | `#0A0A0A` | Recessed wells: video output, sunken field interiors |
| `--border` | `#4A3717` | Default panel & control borders |
| `--border-dim` | `#3A2C13` | Internal dividers, list rows |

### 3.2 Amber ink

| Token | Hex | Use |
|-------|-----|-----|
| `--amber` | `#FFB000` | Primary text, primary control outline, readouts |
| `--amber-bright` | `#FFD479` | Active/hover ink, focused field text, emphasis |
| `--amber-dim` | `#C8861F` | Secondary text, panel headers, labels |
| `--amber-muted` | `#7A5A1E` | Tertiary/disabled text, idle markers, hints |

### 3.3 Status colors (carried over from brand semantics)

| Token | Hex | Glow | Meaning |
|-------|-----|------|---------|
| `--live` | `#3CA643` | `#46E06A` | Live / connected / Go Live / on-air |
| `--warning` | `#F2E96D` | — | Warning, broadcast-delay caution, pending |
| `--danger` | `#EF4444` | `#FF5A4D` | Record, error, destructive actions, disconnected |
| `--transition` | `#C8861F` | — | Mid-action / transitioning (pulsing, §5) |

Status colors appear **only** as small fills (pills, dots, meter segments, button faces).
Body and chrome are always amber. The retired `#3C3459` brand purple/blue is **not** used
in the UI (it survives only inside the logo artwork).

### 3.4 Usage rules

- Default text is `--amber-dim`; promote to `--amber` for primary content, `--amber-bright`
  for the single most important / active element in a view.
- Never put amber text on a status-color fill. Text on a green/red/yellow fill uses a
  near-black from that hue's shadow (e.g. green pill text `#46E06A` on `#0E2A14`).
- Disabled = `--amber-muted` ink at `opacity: 0.5`, no border glow.

---

## 4. Typography

### 4.1 Fonts

```css
--font-mono: 'JetBrains Mono', 'IBM Plex Mono', ui-monospace, 'SF Mono', Menlo, monospace;
--font-led:  'DSEG7 Classic', 'JetBrains Mono', monospace; /* 7-segment numeric readouts */
```

- Load **JetBrains Mono** (weights 400/500/700) from `fonts.googleapis.com`.
- Load **DSEG7 Classic** from `cdn.jsdelivr.net` (npm `dseg`). It is used **only** for
  numeric readouts: stream timer, broadcast-delay seconds, bitrate, counts, clocks.
- The entire UI is monospace. There is no sans-serif body font.

### 4.2 Scale

| Role | Size | Weight | Transform | Color |
|------|------|--------|-----------|-------|
| Screen title (status bar) | 13px | 500 | none | `--amber-bright` |
| Panel header | 11px | 500 | UPPERCASE, `letter-spacing: 2px` | `--amber-dim` |
| Field label | 11px | 400 | UPPERCASE, `letter-spacing: 1px` | `--amber-dim` |
| Body / values | 12–13px | 400 | none | `--amber` / `--amber-dim` |
| Button text | 12–13px | 500 | UPPERCASE, `letter-spacing: 1px` | per button |
| LED readout (sm) | 18px | — | `--font-led` | `--amber` + glow |
| LED readout (lg) | 28–40px | — | `--font-led` | `--amber` + glow |
| Code / IDs | 12px | 400 | none | `--amber-dim` |

Minimum font size anywhere is **11px**. Headers and labels are UPPERCASE; values and prose
are normal case.

---

## 5. CRT effects & motion

The skin is immersive but **motion-safe by default.**

### 5.1 Scanlines

A static, non-animated scanline texture sits on the app background and inside large wells:

```css
background-image: repeating-linear-gradient(0deg, rgba(0,0,0,0.18) 0 1px, transparent 1px 3px);
```

Scanlines are texture only — they never reduce text contrast below AA (§10).

### 5.2 Glow

Soft amber glow is applied **only** to: LED readouts, the active/focused control, and
on-air indicators. Use `text-shadow`/`box-shadow`, never `filter: blur` on text.

```css
--glow-amber: 0 0 6px rgba(255,176,0,0.5);
--glow-live:  0 0 6px rgba(70,224,106,0.4);
```

### 5.3 No flicker by default

There is **no animated flicker, curvature, or bloom.** The only permitted motion is a slow
(≥1.2s) opacity pulse on `--transition` (mid-action) and record indicators.

### 5.4 "Reduce effects" + reduced motion

- A **"reduce effects"** toggle (in Settings and the status bar) sets `data-fx="off"` on
  `<html>`, which disables scanlines, glow, and all pulsing. The choice persists in
  `localStorage` under `muxshed-fx`.
- `@media (prefers-reduced-motion: reduce)` **also** disables scanlines/glow/pulse
  automatically, regardless of the toggle.

```css
:root[data-fx="off"] .scanlines,
@media (prefers-reduced-motion: reduce) { /* no scanlines, no glow, no pulse */ }
```

---

## 6. Spacing, borders, radii, elevation

- **Spacing scale (4px base):** 4 / 6 / 8 / 10 / 12 / 16 / 24 / 32.
- **Borders:** `1px solid var(--border)` for panels/controls; `1px solid var(--border-dim)`
  for internal dividers. Borders are crisp, never soft.
- **Radii:** chrome is mostly square. `--radius-sm: 2px` (fields, buttons),
  `--radius-md: 4px` (panels, cards). Nothing is pill-shaped except status dots/LEDs.
- **Elevation = bezels, not shadows.** Raised controls get a 1px light top/left inset
  (`rgba(255,212,121,0.08)`) and a darker bottom/right; recessed wells invert it. No
  blurred drop shadows.

---

## 7. Layout & app shell

```
┌──────────────────────────────────────────────────────────────┐
│ STATUS BAR  ◉ MUXSHED / DASHBOARD        ● ON AIR  [01:24:07] │  status bar
├────────────┬─────────────────────────────────────────────────┤
│ RACK NAV   │  PANEL GRID                                      │
│ ▮ DASHBOARD│  ┌── ▮ SOURCES ──┐  ┌── ▮ PROGRAM ──┐           │
│ ▸ Sources  │  │   ...          │  │   ...          │           │
│ ▸ Scenes   │  └────────────────┘  └────────────────┘           │
│ ▸ Dest.    │                                                  │
│ ▸ Library  │                                                  │
│ ▸ Delay    │                                                  │
│ ▸ Settings │                                                  │
└────────────┴─────────────────────────────────────────────────┘
```

- **Status bar** (top, fixed height ~40px): logo + breadcrumb on the left; global state on
  the right (ON AIR pill + LED stream timer, record indicator, reduce-effects toggle).
- **Rack nav** (left, ~180px): vertical list of "modules." Active item is amber-bright with
  a left accent bar and `--panel-raised` fill.
- **Panel grid** (content): CSS Grid of labeled rack-module panels. Each panel = header
  strip (`▮ TITLE`) + body.
- **Popout windows** (`/popout/program|preview|sources|audio`) reuse the same chrome with a
  minimal/no nav, sized for second-monitor use.

Container max width 1400px; panel grid is responsive (`auto-fit, minmax(280px, 1fr)`).

---

## 8. Components

Every component below is **canonical**. Build these once as shared classes/primitives and
reuse — do not restyle ad hoc per page.

### 8.1 Panel (rack module)

Anatomy: 1px `--border`, `--radius-md`, `--panel` body, header strip in `--panel-raised`
with `▮ TITLE` (panel header type, §4.2). Optional header-right slot for actions.

```svelte
<section class="panel">
  <header class="panel__head">▮ SOURCES <span class="panel__actions">…</span></header>
  <div class="panel__body">…</div>
</section>
```

### 8.2 Buttons

| Variant | Face | Border | Text | Use |
|---------|------|--------|------|-----|
| Primary | transparent | `1px var(--amber)` | `--amber` | default actions ("+ ADD") |
| Go Live | `#0E2A14` | `1px #2F8F44` | `--live-glow` + glow | start stream / activate |
| Danger | `#2A0D0A` | `1px #8F2F2F` | `--danger-glow` | record, delete, stop |
| Ghost | transparent | none | `--amber-dim` | tertiary / inline |
| Icon | `--panel-raised` | `1px var(--border)` | `--amber` | square icon-only (≥44px) |

States: hover lifts ink to `--amber-bright` and adds the matching glow; active translates
1px down (pressed); disabled = `--amber-muted` @ `opacity:.5`, no glow, `cursor:not-allowed`.
Button text is UPPERCASE.

### 8.3 Form fields

The most important interactive surface — **all forms must use these.**

- **Text input / textarea / select:** recessed well (`--well` bg, inset bezel),
  `1px var(--border)`, `--amber-bright` text, `--font-mono`, padding `6px 8px`,
  `--radius-sm`. Placeholder = `--amber-muted`. A blinking amber caret (`caret-color: var(--amber)`).
- **Label:** field-label type above the control (§4.2). Always associate via `for`/`id`
  (a11y, §10).
- **Focus:** border → `--amber`, plus 1px outer amber focus ring (`box-shadow: 0 0 0 1px var(--amber)`) and the amber glow. Never remove focus styling.
- **Checkbox / radio:** square 16px well that fills `--amber` with a black check/dot when on.
- **Toggle switch:** hardware rocker — `OFF` = `--well`, `ON` = `--live` fill; ~40×20px.
- **Validation:** error state borders `--danger` with an inline `--danger` message below;
  success/confirmed uses `--live`.

### 8.4 Status pills & indicators

Small uppercase pills with a leading glyph: `● ON AIR` (live, green), `● LIVE` (green),
`○ IDLE` (amber-muted), `● REC` (danger, slow pulse), `▲ DELAY` (warning). Pill = hue-tinted
near-black fill + 1px hue border + hue-glow text. Standalone dots are 8px circles.

### 8.5 LED readout

`--font-led`, `--amber`, `--glow-amber`. For timers `HH:MM:SS`, delay `7.0s`, bitrate
`6500 kbps`, counts. Optionally show a dim "off segments" ghost behind (authentic 7-seg).
Right-aligned in its container. Disabled by `data-fx="off"` only for the glow, not the value.

### 8.6 Segmented meter

Row of equal segments (gap 2px) that fill `--amber` → `--amber-dim` → empty `--border-dim`.
Used for broadcast-delay buffer and audio VU. VU peak segment uses `--danger`. No smooth
animation; segments switch discretely.

### 8.7 Nav (rack list)

Vertical list; each item is a row with optional icon + label. Active = `--amber-bright`
text, `--panel-raised` fill, 2px left accent in `--amber`. Hover = `--amber`.

### 8.8 Lists / cards (sources, destinations, recordings)

Row or card on `--panel-raised`, `1px --border-dim`, name in `--amber`, metadata in
`--amber-dim`, a trailing status pill and an icon-button action cluster. Source/destination
cards show a live indicator dot.

### 8.9 Dialog / modal

Centered panel over a `rgba(10,7,3,0.7)` scrim. Same panel chrome with a `▮ TITLE` header,
body, and a right-aligned action row (Ghost cancel + Primary/Danger confirm). Trap focus;
`Esc` closes; first field autofocused.

### 8.10 Toast

Bottom-right stack of small panels, 1px hue border by severity (info=amber, success=live,
error=danger), auto-dismiss ~4s, with a close icon. Never use it for blocking errors.

### 8.11 Empty / offline state

Centered, low-key: a mono glyph, `--amber-dim` headline, one-line `--amber-muted` hint, and
a single Primary action. Used when no sources/instance/connection.

---

## 9. Iconography

- Line/mono icons only, drawn in `currentColor` so they inherit amber. No filled, no
  multicolor, **no emoji anywhere.**
- Prefer simple geometric glyphs (▮ ◉ ● ○ ▲ ▸ ■ ⏻) and a consistent line-icon set
  (e.g. Lucide) sized 16–20px.
- Never use placeholder icons. The shed logo is used as-is at the brand mark only.

---

## 10. Accessibility (mandatory)

- **Contrast:** every text/background pair meets **WCAG AA** — 4.5:1 normal, 3:1 for ≥18px.
  `--amber` (#FFB000) on `--bg`/`--panel` passes; `--amber-muted` is for ≥14px non-essential
  text only. Verify on change.
- **Focus:** visible amber focus ring on every interactive element; never `outline:none`
  without a replacement.
- **Targets:** interactive elements ≥ 44×44px (icon buttons included).
- **Semantics:** real `<button>`, `<input>` with associated `<label>`, `<nav>`, headings.
  ARIA only where semantics fall short.
- **Motion:** all effects respect `prefers-reduced-motion` and the reduce-effects toggle (§5).
- **Keyboard:** full tab order; dialogs trap focus; `Esc` closes overlays.

---

## 11. CSS variable reference

Drop this into `system/web/src/app.css` (`@layer base :root`) and mirror in the portal.
Components reference variables only.

```css
:root {
  color-scheme: dark;

  /* surfaces */
  --bg: #120D07;
  --panel: #160F08;
  --panel-raised: #1B140A;
  --well: #0A0A0A;
  --border: #4A3717;
  --border-dim: #3A2C13;

  /* amber ink */
  --amber: #FFB000;
  --amber-bright: #FFD479;
  --amber-dim: #C8861F;
  --amber-muted: #7A5A1E;

  /* status */
  --live: #3CA643;        --live-glow: #46E06A;
  --warning: #F2E96D;
  --danger: #EF4444;      --danger-glow: #FF5A4D;
  --transition: #C8861F;

  /* effects */
  --glow-amber: 0 0 6px rgba(255,176,0,0.5);
  --glow-live:  0 0 6px rgba(70,224,106,0.4);
  --scanlines: repeating-linear-gradient(0deg, rgba(0,0,0,0.18) 0 1px, transparent 1px 3px);

  /* geometry */
  --radius-sm: 2px;
  --radius-md: 4px;

  /* type */
  --font-mono: 'JetBrains Mono','IBM Plex Mono',ui-monospace,'SF Mono',Menlo,monospace;
  --font-led:  'DSEG7 Classic','JetBrains Mono',monospace;
}

:root[data-fx="off"] { --glow-amber: none; --glow-live: none; --scanlines: none; }
@media (prefers-reduced-motion: reduce) { :root { --scanlines: none; } }
```

---

## 12. Tailwind v4 integration (`system/web`)

`system/web` uses Tailwind v4 (`@import 'tailwindcss'`) with styles centralized in
`src/app.css`. Implement the system there:

- Define the tokens above in `@layer base`.
- Expose them to utilities via `@theme` (e.g. `--color-amber`, `--color-panel`) so
  `text-amber`, `bg-panel`, `border-border` work.
- Re-author the shared `@layer components` classes to the new system. Mapping from 1.0:

| Old class | New |
|-----------|-----|
| `.card-glass` | `.panel` (rack module, §8.1) |
| `.btn-primary` (gradient) | `.btn` + `.btn--primary` amber outline (§8.2) |
| `.nav-item` / `.active` | `.rack-item` / `.rack-item--active` (§8.7) |
| `.badge-success` / `.badge-muted` | `.pill--live` / `.pill--idle` (§8.4) |
| `.glow-brand` | removed (use `--glow-amber` on readouts only) |

Restyle the `ui/` primitives (`button`, `card`, `checkbox`, `dialog`) to match before
touching pages.

---

## 13. Do / Don't

**Do**
- Treat every screen as a labeled control panel.
- Reserve green/red/yellow strictly for live/record/warning meaning.
- Keep amber text at AA contrast; verify after changes.
- Make fields obviously recessed and buttons obviously pressable.

**Don't**
- Add a light mode, `dark:` variants, or `prefers-color-scheme: light`.
- Use gradients, blurred drop shadows, neon, or animated flicker.
- Use emoji or placeholder/filled icons.
- Put amber text on a colored status fill.
- Restyle a component inline when a canonical one exists.

---

## 14. Rollout checklist (`system/web` first)

- [ ] Add fonts (JetBrains Mono, DSEG7) and §11 tokens to `app.css`
- [ ] Wire `@theme` + re-author shared component classes (§12)
- [ ] Restyle `ui/` primitives (button, card, checkbox, dialog)
- [ ] App shell: status bar, rack nav, panel grid, popout chrome (§7)
- [ ] Pages: dashboard/switcher, sources, scenes, destinations, library, delay, settings, keys
- [ ] Popouts: program, preview, sources, audio
- [ ] Add "reduce effects" toggle + `prefers-reduced-motion` (§5)
- [ ] Verify every form end-to-end against a running core (create/update/delete/toggle)
- [ ] AA contrast + keyboard/focus audit on all pages
- [ ] Then: apply the same system to `commercial/portal`

---

*This document is the single source of truth for Muxshed visual design. Changes are made
here first, then in code.*
