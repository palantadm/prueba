// Editor chrome for the 3D workspace: toolbar, object palette/list, properties.
import { signal } from "@preact/signals";
import type { ComponentChildren } from "preact";
import { useRef } from "preact/hooks";
import { library, selection, send, toast } from "../store";
import { t, tp } from "../i18n";
import type { Fixture, MediaKind, StageObject } from "../types";
import {
  TEMPLATES, aimTool, camPreset, editFixture, editMode, editObject, editStage, effFixtures, effStage, gizmoMode,
  haze, hiddenObjs, nextObjectId, selObject, showBeams, showGrid, showLabels, showPools, snapOn, stats,
  toggleHidden, workLight, bloomOn, smokeCurrents, smokeDebug, smokeDiss, smokeHvac, smokeInfo, smokeOpen, smokeSliceY, smokeVent, smokeWind, quality, showAudience, showMannequin, venueMode, type CamPreset, type QualityPref, type VenueMode,
} from "./state";
import { getSmoke } from "./smoke";
import { coarsePointer, openPad } from "./numpad";
import { hasStream, startCapture, stopCapture, streamVersion, uploadMedia } from "./media";

export const leftOpen = signal(typeof window === "undefined" ? true : window.innerWidth > 1000);
export const rightOpen = signal(typeof window === "undefined" ? true : window.innerWidth > 1000);

const r3 = (n: number) => Math.round(n * 1000) / 1000;

/** Touch-size mono cell. On coarse pointers it opens the numeric keypad sheet instead of the OS keyboard. */
function Num(props: { value: number; onChange: (v: number) => void; step?: number; min?: number; max?: number; title?: string; label?: string; axis?: string }) {
  const step = props.step ?? 0.1;
  return (
    <label class="st-cell">
      {props.axis && <span class="st-ax">{props.axis}</span>}
      <input
        class="st-num mono" type="number" step={step} min={props.min} max={props.max} title={props.title} aria-label={props.label}
        value={r3(props.value)}
        onFocus={(e) => {
          if (!coarsePointer()) return;
          (e.target as HTMLInputElement).blur();
          openPad({ label: props.label ?? props.title ?? "", value: props.value, step, apply: props.onChange });
        }}
        onInput={(e) => { const v = parseFloat((e.target as HTMLInputElement).value); if (Number.isFinite(v)) props.onChange(v); }}
      />
    </label>
  );
}

function Vec3(props: { label: string; value: [number, number, number]; step?: number; onChange: (i: number, v: number) => void }) {
  return (
    <div class="st-vec">
      <div class="st-h">{t(props.label)}</div>
      <div class="st-vec-row">
        {[0, 1, 2].map((i) => (
          <Num key={i} value={props.value[i]} step={props.step} axis={"XYZ"[i]} title={"xyz"[i]} label={`${t(props.label)} ${"XYZ"[i]}`} onChange={(v) => props.onChange(i, v)} />
        ))}
      </div>
    </div>
  );
}

function Tip(props: { children: ComponentChildren }) { return <div class="st-hint">{props.children}</div>; }

function Range(p: { value: number; min: number; max: number; step: number; onInput: (v: number) => void; label?: string }) {
  const pct = ((p.value - p.min) / (p.max - p.min)) * 100;
  return (
    <input class="st-range" type="range" min={p.min} max={p.max} step={p.step} value={p.value} aria-label={p.label}
      style={{ "--p": `${pct}%` }} onInput={(e) => p.onInput(parseFloat((e.target as HTMLInputElement).value))} />
  );
}

// ── toolbar ─────────────────────────────────────────────
const PRESETS: { id: CamPreset; label: string; title: string }[] = [
  { id: "audience", label: "Audience", title: "View from the audience" },
  { id: "top", label: "Overhead", title: "Plan view" },
  { id: "side", label: "Side", title: "Side elevation" },
  { id: "iso", label: "3/4", title: "Three-quarter view" },
  { id: "free", label: "Free", title: "Free camera (remembered)" },
];

const viewOpen = signal(false);

function Tg(p: { on: boolean; set: (v: boolean) => void; label: string; title?: string }) {
  return <button class={`btn ${p.on ? "sel" : ""}`} title={p.title ? t(p.title) : undefined} aria-pressed={p.on} onClick={() => p.set(!p.on)}>{t(p.label)}</button>;
}

/** Floating chrome over the viewport: edit/camera pill (top), HUD chip, tools + layers pills (bottom). */
export function Toolbar(props: { onFullscreen: () => void }) {
  const edit = editMode.value;
  return (
    <>
      <div class="st-top st-card">
        <div class="st-pill" role="group">
          <button class={`btn ${edit ? "sel" : ""}`} onClick={() => (editMode.value = true)} title={t("Edit stage")}>{t("Edit")}</button>
          <button class={`btn ${!edit ? "sel" : ""}`} onClick={() => { editMode.value = false; aimTool.value = false; }} title={t("Clean preview")}>{t("View")}</button>
        </div>
        <span class="st-vsep" />
        <div class="st-pill" role="group">
          {PRESETS.map((p) => (
            <button key={p.id} class={`btn ${camPreset.value === p.id ? "sel" : ""}`} title={t(p.title)} aria-pressed={camPreset.value === p.id} onClick={() => (camPreset.value = p.id)}>{t(p.label)}</button>
          ))}
        </div>
        {edit && (
          <div class="st-pill st-narrow-only" role="group">
            <button class={`btn ${leftOpen.value ? "sel" : ""}`} onClick={() => (leftOpen.value = !leftOpen.value)}>{t("Objects")}</button>
            <button class={`btn ${rightOpen.value ? "sel" : ""}`} onClick={() => (rightOpen.value = !rightOpen.value)}>{t("Props")}</button>
          </div>
        )}
        <button class="btn icon" onClick={props.onFullscreen} title={t("Fullscreen")} aria-label={t("Fullscreen")}>⛶</button>
      </div>
      <div class="st-hudrow">
        {stats.value.warn && <span class="st-warn" role="status" title={t("Pick a quality level to retry")}>⚠ {stats.value.warn}</span>}
        <span class="st-hud mono" title={t("fps · draw calls · real lights")}>{stats.value.fps} fps · {stats.value.calls} dc · {stats.value.lights} L · {stats.value.q}</span>
      </div>
      <div class="st-dockrow">
        {viewOpen.value && <ViewCard />}
        {edit && (
          <div class="st-pill st-card" role="group" aria-label={t("Tools")}>
            {(["translate", "rotate", "scale"] as const).map((m, i) => (
              <button key={m} class={`btn ${gizmoMode.value === m && !aimTool.value ? "sel" : ""}`} title={`${t(["Move", "Rotate", "Scale"][i])} (${"WER"[i]})`}
                aria-pressed={gizmoMode.value === m && !aimTool.value}
                onClick={() => { gizmoMode.value = m; aimTool.value = false; }}>{t(["Move", "Rotate", "Scale"][i])}</button>
            ))}
            <Tg on={snapOn.value} set={(v) => (snapOn.value = v)} label="Snap" title="0.1 m / 15°" />
            <Tg on={aimTool.value} set={(v) => (aimTool.value = v)} label="◎ Aim" title="Click the stage to aim selected moving heads (A)" />
          </div>
        )}
        <div class="st-pill st-card" role="group" aria-label={t("Layers")}>
          <Tg on={showBeams.value} set={(v) => (showBeams.value = v)} label="Beams" />
          <Tg on={showPools.value} set={(v) => (showPools.value = v)} label="Pools" />
          <Tg on={showGrid.value} set={(v) => (showGrid.value = v)} label="Grid" />
          <Tg on={showLabels.value} set={(v) => (showLabels.value = v)} label="IDs" />
          <Tg on={smokeOpen.value} set={(v) => (smokeOpen.value = v)} label="Smoke" title="Smoke & haze simulation" />
          <button class={`btn ${viewOpen.value ? "sel" : ""}`} aria-pressed={viewOpen.value} aria-label={t("Display options")} title={t("Display options")} onClick={() => (viewOpen.value = !viewOpen.value)}>⋯</button>
        </div>
      </div>
    </>
  );
}

/** Secondary display options (lighting aids, venue shell, quality, haze) that did not fit the layers pill. */
function ViewCard() {
  return (
    <div class="st-viewcard st-card" role="dialog" aria-label={t("Display options")}>
      <div class="st-h">{t("Display options")}</div>
      <div class="st-viewgrid">
        <Tg on={workLight.value} set={(v) => (workLight.value = v)} label="Work light" title="Bright ambient light for editing" />
        <Tg on={bloomOn.value} set={(v) => (bloomOn.value = v)} label="Bloom" title="HDR glow around bright lights and LED screens" />
        <Tg on={showMannequin.value} set={(v) => (showMannequin.value = v)} label="Scale" title="1.75 m figure for scale" />
        <Tg on={showAudience.value} set={(v) => (showAudience.value = v)} label="House" title="Audience seating" />
      </div>
      <div class="st-selrow">
        <select class="input" aria-label={t("Venue")} title={t("Venue shell")} value={venueMode.value} onChange={(e) => (venueMode.value = (e.target as HTMLSelectElement).value as VenueMode)}>
          <option value="theatre">{t("Theatre")}</option>
          <option value="blackbox">{t("Black box")}</option>
          <option value="cyc">{t("Cyclorama")}</option>
          <option value="open">{t("Open stage")}</option>
        </select>
        <select class="input" aria-label={t("Quality")} title={t("Render quality")} value={quality.value} onChange={(e) => (quality.value = (e.target as HTMLSelectElement).value as QualityPref)}>
          <option value="auto">{t("Quality: auto")}</option>
          <option value="low">{t("Low")}</option>
          <option value="med">{t("Medium")}</option>
          <option value="high">{t("High")}</option>
        </select>
      </div>
      <div class="st-row-field" title={t("Haze: makes beams more visible")}>
        <span class="st-lbl">{t("Haze")}</span>
        <Range value={haze.value} min={0} max={1} step={0.01} label={t("Haze")} onInput={(v) => (haze.value = v)} />
      </div>
    </div>
  );
}

function Seg<T extends string>(p: { value: T; set: (v: T) => void; opts: [T, string][] }) {
  return (
    <div class="st-pill st-seg" role="group">
      {p.opts.map(([v, l]) => (
        <button key={v} class={`btn ${p.value === v ? "sel" : ""}`} aria-pressed={p.value === v} onClick={() => p.set(v)}>{t(l)}</button>
      ))}
    </div>
  );
}

/** Smoke / haze simulation controls (the sim itself lives in smoke.ts and is shared with the 2D plan). */
export function SmokePanel() {
  const info = smokeInfo.value;
  const sm = getSmoke();
  const active = info.mean > 0.004;
  return (
    <div class="st-smoke st-card" role="dialog" aria-label={t("Smoke & haze simulation")}>
      <div class="st-cardhead">
        <div class="st-h">{t("Smoke")}</div>
        <span class="grow" />
        <span class={`chip ${active ? "ok" : ""}`}>{active ? t("Smoke active") : t("Clear air")}</span>
        <button class="btn ghost icon" aria-label={t("Close")} onClick={() => (smokeOpen.value = false)}>✕</button>
      </div>
      <div class="st-field"><span class="st-lbl">{t("Air currents")}</span>
        <Seg value={smokeCurrents.value} set={(v) => (smokeCurrents.value = v)} opts={[["off", "Off"], ["low", "Low"], ["med", "Medium"], ["high", "High"]]} /></div>
      <div class="st-row-field"><span class="st-lbl" title={t("0° = towards the audience")}>{t("Wind direction")} · {smokeWind.value}°</span>
        <Range value={smokeWind.value} min={0} max={359} step={5} label={t("Wind direction")} onInput={(v) => (smokeWind.value = Math.round(v))} /></div>
      <div class="st-field"><span class="st-lbl">{t("Air conditioning (HVAC)")}</span>
        <Seg value={smokeHvac.value} set={(v) => (smokeHvac.value = v)} opts={[["none", "None"], ["down", "Down-draft"], ["up", "Up-draft"]]} /></div>
      <div class="st-row-field"><span class="st-lbl">{t("Dissipation")} · ×{smokeDiss.value.toFixed(2)}</span>
        <Range value={smokeDiss.value} min={0.1} max={4} step={0.05} label={t("Dissipation")} onInput={(v) => (smokeDiss.value = v)} /></div>
      <button class="switch-row" role="switch" aria-checked={smokeVent.value} onClick={() => (smokeVent.value = !smokeVent.value)}>
        <span class={`switch ${smokeVent.value ? "on" : ""}`} /><span>{t("Room vent open")}</span>
      </button>
      <div class="st-btnrow">
        <button class="btn tall" onClick={() => sm.purge()}>{t("Purge room")}</button>
        <button class="btn tall danger" onClick={() => sm.reset()}>{t("Reset smoke")}</button>
      </div>
      <div class="st-field"><span class="st-lbl">{t("Show density")}</span>
        <Seg value={smokeDebug.value} set={(v) => (smokeDebug.value = v)} opts={[["off", "Off"], ["slice", "Slice"], ["volume", "Volume"]]} /></div>
      {smokeDebug.value === "slice" && (
        <div class="st-row-field"><span class="st-lbl">{t("Slice height")} · {smokeSliceY.value.toFixed(1)} m</span>
          <Range value={smokeSliceY.value} min={0.1} max={7} step={0.1} label={t("Slice height")} onInput={(v) => (smokeSliceY.value = v)} /></div>
      )}
      <div class="st-hint">{tp(info.machines, "{n} smoke machine patched", "{n} smoke machines patched")} · {t("Mean density")} {(info.mean * 100).toFixed(1)}% · {info.grid} · {info.stepMs.toFixed(1)} ms</div>
    </div>
  );
}

// ── left: add + objects + fixtures ──────────────────────
const KIND_ICON: Record<string, string> = {
  truss: "▤", platform: "▭", wall: "▮", column: "●", curtain: "≋", tree: "▲", box: "■", screen: "▣",
};

const leftTab = signal<"add" | "objects" | "fixtures">("add");

export function LeftPanel() {
  const stage = effStage();
  const objs = stage?.objects ?? [];
  const fixtures = effFixtures();
  const hidden = new Set(hiddenObjs.value);
  const add = (i: number) => {
    if (!stage) return;
    const tpl = TEMPLATES[i].make(stage);
    const id = nextObjectId();
    editStage((s) => { s.objects.push({ ...tpl, id }); }, 0);
    selObject.value = id;
  };
  return (
    <aside class="st-lcol st-card" aria-label={t("Add to stage")}>
      <div class="st-pill st-tabs" role="tablist">
        {([["add", t("Add")], ["objects", `${t("Objects")} ${objs.length}`], ["fixtures", `${t("Fixtures")} ${fixtures.length}`]] as const).map(([k, l]) => (
          <button key={k} role="tab" aria-selected={leftTab.value === k} class={`btn ${leftTab.value === k ? "sel" : ""}`} onClick={() => (leftTab.value = k)}>{l}</button>
        ))}
      </div>
      {leftTab.value === "add" && (
        <div class="st-addgrid">
          {TEMPLATES.map((tpl, i) => <button key={tpl.label} class="st-tile" onClick={() => add(i)}>{t(tpl.label)}</button>)}
        </div>
      )}
      {leftTab.value === "objects" && (
        <div class="st-list scroll">
          {objs.length === 0 && <div class="st-empty dim">{t("No objects yet. Add trusses, decor and screens above.")}</div>}
          {objs.map((o) => {
            const sel = selObject.value === o.id;
            return (
              <div key={o.id} class={`st-row ${sel ? "sel" : ""} ${hidden.has(o.id) ? "off" : ""}`} onClick={() => { selObject.value = o.id; }}>
                <span class="st-ico">{KIND_ICON[o.kind] ?? "■"}</span>
                <span class="grow st-name">{o.name}</span>
                <button class="btn ghost icon" title={hidden.has(o.id) ? t("Show object") : t("Hide object")} aria-label={hidden.has(o.id) ? t("Show object") : t("Hide object")} onClick={(e) => { e.stopPropagation(); toggleHidden(o.id); }}>{hidden.has(o.id) ? "◌" : "◉"}</button>
                {sel && <button class="btn ghost icon" title={t("Duplicate")} aria-label={t("Duplicate")} onClick={(e) => { e.stopPropagation(); duplicateObject(o.id); }}>⧉</button>}
                {sel && <button class="btn ghost icon" title={t("Delete")} aria-label={t("Delete")} onClick={(e) => { e.stopPropagation(); deleteObject(o.id); }}>✕</button>}
              </div>
            );
          })}
        </div>
      )}
      {leftTab.value === "fixtures" && (
        <>
          <button class="btn tall" title={t("Place selected (or unplaced) fixtures along the first truss")} onClick={arrangeFixtures}>{t("Arrange on truss")}</button>
          <div class="st-list scroll">
            {fixtures.length === 0 && <div class="st-empty dim">{t("Patch fixtures to see them here.")}</div>}
            {fixtures.map((f) => {
              const sel = selection.value.includes(f.id);
              return (
                <div key={f.id} class={`st-row ${sel ? "sel" : ""}`} onClick={(e) => {
                  selObject.value = null;
                  selection.value = e.shiftKey || e.ctrlKey ? (sel ? selection.value.filter((x) => x !== f.id) : [...selection.value, f.id]) : [f.id];
                }}>
                  <span class="st-ico mono faint">{f.id}</span>
                  <span class="grow st-name">{f.name}</span>
                  <span class="faint mono">{f.universe}.{f.address}</span>
                </div>
              );
            })}
          </div>
        </>
      )}
    </aside>
  );
}

export function deleteObject(id: number) {
  editStage((s) => { s.objects = s.objects.filter((o) => o.id !== id); }, 0);
  if (selObject.value === id) selObject.value = null;
}

export function duplicateObject(id: number) {
  const src = effStage()?.objects.find((o) => o.id === id);
  if (!src) return;
  const nid = nextObjectId();
  editStage((s) => {
    const c: StageObject = JSON.parse(JSON.stringify(src));
    c.id = nid; c.name = src.name + " " + t("copy"); c.pos = [c.pos[0] + 0.5, c.pos[1], c.pos[2] + 0.5];
    s.objects.push(c);
  }, 0);
  selObject.value = nid;
}

function arrangeFixtures() {
  const stage = effStage();
  if (!stage) return;
  const all = effFixtures();
  const sel = new Set(selection.value);
  let list = all.filter((f) => sel.has(f.id));
  if (!list.length) list = all.filter((f) => f.pos[0] === 0 && f.pos[1] === 0 && f.pos[2] === 0);
  if (!list.length) { toast(t("Select fixtures (or patch new ones) to arrange"), "info"); return; }
  const truss = stage.objects.find((o) => o.kind === "truss");
  let cx = 0, y = stage.size[2] - 0.6, z = -1, len = stage.size[0] * 0.8;
  if (truss) {
    cx = truss.pos[0]; z = truss.pos[2]; len = truss.size[0] * 0.92;
    y = truss.pos[1] - Math.max(0.15, truss.size[1]) / 2 - 0.02;
  }
  list.sort((a, b) => a.id - b.id);
  list.forEach((f, i) => {
    const x = list.length === 1 ? cx : cx - len / 2 + (len * i) / (list.length - 1);
    editFixture(f.id, (ff) => { ff.pos = [r3(x), r3(y), r3(z)]; }, 50 + i * 4);
  });
  toast(tp(list.length, "Arranged {n} fixture", "Arranged {n} fixtures"), "ok");
}

// ── right: properties ───────────────────────────────────
const MEDIA_KINDS: { v: MediaKind; l: string }[] = [
  { v: "none", l: "Test pattern" }, { v: "color", l: "Solid colour" }, { v: "image", l: "Image" }, { v: "video", l: "Video file" },
  { v: "url", l: "Stream URL (MJPEG / mp4 / HLS)" }, { v: "webcam", l: "Webcam" }, { v: "screen", l: "Screen / window capture (NDI, Resolume, OBS)" },
];

function MediaEditor(props: { o: StageObject }) {
  const o = props.o;
  const m = o.media ?? { kind: "none" as MediaKind, src: "", looped: true };
  const file = useRef<HTMLInputElement>(null);
  void streamVersion.value;
  const set = (patch: Partial<typeof m>) => editObject(o.id, (x) => { x.media = { ...m, ...patch }; });
  const cap = m.kind === "webcam" || m.kind === "screen";
  const hasFile = m.kind === "image" || m.kind === "video";
  return (
    <div class="col st-media">
      <div class="label">{t("Media source")}</div>
      <select class="input" value={m.kind} onChange={(e) => set({ kind: (e.target as HTMLSelectElement).value as MediaKind, src: "" })}>
        {MEDIA_KINDS.map((k) => <option key={k.v} value={k.v}>{t(k.l)}</option>)}
      </select>
      {m.kind === "color" && <input type="color" class="st-color" value={m.src || "#ffffff"} onInput={(e) => set({ src: (e.target as HTMLInputElement).value })} />}
      {(hasFile || m.kind === "url") && (
        <input class="input" placeholder={m.kind === "url" ? t("http://…/stream.mjpg or .mp4") : t("/media/file or URL")} value={m.src}
          onChange={(e) => set({ src: (e.target as HTMLInputElement).value })} />
      )}
      {hasFile && (
        <div class="row">
          <input ref={file} type="file" hidden accept={m.kind === "image" ? "image/*" : "video/*"}
            onChange={async (e) => {
              const f = (e.target as HTMLInputElement).files?.[0];
              if (!f) return;
              try { set({ src: await uploadMedia(f) }); toast(t("Uploaded {name}", { name: f.name }), "ok"); } catch (err) { toast((err as Error).message, "err"); }
            }} />
          <button class="btn sm" onClick={() => file.current?.click()}>{t("Upload…")}</button>
        </div>
      )}
      {cap && (
        <div class="row">
          {hasStream(m.kind as "webcam" | "screen")
            ? <button class="btn sm danger" onClick={() => stopCapture(m.kind as "webcam" | "screen")}>{t("Stop capture")}</button>
            : <button class="btn sm go" onClick={() => startCapture(m.kind as "webcam" | "screen").catch((e) => toast((e as Error).message, "err"))}>{t("Start capture…")}</button>}
          <span class="faint st-hint-inline">{hasStream(m.kind as "webcam" | "screen") ? t("live") : t("no signal")}</span>
        </div>
      )}
      {m.kind === "screen" && <Tip>{t("Browsers can't receive NDI directly: open NDI Monitor (or the Resolume / OBS / Watchout output window) and capture that window here.")}</Tip>}
      {(m.kind === "video" || m.kind === "url") && (
        <label class="check"><input type="checkbox" checked={m.looped} onChange={(e) => set({ looped: (e.target as HTMLInputElement).checked })} /> {t("Loop")}</label>
      )}
    </div>
  );
}

function ObjectProps(props: { o: StageObject }) {
  const o = props.o;
  const up = (mut: (x: StageObject) => void) => editObject(o.id, mut);
  const isScreen = o.kind === "screen";
  return (
    <div class="col st-props">
      <div class="field"><label>{t("Name")}</label><input class="input st-in" value={o.name} onChange={(e) => up((x) => { x.name = (e.target as HTMLInputElement).value; })} /></div>
      <div><span class="chip">{t(o.kind)}</span></div>
      <Vec3 label="Position m" value={o.pos} onChange={(i, v) => up((x) => { x.pos[i] = v; })} />
      <Vec3 label="Rotation °" value={o.rot} step={5} onChange={(i, v) => up((x) => { x.rot[i] = v; })} />
      <Vec3 label={o.kind === "truss" ? "Len / side / side" : "Size W H D"} value={o.size} onChange={(i, v) => up((x) => { x.size[i] = Math.max(0.02, v); })} />
      <div class="row st-colorrow">
        <span class="label">{isScreen && o.screen_type === "projection" ? t("Surface") : t("Colour")}</span>
        <input type="color" class="st-color" value={o.color.length === 7 ? o.color : "#888888"} onInput={(e) => up((x) => { x.color = (e.target as HTMLInputElement).value; })} />
      </div>
      {isScreen && (
        <>
          <div class="field"><label>{t("Type")}</label>
            <select class="input" value={o.screen_type ?? "led"} onChange={(e) => up((x) => { x.screen_type = (e.target as HTMLSelectElement).value; })}>
              <option value="led">{t("LED panel (emissive)")}</option>
              <option value="projection">{t("Projection (receives light)")}</option>
            </select>
          </div>
          <div class="field"><label>{t("Brightness")} {o.brightness.toFixed(2)}</label>
            <input type="range" min="0" max="2.5" step="0.05" value={o.brightness} onInput={(e) => up((x) => { x.brightness = parseFloat((e.target as HTMLInputElement).value); })} />
          </div>
          <MediaEditor o={o} />
        </>
      )}
      <div class="st-btnrow">
        <button class="btn tall" onClick={() => duplicateObject(o.id)}>{t("Duplicate")}</button>
        <button class="btn tall danger" onClick={() => deleteObject(o.id)}>{t("Delete")}</button>
      </div>
    </div>
  );
}

function FixtureProps(props: { fxs: Fixture[] }) {
  const f = props.fxs[0];
  const multi = props.fxs.length > 1;
  const prof = library.value.find((p) => p.id === f.profile);
  const each = (mut: (x: Fixture) => void) => props.fxs.forEach((x) => editFixture(x.id, mut));
  const aim = () => (aimTool.value = !aimTool.value);
  return (
    <div class="col st-props">
      {!multi && (
        <div class="field"><label>{t("Name")}</label><input class="input st-in" value={f.name} onChange={(e) => each((x) => { x.name = (e.target as HTMLInputElement).value; })} /></div>
      )}
      {!multi && <div class="dim st-small">{prof ? `${prof.manufacturer} ${prof.model} — ${f.mode}` : f.profile}</div>}
      <Vec3 label="Position m" value={f.pos} onChange={(i, v) => each((x) => { x.pos[i] = v; })} />
      <Vec3 label="Rotation °" value={f.rot} step={5} onChange={(i, v) => each((x) => { x.rot[i] = v; })} />
      <div class="st-btnrow">
        <button class={`btn tall ${aimTool.value ? "sel" : ""}`} aria-pressed={aimTool.value} title={t("◎ Aim at point")} onClick={aim}>{t("Aim")}</button>
        <button class="btn tall" title={t("Pan/tilt to centre")} onClick={() => props.fxs.forEach((x) => send({ type: "prog_set", fixtures: [x.id], attrs: { pan: 0.5, tilt: 0.5 } }, { quiet: true }))}>{t("Home")}</button>
        <button class="btn tall" title={t("Place selected (or unplaced) fixtures along the first truss")} onClick={arrangeFixtures}>{t("On truss")}</button>
      </div>
      {aimTool.value && <Tip>{t("Click on the floor or any object to aim the selected moving heads (writes pan/tilt to the programmer).")}</Tip>}
    </div>
  );
}

export function RightPanel() {
  const stage = effStage();
  const obj = selObject.value !== null ? stage?.objects.find((o) => o.id === selObject.value) : undefined;
  const fxs = effFixtures().filter((f) => selection.value.includes(f.id));
  const kicker = obj ? t(obj.kind) : fxs.length > 1 ? tp(fxs.length, "{n} fixture", "{n} fixtures") : fxs.length ? `${t("Fixture")} ${fxs[0].id} · U${fxs[0].universe}.${fxs[0].address}` : t("Properties");
  const title = obj ? obj.name : fxs.length === 1 ? fxs[0].name : "";
  return (
    <aside class="st-rprops st-card" aria-label={t("Properties")}>
      <div class="st-h">{kicker}</div>
      {title && <div class="st-title">{title}</div>}
      <div class="scroll st-propwrap">
        {obj ? <ObjectProps o={obj} /> : fxs.length ? <FixtureProps fxs={fxs} /> : (
          <div class="st-empty dim">{t("Select an object or a fixture in the viewport or the lists.")}<br /><br />
            <span class="faint">{t("W / E / R move · rotate · scale")}<br />{t("A aim · Del delete · Ctrl+D duplicate")}</span></div>
        )}
      </div>
    </aside>
  );
}
