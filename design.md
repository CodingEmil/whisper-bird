# Design: Whisper Bird

The locked system. Every Hallmark run reads this first and pages share it. Change it on purpose; the file is the rule.

## System
- Genre: playful
- Shape of the first page: app screen (a sidebar of 13 destinations, settings as groups of rows)
- Theme: brand, the kingfisher in `src/lib/bird.tsx` (Whisper Bird only; the Birdy Design package stays as it is)
- Axes: dark/grotesk-sans/chromatic-teal · posture: full-palette (field petrol, signal seafoam, seal apricot, ink cream)

## Tokens
`src/tokens.css` is the source of truth. The roles:
```css
:root {
  --color-chrome: oklch(24% 0.022 225);  /* the sidebar, a step under the paper */
  --color-paper: oklch(27% 0.026 225);  --color-paper-2: oklch(30.5% 0.028 225);  --color-paper-3: oklch(34.5% 0.03 225);
  --color-ink: oklch(94.5% 0.014 85);  --color-ink-2: oklch(85% 0.016 85);  --color-muted: oklch(75% 0.02 205);  --color-rule: oklch(38.5% 0.026 225);
  --color-edge: oklch(58% 0.026 222);  /* field edges, 3:1 on the wells */
  --color-accent: oklch(84% 0.1 175);  --color-accent-ink: oklch(25% 0.045 190);  --color-focus: oklch(87% 0.13 172);
  --color-accent-2: oklch(81% 0.115 62);  /* the seal: only "recording now" */
  --font-display: "Segoe UI Variable Display", "Segoe UI", system-ui;  --font-body: "Segoe UI Variable Text", "Segoe UI", system-ui;  --font-outlier: "Cascadia Code", ui-monospace;
  --radius-card: 14px;  --radius-input: 10px;  --radius-key: 7px;  --radius-pill: 999px;  --rule-hair: 1px;  --shadow-card: none;
}
```
The spacing scale, the type scale (ratio 1.2), the easings and the durations are the scaffold's and live in `tokens.css`.

## Surfaces
- A group is a heading outside a tinted well (`--color-paper-2`), its rows parted by hairlines. Never a card in a well.
- The sidebar groups the same way: a small muted heading over each group (Diktieren, Aufnahmen), settings and help at the foot above a hairline. In the icon rail a hairline stands in for the heading.
- Fields sit recessed (`--color-paper`) with an `--color-edge` border; focus is an outline, never a border change.
- No glass, no shadows on the page. Only the floating overlay and the edit menu carry one shadow, because they sit over other apps.

## Controls
- Primary: seafoam fill, accent ink, pill, 40px (32px small). One per screen.
- Secondary: `--color-paper-3` fill, pill, same heights. Ghost: text only. Delete: secondary, rose text.
- Seal: apricot fill, only to stop a recording (meeting, overlay).
- Selection (segmented, nav): a seafoam wash, never a fill.
- A shortcut is always drawn as keycaps, in the recorder as in running text.

## Motion
- Character: calm. State only (`--dur-micro`, `--dur-short`), `transform` and `opacity`; no reveal; under reduced motion everything ends at once. The bird moves like a real one: head, body, tail and eye are separate parts, the feet stay put, the head snaps between held poses. It pecks while working (leans in, two quick pecks, tail up as counterweight) and sings along to the voice (beak opens, head lifts a little). At rest (the bubble, the bird on Home) every 5–12 s it looks around, bobs like a kingfisher or preens; the sidebar bird stays still. While a meeting records, the bubble wears a breathing apricot ring that glows with the audio level.

## Exports
`src/tokens.css` in this project. Tailwind, DTCG or shadcn blocks are added here on request, per `export-formats.md`.
