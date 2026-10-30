# Lumos design system (touch redesign)

Dark console UI built for touch. Reference boards: the Lumos design canvas (Live, Cues, Console, Stage, Settings, System).
Source of truth in code: `ui/src/styles/tokens.css` (variables) and `ui/src/styles/base.css` (primitives).

## Principles
1. **One colour, one meaning.** Amber = what you touched (programmer, primary action). Cyan = selection/focus. Green = GO/running/OK. Red = blackout/delete/error. Violet = chases/effects. Orange = beam/warning. Pink = colour. Lime = control.
2. **Touch first.** Minimum interactive size 48 px (`--touch`); primary controls 56–68 px (`--touch-lg`, `--touch-xl`). Never 44 px or less except inside a 48 px hit area. No hover-only information. Drag controls have a ± alternative.
3. **Numbers are monospace** (`.mono` / `.num`, JetBrains Mono, tabular figures). Everything else is Inter. Fonts ship with the app (`@fontsource`), no network.
4. **Always reachable.** GO/BLACKOUT/Master live in the bottom dock on every screen, at thumb height.

## Tokens (`tokens.css`)
| Group | Variables |
|---|---|
| Surfaces | `--ink-0` #0a0c10 app · `--ink-1` #10131a panels/bars · `--ink-2` #171b24 tiles/cards · `--ink-3` #1f2430 controls · `--ink-4` #2a3140 active · `--line`, `--line-strong` |
| Text | `--text` #e8ebf2 · `--dim` #a3abbc · `--mute` #8791a5 · `--on-accent`, `--on-amber`, `--on-green` |
| Meaning | `--amber` `--cyan` `--green` `--red` `--violet` `--orange` `--pink` `--lime` `--blue` |
| Tints | `--cyan-fill` `--amber-fill` `--green-fill` `--green-fill-2` `--red-fill` `--red-line` `--violet-fill` |
| Shape | `--r-sm` 10 · `--r-md` 12 · `--r-lg` 14 · `--r-xl` 18 · `--r-pill` |
| Size | `--touch` 48 · `--touch-lg` 56 · `--touch-xl` 68 · `--dock-h` 92 · `--topbar-h` 64 |
| Type | `--font-ui` Inter · `--font-mono` JetBrains Mono · `--fs-xs` 12 … `--fs-2xl` 28 |
| Spacing | `--s-1`…`--s-6` (4 px base) |

Legacy names (`--bg-0…4`, `--text-dim`, `--accent`, `--live`, `--go`, `--stop`, …) remain as aliases so unconverted workspaces keep working. **Use the new names in new code.**

## Components (`base.css`)
- **Buttons** `.btn` (48 px) · sizes `.sm` (44) `.lg` (56) `.xl` (68) `.icon` · variants `.primary` (amber) `.cyan` `.go` `.danger` `.ghost` `.on` (amber tint) `.sel` (cyan tint).
- **Inputs** `.input` (48 px; `.num` for numbers), `select.input`, `textarea.input`, `.field` + `label`, `.check` (24 px box), `.switch` (+ `.switch-row`), `.stepper` (− value +), `.readout`.
- **Segmented / tabs** `.seg` + `.seg-btn.active`, `.tabs` + `.tab.active` — pill group, cyan active.
- **Tile** `.tile` (+ `.sel`, `.held`, `.off`; children `.tile-id`, `.tile-name`, `.tile-bar > i`) in a `.tile-grid`. Used for fixtures, palettes, scenes, macros.
- **Faders** `.fader-h` / `.fader-v` with `.fill` and `.cap` (set `%` inline). Handles ≥ 18 px wide, track ≥ 52 px.
- **Panels** `.panel`, `.panel-head` (44 px label strip), `.panel-body`; `.page` / `.page-head` / `.page-body`; `.side` + `.side-item` (264 px rail).
- **Tables** `.tbl` rows ≥ 52 px; selected row `.sel` (cyan tint + bar).
- **Feedback** `.chip` (`.ok` `.err` `.warn`), `.dot`, `.banner`, `.toast`, `.modal`, `.menu`.
- **Shell** `.topbar` (64 px), `.dock` (92 px) with `.dock-btn` (78×68, `.active` = amber tint), `.dock-master` (grand master), `.btn-blackout` (176×68; `.active` blinks red), `.status-chip`.

## Touch rules
- `touch-action: manipulation` on all interactive elements; tap highlight disabled; `@media (pointer: coarse)` raises sliders and rows further.
- Target spacing ≥ 8 px. Use `Pointer Events` (`setPointerCapture`) for drag controls so mouse, pen and touch behave the same.
- Keyboard: every workspace keeps its number shortcut; sliders also respond to arrow keys.

## Migrating a workspace
1. Replace raw colours/sizes by tokens; use `.tile`, `.seg`, `.stepper`, `.switch`, `.fader-*` instead of ad-hoc controls.
2. Check every control ≥ 48 px (primary 56+) and that nothing depends on hover.
3. Numbers → `.mono`; labels → uppercase `.label`; keep all strings in `t()`.
4. Workspaces fill `main` (`height: 100%`, `min-height: 0`); the dock and command bar are provided by the shell.
