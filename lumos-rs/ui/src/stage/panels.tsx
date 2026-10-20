// Editor chrome for the 3D workspace: toolbar, object palette/list, properties.
import { signal } from "@preact/signals";
import type { ComponentChildren } from "preact";
import { useRef } from "preact/hooks";
import { library, selection, send, toast } from "../store";
import type { Fixture, MediaKind, StageObject } from "../types";
import {
  TEMPLATES, aimTool, camPreset, editFixture, editMode, editObject, editStage, effFixtures, effStage, gizmoMode,
  haze, hiddenObjs, nextObjectId, selObject, showBeams, showGrid, showLabels, showPools, snapOn, stats,
  toggleHidden, workLight, bloomOn, quality, showAudience, showMannequin, venueMode, type CamPreset, type QualityPref, type VenueMode,
} from "./state";
import { hasStream, startCapture, stopCapture, streamVersion, uploadMedia } from "./media";

export const leftOpen = signal(typeof window === "undefined" ? true : window.innerWidth > 1000);
export const rightOpen = signal(typeof window === "undefined" ? true : window.innerWidth > 1000);

const r3 = (n: number) => Math.round(n * 1000) / 1000;

function Num(props: { value: number; onChange: (v: number) => void; step?: number; min?: number; max?: number; width?: number; title?: string }) {
  return (
    <input
      class="input num st-num" type="number" step={props.step ?? 0.1} min={props.min} max={props.max} title={props.title}
      style={props.width ? { width: props.width } : undefined}
      value={r3(props.value)}
      onInput={(e) => { const v = parseFloat((e.target as HTMLInputElement).value); if (Number.isFinite(v)) props.onChange(v); }}
    />
  );
}

function Vec3(props: { label: string; value: [number, number, number]; step?: number; onChange: (i: number, v: number) => void }) {
  return (
    <div class="st-vec">
      <span class="label">{props.label}</span>
      {[0, 1, 2].map((i) => (
        <Num key={i} value={props.value[i]} step={props.step} onChange={(v) => props.onChange(i, v)} title={"xyz"[i]} />
      ))}
    </div>
  );
}

function Tip(props: { children: ComponentChildren }) { return <div class="st-hint">{props.children}</div>; }

// ── toolbar ─────────────────────────────────────────────
const PRESETS: { id: CamPreset; label: string; title: string }[] = [
  { id: "audience", label: "Audience", title: "View from the audience" },
  { id: "top", label: "Top", title: "Plan view" },
  { id: "side", label: "Side", title: "Side elevation" },
  { id: "iso", label: "3/4", title: "Three-quarter view" },
  { id: "free", label: "Free", title: "Free camera (remembered)" },
];

export function Toolbar(props: { onFullscreen: () => void }) {
  const edit = editMode.value;
  const Tg = (p: { on: boolean; set: (v: boolean) => void; label: string; title?: string }) => (
    <button class={`btn sm ${p.on ? "sel" : ""}`} title={p.title} aria-pressed={p.on} onClick={() => p.set(!p.on)}>{p.label}</button>
  );
  return (
    <div class="st-toolbar">
      <div class="st-group">
        <button class={`btn sm ${edit ? "sel" : ""}`} onClick={() => (editMode.value = true)} title="Edit stage">Edit</button>
        <button class={`btn sm ${!edit ? "sel" : ""}`} onClick={() => { editMode.value = false; aimTool.value = false; }} title="Clean preview">View</button>
      </div>
      <div class="st-group">
        {PRESETS.map((p) => (
          <button key={p.id} class={`btn sm ${camPreset.value === p.id ? "sel" : ""}`} title={p.title} onClick={() => (camPreset.value = p.id)}>{p.label}</button>
        ))}
      </div>
      {edit && (
        <div class="st-group">
          {(["translate", "rotate", "scale"] as const).map((m, i) => (
            <button key={m} class={`btn sm ${gizmoMode.value === m && !aimTool.value ? "sel" : ""}`} title={`${m} (${"WER"[i]})`}
              onClick={() => { gizmoMode.value = m; aimTool.value = false; }}>{["Move", "Rotate", "Scale"][i]}</button>
          ))}
          <Tg on={snapOn.value} set={(v) => (snapOn.value = v)} label="Snap" title="0.1 m / 15°" />
          <Tg on={aimTool.value} set={(v) => (aimTool.value = v)} label="◎ Aim" title="Click the stage to aim selected moving heads (A)" />
        </div>
      )}
      <div class="st-group">
        <Tg on={showBeams.value} set={(v) => (showBeams.value = v)} label="Beams" />
        <Tg on={showPools.value} set={(v) => (showPools.value = v)} label="Pools" />
        <Tg on={showGrid.value} set={(v) => (showGrid.value = v)} label="Grid" />
        <Tg on={showLabels.value} set={(v) => (showLabels.value = v)} label="IDs" />
        <Tg on={workLight.value} set={(v) => (workLight.value = v)} label="Work light" title="Bright ambient light for editing" />
        <Tg on={bloomOn.value} set={(v) => (bloomOn.value = v)} label="Bloom" title="HDR glow around bright lights and LED screens" />
        <Tg on={showMannequin.value} set={(v) => (showMannequin.value = v)} label="Scale" title="1.75 m figure for scale" />
        <Tg on={showAudience.value} set={(v) => (showAudience.value = v)} label="House" title="Audience seating" />
        <select class="input st-sel" aria-label="Venue" title="Venue shell" value={venueMode.value} onChange={(e) => (venueMode.value = (e.target as HTMLSelectElement).value as VenueMode)}>
          <option value="theatre">Theatre</option>
          <option value="blackbox">Black box</option>
          <option value="cyc">Cyclorama</option>
          <option value="open">Open</option>
        </select>
        <select class="input st-sel" aria-label="Quality" title="Render quality" value={quality.value} onChange={(e) => (quality.value = (e.target as HTMLSelectElement).value as QualityPref)}>
          <option value="auto">Quality: auto</option>
          <option value="low">Low</option>
          <option value="med">Medium</option>
          <option value="high">High</option>
        </select>
        <label class="st-haze" title="Haze: makes beams more visible">
          <span class="label">Haze</span>
          <input type="range" min="0" max="1" step="0.01" value={haze.value} onInput={(e) => (haze.value = parseFloat((e.target as HTMLInputElement).value))} />
        </label>
      </div>
      <span class="sp" />
      {edit && (
        <div class="st-group st-narrow-only">
          <button class={`btn sm ${leftOpen.value ? "sel" : ""}`} onClick={() => (leftOpen.value = !leftOpen.value)}>Objects</button>
          <button class={`btn sm ${rightOpen.value ? "sel" : ""}`} onClick={() => (rightOpen.value = !rightOpen.value)}>Props</button>
        </div>
      )}
      <span class="st-hud mono" title="fps · draw calls · real lights">{stats.value.fps} fps · {stats.value.calls} dc · {stats.value.lights} L · {stats.value.q}</span>
      <button class="btn sm" onClick={props.onFullscreen} title="Fullscreen">⛶</button>
    </div>
  );
}

// ── left: add + objects + fixtures ──────────────────────
const KIND_ICON: Record<string, string> = {
  truss: "▤", platform: "▭", wall: "▮", column: "●", curtain: "≋", tree: "▲", box: "■", screen: "▣",
};

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
    <aside class="st-side st-left">
      <div class="panel-head">Add to stage</div>
      <div class="st-addgrid">
        {TEMPLATES.map((t, i) => <button key={t.label} class="btn sm" onClick={() => add(i)}>{t.label}</button>)}
      </div>
      <div class="panel-head">Objects <span class="faint">{objs.length}</span></div>
      <div class="st-list scroll">
        {objs.length === 0 && <div class="st-empty dim">No objects yet. Add trusses, decor and screens above.</div>}
        {objs.map((o) => (
          <div key={o.id} class={`st-row ${selObject.value === o.id ? "sel" : ""} ${hidden.has(o.id) ? "off" : ""}`}
            onClick={() => { selObject.value = o.id; }}>
            <span class="st-ico">{KIND_ICON[o.kind] ?? "■"}</span>
            <span class="grow st-name">{o.name}</span>
            <button class="btn ghost sm icon" title={hidden.has(o.id) ? "Show" : "Hide"} onClick={(e) => { e.stopPropagation(); toggleHidden(o.id); }}>{hidden.has(o.id) ? "◌" : "◉"}</button>
            <button class="btn ghost sm icon" title="Duplicate" onClick={(e) => { e.stopPropagation(); duplicateObject(o.id); }}>⧉</button>
            <button class="btn ghost sm icon" title="Delete" onClick={(e) => { e.stopPropagation(); deleteObject(o.id); }}>✕</button>
          </div>
        ))}
      </div>
      <div class="panel-head">Fixtures <span class="faint">{fixtures.length}</span>
        <span class="sp" />
        <button class="btn sm" title="Place selected (or unplaced) fixtures along the first truss" onClick={arrangeFixtures}>Arrange on truss</button>
      </div>
      <div class="st-list scroll st-fxlist">
        {fixtures.length === 0 && <div class="st-empty dim">Patch fixtures to see them here.</div>}
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
    c.id = nid; c.name = src.name + " copy"; c.pos = [c.pos[0] + 0.5, c.pos[1], c.pos[2] + 0.5];
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
  if (!list.length) { toast("Select fixtures (or patch new ones) to arrange", "info"); return; }
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
  toast(`Arranged ${list.length} fixture${list.length > 1 ? "s" : ""}`, "ok");
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
      <div class="label">Media source</div>
      <select class="input" value={m.kind} onChange={(e) => set({ kind: (e.target as HTMLSelectElement).value as MediaKind, src: "" })}>
        {MEDIA_KINDS.map((k) => <option key={k.v} value={k.v}>{k.l}</option>)}
      </select>
      {m.kind === "color" && <input type="color" class="st-color" value={m.src || "#ffffff"} onInput={(e) => set({ src: (e.target as HTMLInputElement).value })} />}
      {(hasFile || m.kind === "url") && (
        <input class="input" placeholder={m.kind === "url" ? "http://…/stream.mjpg or .mp4" : "/media/file or URL"} value={m.src}
          onChange={(e) => set({ src: (e.target as HTMLInputElement).value })} />
      )}
      {hasFile && (
        <div class="row">
          <input ref={file} type="file" hidden accept={m.kind === "image" ? "image/*" : "video/*"}
            onChange={async (e) => {
              const f = (e.target as HTMLInputElement).files?.[0];
              if (!f) return;
              try { set({ src: await uploadMedia(f) }); toast("Uploaded " + f.name, "ok"); } catch (err) { toast((err as Error).message, "err"); }
            }} />
          <button class="btn sm" onClick={() => file.current?.click()}>Upload…</button>
        </div>
      )}
      {cap && (
        <div class="row">
          {hasStream(m.kind as "webcam" | "screen")
            ? <button class="btn sm danger" onClick={() => stopCapture(m.kind as "webcam" | "screen")}>Stop capture</button>
            : <button class="btn sm go" onClick={() => startCapture(m.kind as "webcam" | "screen").catch((e) => toast((e as Error).message, "err"))}>Start capture…</button>}
          <span class="faint st-hint-inline">{hasStream(m.kind as "webcam" | "screen") ? "live" : "no signal"}</span>
        </div>
      )}
      {m.kind === "screen" && <Tip>Browsers can't receive NDI directly: open NDI Monitor (or the Resolume / OBS / Watchout output window) and capture that window here.</Tip>}
      {(m.kind === "video" || m.kind === "url") && (
        <label class="check"><input type="checkbox" checked={m.looped} onChange={(e) => set({ looped: (e.target as HTMLInputElement).checked })} /> Loop</label>
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
      <div class="field"><label>Name</label><input class="input" value={o.name} onChange={(e) => up((x) => { x.name = (e.target as HTMLInputElement).value; })} /></div>
      <div class="chip">{o.kind}</div>
      <Vec3 label="Position m" value={o.pos} onChange={(i, v) => up((x) => { x.pos[i] = v; })} />
      <Vec3 label="Rotation °" value={o.rot} step={5} onChange={(i, v) => up((x) => { x.rot[i] = v; })} />
      <Vec3 label={o.kind === "truss" ? "Len / side / side" : "Size W H D"} value={o.size} onChange={(i, v) => up((x) => { x.size[i] = Math.max(0.02, v); })} />
      <div class="row">
        <span class="label">{isScreen && o.screen_type === "projection" ? "Surface" : "Colour"}</span>
        <input type="color" class="st-color" value={o.color.length === 7 ? o.color : "#888888"} onInput={(e) => up((x) => { x.color = (e.target as HTMLInputElement).value; })} />
      </div>
      {isScreen && (
        <>
          <div class="field"><label>Type</label>
            <select class="input" value={o.screen_type ?? "led"} onChange={(e) => up((x) => { x.screen_type = (e.target as HTMLSelectElement).value; })}>
              <option value="led">LED panel (emissive)</option>
              <option value="projection">Projection (receives light)</option>
            </select>
          </div>
          <div class="field"><label>Brightness {o.brightness.toFixed(2)}</label>
            <input type="range" min="0" max="2.5" step="0.05" value={o.brightness} onInput={(e) => up((x) => { x.brightness = parseFloat((e.target as HTMLInputElement).value); })} />
          </div>
          <MediaEditor o={o} />
        </>
      )}
      <div class="row">
        <button class="btn sm" onClick={() => duplicateObject(o.id)}>Duplicate</button>
        <button class="btn sm danger" onClick={() => deleteObject(o.id)}>Delete</button>
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
        <div class="field"><label>Name</label><input class="input" value={f.name} onChange={(e) => each((x) => { x.name = (e.target as HTMLInputElement).value; })} /></div>
      )}
      <div class="chip">{multi ? `${props.fxs.length} fixtures` : `#${f.id} · U${f.universe}.${f.address}`}</div>
      {!multi && <div class="dim st-small">{prof ? `${prof.manufacturer} ${prof.model} — ${f.mode}` : f.profile}</div>}
      <Vec3 label="Position m" value={f.pos} onChange={(i, v) => each((x) => { x.pos[i] = v; })} />
      <Vec3 label="Rotation °" value={f.rot} step={5} onChange={(i, v) => each((x) => { x.rot[i] = v; })} />
      <div class="row">
        <button class={`btn sm ${aimTool.value ? "sel" : ""}`} onClick={aim}>◎ Aim at point</button>
        <button class="btn sm" title="Pan/tilt to centre" onClick={() => props.fxs.forEach((x) => send({ type: "prog_set", fixtures: [x.id], attrs: { pan: 0.5, tilt: 0.5 } }, { quiet: true }))}>Home</button>
      </div>
      {aimTool.value && <Tip>Click on the floor or any object to aim the selected moving heads (writes pan/tilt to the programmer).</Tip>}
    </div>
  );
}

export function RightPanel() {
  const stage = effStage();
  const obj = selObject.value !== null ? stage?.objects.find((o) => o.id === selObject.value) : undefined;
  const fxs = effFixtures().filter((f) => selection.value.includes(f.id));
  return (
    <aside class="st-side st-right">
      <div class="panel-head">Properties</div>
      <div class="scroll grow st-propwrap">
        {obj ? <ObjectProps o={obj} /> : fxs.length ? <FixtureProps fxs={fxs} /> : (
          <div class="st-empty dim">Select an object or a fixture in the viewport or the lists.<br /><br />
            <span class="faint">W / E / R move · rotate · scale<br />A aim · Del delete · Ctrl+D duplicate</span></div>
        )}
      </div>
    </aside>
  );
}
