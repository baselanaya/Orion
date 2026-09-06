# Orion client — design v0.3 ("the breaker room", overhaul)

Status: **v0.5 BUILT** (2026-09-06 late night, user-requested "similar to
NordVPN"): full rewrite to the NordVPN-style two-pane layout — 880×600,
sidebar server list (profiles with mode tags + geolocated city), main view =
dotted world map generated at runtime from a bundled equirectangular raster
(sampled to ~7k dots) with real geolocated pins (home + exit, ipwho.is) and a
dashed route arc, circular power button with SVG status ring (grey/blue spin/
secured/red fault), calm status copy. Map-less option was declined by user.
The v0.2-v0.4 "breaker room" spec below is the design history; v0.5 keeps its
discipline layer (guidelines fixes, protocol-driven motion, honest states)
but replaces the instrument aesthetic with the commercial navy look.
Implemented in client/app/ui (index.html, style.css, main.js, assets/world.jpg).

**v0.4 revision (same night, user feedback "production ready instead of fancy
colors" + frame clipping):** window 420×680 with a non-clipping flex budget;
palette flattened to monochrome ink/ash/line — the amber/cyan mode-lock accent
is retired; color is semantic only (ok-green = secured states, alarm red =
faults); route canvas renders neutral (ink strokes when live, hairlines at
rest, red on fault); mode rail = plain segmented control with ink active
state. The breaker, state hierarchy, and layout rhythm carry the design.
v0.3 supersedes v0.2 after reviewing the live render (user screenshot) against
four skill lenses: design-taste-frontend, frontend-design,
web-design-guidelines, motion-design. Scope note unchanged: product UI — the
landing-page machinery of the skills does not apply; their discipline layers
do.

Design read: instrument-panel product UI for one privacy-conscious user,
dark-tech language, custom instrument aesthetic.
Dials: VARIANCE 6 · MOTION 4 · DENSITY 5.

## Diagnosis from the live render (what actually failed)

1. **Vertical overflow** — the actions row is clipped by the window edge.
   Fixed heights (route 216 + state 78px word + telemetry) exceed 640px with
   no flex to absorb.
2. **The Route hero has no presence** — 14px node squares and a 3-line fan
   with 12px spread render as a flattened diamond artifact in a sea of empty
   panel. The signature element got the least craft in the build.
3. **STANDBY is a type wall** — 78px/700 weight consumes a third of the
   window and pushes the composition apart. Boldness was spent in the wrong
   place.
4. **The breaker reads as a missing image** — an empty 34×64 rectangle with
   no rails, no lever visual, no label.
5. **No rhythm** — paddings 18/20/12/14, panels of unrelated heights,
   nothing on a shared grid. It reads as unstyled HTML, not an instrument.
6. **The exit readout — the product's payoff — is the least visible row.**
7. **No accent at rest** — the mode-lock accent system only exists when
   connected, so STANDBY looks dead grey instead of armed-and-quiet.

## v0.3 decisions

### Layout (the overflow fix + grid)

Window 400×640, single column, flex with explicit budget: masthead 40px,
mode rail 44px, route panel = FLEX (absorbs 200-260px), state row 88px,
telemetry 56px, actions 52px, gaps on a 12px rhythm. Nothing clips: the
route flexes, everything else is content-sized. Vertical padding 16px.
Hairline dividers between regions instead of boxed cards (structure as
information: the window is one instrument, not stacked widgets).

### The Route, rebuilt as the signature

- Nodes become real glyphs: 24px squares, 2px stroke, inner 6px dot; labels
  set beside-below in 10px mono caps with 8px clearance (no clipping).
- Wires: 2px accent with 1px hairline casing; geometry computed with real
  spacing — the ghost fan diverges ±16px across 40% of the segment, runs
  parallel through a boxed TOR plate (hairline box + mono label), converges.
  No diamond artifact.
- Blueprint texture: faint 24px dot grid + L-shaped corner registration
  marks inside the panel — instrument vernacular, structural not decorative.
- Route is the one bold element; everything around it quiets down.

### Type rebalance

- State word: 56px / weight 600 (from 78/700), letter-spacing 0.04em; sits
  in the state row with the breaker, one composition not two floats.
- Labels: 10px mono caps, 0.14em tracking (unchanged).
- Body/buttons: Hanken 13-14px.

### The breaker, rebuilt

40×72 housing, two inner rails, a lever block with three grip ridges that
travels top (OPEN) to bottom (LOCKED); FAULT = lever centered + alarm stroke
+ red LED pip on the housing. Position labels OPEN/LOCKED as 9px mono beside
the travel path. It must read as hardware at a glance.

### Accent at rest (mode-lock extended)

The selected mode's hue is visible in STANDBY, dimmed to 60%: active mode
segment, state-word underline, breaker lever edge, exit markers. Connecting
raises the lights to full accent + live pulses. The switch always previews
what you're arming.

### Mode rail

Single hairline container, three segments; the active segment carries its
mode hue (FAST amber / GHOST cyan) with a 2px underline that slides between
segments on switch (150ms). DBL = reserved segment with a lock glyph +
tooltip, visually present, not faded text.

### Telemetry + exit

- Telemetry: 3 columns split by hairlines, values 13px mono right-aligned,
  live handshake counter ticks each poll.
- Exit readout promoted to its own row above telemetry: status glyph, exit
  IP in 14px mono, copy button as a real 28px target. Cyan in ghost, amber
  in fast.

### Motion (motion-design skill: personality = Premium; signature easing
cubic-bezier(0.2, 0, 0, 1); durations 120 / 240 / 450ms)

- Connect choreography (staggered, total <600ms, hero first): breaker
  travels → segment 1 energizes → segment 2 + fan draw → exit scramble.
- Mode switch: underline slides 150ms; re-tint 240ms.
- Breaker: 300ms mechanical travel with 4% overshoot; FAULT = no overshoot,
  firm.
- Pulses unchanged (protocol-driven: keepalive heartbeat, throughput
  deltas).
- prefers-reduced-motion: everything collapses to state changes.

### Guidelines compliance (web-design-guidelines — audited post-build)

Focus-visible 2px accent rings, hit targets ≥28px, contrast verified
(ash-on-void 5.9:1, ink-on-void 16.9:1, accent-on-void >9:1), keyboard order
matches visual order, no pointer-only affordances.

## Explicitly deferred (unchanged)

Double visuals become REAL once the entry node exists (the state machine and
route already model the mode; DBL stays a reserved segment). Tor Browser
wiring, NEW ID action, tray mode, light theme.
