# Lumos design system

Dark, dense, console-style UI. Built for low-light rooms, mouse **and** touch (tablets on the LAN).
Source of truth: `ui/src/styles/tokens.css` (variables) and `ui/src/styles/base.css` (primitive classes).
Components live in `ui/src/ui/` and are exported from `ui/src/ui/index.ts`.

## Principles
1. **Tokens, never raw values.** Colours, spacing, radii and type come from CSS variables.
2. **Colour carries meaning.** Amber = live/programmer/warning, cyan = selection/focus, green = go/ok, red = stop/error.
   Attribute families have fixed hues (`--attr-intensity|position|color|beam|control|effect`) used everywhere an attribute appears.
3. **Touch-first sizing.** Primary controls are ≥ 40 px (`.btn.tall`, `--touch`); dense table controls may be 32 px.
4. **State is visible.** Every editor shows saved/unsaved; every output shows live status; nothing fails silently (toasts).
5. **Keyboard reachable.** All controls are real `button/input/select`; focus ring is `--accent`.

## Tokens
| Group | Variables |
|---|---|
| Surface | `--bg-0` app · `--bg-1` panel · `--bg-2` raised/header · `--bg-3` input · `--bg-4` hover · `--line`, `--line-strong` |
| Text | `--text`, `--text-dim`, `--text-faint`, `--text-on-accent` |
| State | `--accent`(+`-dim`) · `--live`(+`-dim`) · `--go`(+`-dim`) · `--stop`(+`-dim`) · `--info` |
| Attributes | `--attr-intensity` `--attr-position` `--attr-color` `--attr-beam` `--attr-control` `--attr-effect` |
| Space | `--s-1…--s-6` = 4, 8, 12, 16, 24, 32 px |
| Shape | `--r-sm 4` `--r-md 6` `--r-lg 10`, `--touch 40px` |
| Type | `--font-ui`, `--font-mono` (numbers, addresses, timecode); `--fs-xs 11 … --fs-xl 20` |
| Motion | `--dur 120ms`, `--ease` |

## Primitive classes (base.css)
`.row/.col/.grow/.sp/.wrap/.scroll` layout · `.panel/.panel-head/.panel-body` · `.btn` (+`primary go danger ghost on sel`, sizes `sm`, `tall`, `lg`, `icon`)
· `.input` (+`.num`, `.mono`) · `.field` · `.check` · `table.tbl` · `.chip` (+`ok warn err`) · `.dot` · `.tabs/.tab` · `.seg` · `.kbd` · `.vrow` (virtual row) · `.menu` · `.statusbar`.

## Components (`src/ui`)
| Component | Props | Notes / a11y |
|---|---|---|
| `Modal` | `title onClose footer? width?` | `role=dialog aria-modal`, Esc and backdrop close. |
| `Toasts` + `toast()` (store) | — | `aria-live=polite`; kinds info/ok/err. |
| `DialogHost`, `confirmDialog(title,msg,{danger,ok})`, `promptDialog(title,label,value)` | promise-based | Render `<DialogHost/>` once (App). |
| `Field` | `label hint? for?` | Label bound by `for`; hint in faint text. |
| `TextField` | `value onChange label? mono? placeholder? onCommit? invalid?` | `class="bare"` drops the label wrapper (table cells). `onCommit` fires on blur/Enter. |
| `NumberField` | `value onChange min max step float? label?` | Free typing, clamped on commit; ↑/↓ bump. |
| `Select<T>` | `value options[{value,label,disabled}] onChange label?` | Native `<select>` (best on touch). Numeric values stay numbers. |
| `Toggle` | `checked onChange label?` | Native checkbox. |
| `Segmented<T>` | `value options onChange label?` | `role=radiogroup`; 2–4 mutually exclusive choices. |
| `Tabs<T>` | `value options[{value,label,badge?}] onChange` | `role=tablist`. |
| `VirtualList<T>` | `items rowHeight render class?` | Fixed row height windowing; used for 256-universe and large fixture lists. |
| `EmptyState` | `title hint? icon? children` | Always offer the primary action as a child. |
| `PageHead` | `title sub? children` | Title + actions bar of every page. |
| `StatusDot` | `state: ok|err|warn|off title?` | Has `role=img` + label — never colour only. |
| `Kbd` | children | Shortcut hints. |

States: default · hover (`--bg-4`) · active (1 px press) · disabled (40 % opacity, no pointer) · focus-visible (2 px accent ring) · selected (`sel`, accent-dim) · error (red border / `.chip.err`).

## Patterns
* **Settings page** = `PageHead` (title + `SaveBar`) + body of `.panel`s. `useSettingsDraft()` gives `draft/patch/dirty/save/revert`; Save is disabled when clean.
* **Master/detail dialogs** edit a clone and send one command on Save.
* **Bulk operations** (assign outputs to universes 1–16, copy outputs) are explicit dialogs with a preview of first/last result.
* **Tables with many rows** use `VirtualList` + `.vrow`; selection with click, Ctrl/⌘ toggle, Shift range.

## Do / Don't
| Do | Don't |
|---|---|
| Use `var(--accent)` etc. | Hard-code hex values |
| Show live status next to every output/input | Hide connection errors in the console |
| Confirm destructive actions with `confirmDialog({danger:true})` | Delete on a single click |
| Use `.mono` for addresses, values, timecode | Mix proportional digits in tables |
| Keep primary actions ≥ 40 px | Make touch targets < 32 px |

## Keyboard
`1…7` workspaces · `Ctrl/⌘+B` blackout · `Ctrl/⌘+S` save show · `?` help · `Esc` close dialog.
