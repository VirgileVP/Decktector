---
name: Decktective
description: Real-time visualizer for Pioneer Pro DJ Link decks, read by the people outside the booth.
colors:
  bg: '#08090b'
  bg-panel: '#0d0e12'
  bg-inset: '#0a0b0e'
  bg-idle-panel: '#0a0b0e'
  border: '#1d2027'
  border-strong: '#2a2e38'
  text: '#e8eaf0'
  text-muted: '#8b93a5'
  text-faint: '#565d6d'
  onair: 'rgb(255, 69, 58)'
  ok: 'rgb(48, 209, 88)'
  warn: '#ffd60a'
  master: '#ffffff'
  sync: '#b7c1d4'
  led-timecode: '#39ff14'
typography:
  headline:
    fontFamily: "Inter, 'Segoe UI', -apple-system, BlinkMacSystemFont, Roboto, Helvetica, Arial, sans-serif"
    fontSize: '15px'
    fontWeight: 500
    lineHeight: '18px'
    letterSpacing: '0.08em'
  title:
    fontFamily: "Inter, 'Segoe UI', -apple-system, BlinkMacSystemFont, Roboto, Helvetica, Arial, sans-serif"
    fontSize: '13px'
    fontWeight: 600
    lineHeight: 'normal'
    letterSpacing: '0.01em'
  body:
    fontFamily: "Inter, 'Segoe UI', -apple-system, BlinkMacSystemFont, Roboto, Helvetica, Arial, sans-serif"
    fontSize: '12px'
    fontWeight: 500
    lineHeight: 'normal'
    letterSpacing: 'normal'
  label:
    fontFamily: "Inter, 'Segoe UI', -apple-system, BlinkMacSystemFont, Roboto, Helvetica, Arial, sans-serif"
    fontSize: '10px'
    fontWeight: 600
    lineHeight: 'normal'
    letterSpacing: '0.1em'
  # The enumerated ramp behind the four named roles above — see Typography > Scale.
  scale:
    step-8: '8px'
    step-9: '9px'
    step-10: '10px'
    step-11: '11px'
    step-12: '12px'
    step-13: '13px'
    step-14: '14px'
    step-15: '15px'
    step-16: '16px'
    step-18: '18px'
rounded:
  xxs: '2px'
  xs: '3px'
  sm: '4px'
  ms: '5px'
  md: '6px'
  lg: '8px'
  toggle-track: '9px'
  badge-pill: '10px'
  pill: '50%'
spacing:
  xs: '2px'
  sm: '4px'
  md: '8px'
  lg: '12px'
  xl: '16px'
components:
  card-deck:
    backgroundColor: '{colors.bg-panel}'
    textColor: '{colors.text}'
    rounded: '{rounded.md}'
    padding: '8px 10px'
  card-deck-onair:
    backgroundColor: '{colors.bg-panel}'
    textColor: '{colors.onair}'
    rounded: '{rounded.md}'
    padding: '8px 10px'
  button-icon:
    backgroundColor: '{colors.bg-inset}'
    textColor: '{colors.text-faint}'
    rounded: '{rounded.sm}'
    padding: '0'
    height: '26px'
    width: '26px'
  chip-prop:
    backgroundColor: '{colors.bg-inset}'
    textColor: '{colors.text}'
    rounded: '{rounded.xs}'
    padding: '2px 7px'
---

# Design System: Decktective

## Overview

**Creative North Star: "The Broadcast Console"**

Decktective is read by the people around the booth, not by the DJ at the decks: a VJ keying a deck into a video mixer, a lighting operator glancing at a tablet, a technician verifying network state. The system reads as a piece of monitoring hardware built into software — a broadcast console, not a media player skin. Surfaces stay dark, dense and near-flat by default; light exists to carry meaning (on-air red, sync blue, a fixed hardware-green timecode LED) before it exists to decorate. Six selectable themes (neon, dark, light, contrast, amber, midnight) swap the palette's channel values without changing this structure — every surface, spacing and radius decision in this file holds across all six.

The system is mid-evolution: its current instrumented character (very small text, 3–4px radii, glow as the only depth cue) is being softened toward "doux et confiant" — the same console, held with a steadier hand. New and touched components should read a half-step warmer and a half-step more settled than the oldest code, without losing the console's density or its state-first color logic.

**Key Characteristics:**

- Dense, small-type, monitoring-console layout — legible at a glance, not browsed
- State communicated by color and glow (on-air red, sync blue-gray, ok green, warn amber) before it is communicated by position or size
- One color is exempt from theming: timecode green (`#39ff14`) always reads as a physical LED, in every theme
- Six swappable themes share one structural system; nothing here is theme-specific
- Currently flat-by-default with glow for state; introducing a real shadow scale for floating surfaces (Elevation & Depth) while keeping glow exclusively for state, never for elevation

## Colors

Every color below is a CSS custom property that six themes redefine; the hex/rgb values here are the **neon** theme (the default, `:root`). Prose describes the role; only the frontmatter value is normative for neon.

### Primary

- **On-Air Red** (`rgb(255, 69, 58)`, token `onair`): the single most important color in the system. Marks the live deck — border glow, `MASTER`/`SYNC`/`ON AIR` label, remaining-time text, remove/delete affordances. Never reused for anything that is not "this is live" or "this is destructive."

### Secondary

- **Sync Blue-Gray** (`#b7c1d4`, token `sync`): the interactive/selected accent — active settings tab, focused input border, active theme card, drag handles, the SYNC label. The system's "you are here / this is active" color, distinct from on-air's "this is live."

### Tertiary

- **Ok Green** (`rgb(48, 209, 88)`, token `ok`): network-connected state and the transport "playing" color. Narrow, state-only use.
- **Warn Amber** (`#ffd60a`, token `warn`): cued/paused transport, network-searching pulse, lineup warnings. Narrow, state-only use.
- **Timecode LED Green** (`#39ff14`, token `led-timecode`): fixed across all six themes, never derived from the palette. Reads as a real hardware LED, with its own glow (`text-shadow: 0 0 6px rgba(57, 255, 20, 0.55)`).

### Neutral

- **Void** (`#08090b`, token `bg`): app background.
- **Panel** (`#0d0e12`, token `bg-panel`): deck cards, settings dialog, popovers — anything that reads as a distinct surface.
- **Inset** (`#0a0b0e`, token `bg-inset`): recessed surfaces sitting inside a panel — waveform canvases, cue list, inputs, icon buttons.
- **Border** (`#1d2027`, token `border`) / **Border Strong** (`#2a2e38`, token `border-strong`): hairline dividers vs. interactive-element outlines.
- **Text** (`#e8eaf0`), **Text Muted** (`#8b93a5`), **Text Faint** (`#565d6d`): a strict three-step ladder — full text is reserved for what the operator must read first (deck title, current lineup artist, primary values); muted for secondary facts; faint for chrome, dividers, disabled and idle affordances.

### Named Rules

**The Fixed LED Rule.** Timecode green (`led-timecode`) and its glow are not theme tokens. They are constants, because a hardware timecode LED does not change color when you change your DJ software's skin.

**The State-Not-Decoration Rule.** `onair`, `ok`, `warn`, `sync` are reserved for state. A card, button or chip may borrow one of these colors only to say "this is that state," never for visual variety.

## Typography

**Body/UI Font:** Inter (with Segoe UI, system sans fallbacks)
**Numeric readouts:** same family, always `font-variant-numeric: tabular-nums` — clocks, BPM, times and countdowns never jitter horizontally as digits change.

**Character:** A dense instrument-panel voice: no display size exists at the base scale (the largest routine text is 15px), weight and letter-spacing carry hierarchy instead of size.

### Hierarchy

- **Headline** (500–600 weight, 15–16px, 0.08em tracking): the wall clock and timecode readout, and the panel-window zoomed variant (`.header-block-fit`, 16px base, itself scaled by fit/zoom) — the numbers the header is built around.
- **Title** (600–700 weight, 11–13px): deck track title, the on-air lineup artist, the active track-structure segment name — the "what is happening right now" layer.
- **Body** (500 weight, 12–13px): setting labels, deck sub-line (artist/album), base app text.
- **Label** (600–700 weight, 8–11px, 0.04–0.14em tracking, uppercase or tabular): settings section headers, tabs, table headers, prop/badge chips, cue/event markers — structural or micro-numeric, not content. This role deliberately spans the widest range: an 8-9px marker glyph and an 11px settings label are the same structural role at different local densities, not different roles.

### Scale

A single, intentionally fine-grained numeric ramp underlies the four roles above, tuned for a dense console read up close or scaled via panel-window zoom: **8px, 9px, 10px, 11px, 12px, 13px, 14px, 15px, 16px, 18px** (enumerated in the frontmatter as `typography.scale`). A new size should land on one of these steps (or extend the ramp deliberately and update this list in both places) rather than introduce an arbitrary in-between value. The 18px step is reserved for glyph-sized single-character controls (e.g. the settings dialog's `×` close glyph), not body or label text.

### Named Rules

**The Tabular Rule.** Any number an operator reads at a glance (clock, BPM, remaining time, countdown) is `tabular-nums`. No exceptions.

## Layout

Booth-first, dense by default. Base font 13px, most UI text 9–13px; this is deliberate — the surface is read up close (booth screen, keyed overlay) or scaled up by the panel-window zoom, not browsed at arm's length like a marketing page.

- **App shell**: fixed 55px header, flexible deck grid below (`flex: 1`, own scroll). Stacked view (one column, full detail), 3-column grid view (3×2 for six players) and 2-column quad view (secondary info hidden in both tiled views) are user-toggled.
- **Interface scale**: a single `--ui-scale` CSS variable, applied via `zoom` (reflow, not a transform) on `.app` and on detached panel content, so the whole interface — not just text — grows and shrinks together.
- **Detachable panels are a first-class layout, not a special case.** The clock, timecode, header block and any deck can pop into their own OS window; that window mirrors the main layout and gets its own `useFitToBox`-driven scale so content fills whatever size the operator resizes it to. Panel content must therefore be designed to scale as a block, not just reflow.
- **Responsive floor is 640px** (`@media max-width: 640px`), tuned for the remote view on a phone: the tiled views collapse to a single column, the cue list and track-structure rail hide (no room), the header's clock pair and lineup stack vertically instead of sitting side by side.
- **Spacing rhythm** is tight and mostly sub-16px: 2–4px between tightly related items (badge to label, cue rows), 6–10px between elements within a group, 12–18px between groups. Nothing in the working deck view exceeds ~18px of gap.

## Elevation & Depth

**Current state: flat by default, glow for state.** Panels sit on the background with a 1px border, no drop shadow; the only "lift" is the on-air deck's colored glow (`box-shadow: 0 0 12px rgba(onair, 0.28), inset 0 0 10px rgba(onair, 0.06)`) and the timecode LED's `text-shadow` glow. One exception already exists: the header block's settings popover has a true drop shadow (`0 4px 12px rgb(0 0 0 / 0.25)`) to separate it from whatever is behind it.

**Confirmed direction: add a real shadow scale for floating surfaces, without touching state glow.** Two systems, kept strictly separate:

- **State glow** (color-tinted, tied to `onair`/`ok`/`warn`/`sync`/LED green): means "this is in a state." Stays exactly as documented above.
- **Elevation shadow** (neutral, black-based, no color tint): means "this floats above the surface below it" — settings dialog, popovers, dropdown-style menus, panel-window chrome on hover. Extend the popover's existing `0 4px 12px rgb(0 0 0 / 0.25)` into a small scale (e.g. a lighter tier for hover affordances, the current tier for dialogs/popovers) rather than inventing an unrelated shadow language.

### Named Rules

**The Glow-Is-State Rule.** A colored, tinted shadow always means a state (on-air, sync-active, timecode-live). A neutral black shadow always means elevation (floating above the surface). Never mix the two on one element.

## Shapes

Small, consistent radius scale: `2px` (rail markers, day badge) → `3px` (icon buttons, chips, small inputs) → `4px` (settings fields, view-toggle) → `5–6px` (deck cards, theme cards, settings tabs' container) → `8px` (settings dialog) → `9–10px` (the toggle switch's own track and the device-kind pill badge, both sized to their element's half-height rather than the general scale). Fully round (`50%`) for status dots, the toggle knob, and the pop-out button's circular variant. Borders are 1px hairlines throughout (`border`/`border-strong`), never 2px+ except the active settings-tab underline (2px) and the rail's playhead/marker lines. No clipping or unusual silhouettes — every shape is a rounded rectangle, a pill, or a circle.

### Named Rules

**The One Border Weight Rule.** Structural borders are always 1px. Anything thicker (the 2px tab underline, the 2px rail markers) is a deliberate indicator, not a panel edge.

## Components

Current character is precise and instrumented (tight radii, fast 120–200ms transitions, glow-only state). Confirmed direction is **"doux et confiant"** — soft and confident: a touch more corner radius and more perceptible hover/focus transitions on new and touched components, without losing density or the state-color system. Apply this as new components are built or existing ones are revisited; it is not a mandate to batch-restyle everything documented below.

### Buttons

- **Shape:** 3–4px radius, 1px `border-strong` outline on the icon-button family (`.settings-button`, `.tc-source-buttons button`, `.panel-chrome-toggle`).
- **Default:** `bg-inset` background, `text-faint` icon/glyph color.
- **Hover:** color steps up to `text-muted` or `text`; background steps to `bg-inset`/`row-highlight`. Transition is color/background only, 120–150ms ease — no scale or shadow change.
- **Active/selected:** either a filled `border-strong` background (view toggle) or a state-color border + glow (timecode source button, panel toggle when live) — selection state always borrows the state-color system, never a generic "selected blue."

### Chips / Badges

- **Style:** `bg-inset` background, 1px `border-strong`, tiny radius (3px) or full pill (device-kind badge, 10px radius). Uppercase, tracked, 9–10px label type.
- **State:** color-coded by meaning (`device-kind-player` → sync blue, `device-kind-mixer` → warn amber), not by generic category color.

### Cards / Containers

- **Corner Style:** 6px (deck panel), 5px (theme card), 4px (rail viewport/blocks).
- **Background:** `bg-panel` for primary cards, `bg-idle-panel`/`bg-inset` for recessed or de-emphasized ones.
- **Shadow Strategy:** none at rest; on-air state adds the color glow described in Elevation & Depth. Confirmed direction adds a neutral elevation shadow only to floating cards (popovers, dialogs), not to in-flow cards like the deck panel.
- **Border:** 1px `border`, stepping to `border-strong` or a state color on hover/active.
- **Internal Padding:** 8–10px for panel-level containers, 4px for dense inset lists (cue list, event list).

### Inputs / Fields

- **Style:** `bg-inset` fill, 1px `border-strong`, 3px radius, `font: inherit`.
- **Focus:** border color steps to `sync` — the system's one focus indicator, used consistently across text fields, time fields, and (via `accent-color`) native sliders/checkboxes.
- **Toggle:** a custom pill switch (`bg-inset` track → `sync` when checked), not a native checkbox skin.

### Navigation

- **Settings tabs:** flat, no background, 2px bottom border in `sync` when active, `text-faint` → `text` on hover. Same pattern as the view toggle but underline-based instead of fill-based.

### The Deck Panel (signature component)

The unit the whole app is built around. On-air state is the loudest signal in the system (colored border + dual glow + state-colored badge text); idle decks deliberately recede (`--idle-bar-opacity`, a desaturating filter on their waveforms) so a glance sorts live from idle before reading anything else. This on/off-air contrast is the system's core visual idea and should be preserved by any new surface that shows deck state.

## Do's and Don'ts

### Do:

- **Do** keep timecode green (`#39ff14`) fixed across every theme — it represents real hardware, not app chrome.
- **Do** use `sync` as the only focus/active-selection color; don't introduce a second "selected" color.
- **Do** keep numeric readouts `tabular-nums` everywhere an operator reads a changing number at a glance.
- **Do** let idle decks visually recede (opacity/filter) so on-air decks read first in a quad or multi-deck view.
- **Do** design new floating surfaces (popovers, dialogs, menus) with a neutral elevation shadow, per the confirmed Elevation & Depth direction.
- **Do** nudge new/touched components toward slightly larger radii and more perceptible transitions ("doux et confiant"), while keeping the console's density.

### Don't:

- **Don't** use `onair`, `ok`, `warn`, or `sync` for decoration — they are reserved for state.
- **Don't** put a colored glow on something that is merely floating (a dialog, a popover) — glow means state, shadow means elevation.
- **Don't** design a primary surface (deck panel, header) assuming a close, well-lit view only — panel windows are keyed transparent over video and watched from a distance on the remote view; verify legibility at both extremes.
- **Don't** widen the responsive floor's assumptions — 640px is tuned specifically for the remote view's phone case, not a generic breakpoint.
