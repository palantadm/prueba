import { useState } from "preact/hooks";
import type { Capability, ChannelDef, FixtureProfile, Mode } from "../../types";
import { library, send, show, toast } from "../../store";
import { ALL_ATTRS } from "../../lib/attrs";
import { Modal, NumberField, Select, TextField } from "../../ui";
import { confirmDialog } from "../../ui";
import { t } from "../../i18n";

/* Write or correct a fixture profile by hand: modes, channels (attribute, DMX offset, fine channel, default, invert) and the
   ranges of wheels (colour / gobo). Custom profiles live in the show; the built-in library is read-only (duplicate it to start). */

const fail = (e: unknown) => toast(String((e as Error)?.message ?? e), "err");
const footprint = (m: Mode) => m.channels.reduce((n, c) => Math.max(n, c.offset + 1, (c.fine ?? -1) + 1), 0);
const blankChannel = (m: Mode): ChannelDef => ({ attr: "dimmer", offset: footprint(m), fine: null, default: 0, invert: false, capabilities: [] });
const blankProfile = (): FixtureProfile => ({
  id: "", manufacturer: "", model: "", category: "par", beam_angle: [25, 25], pan_range: 0, tilt_range: 0, power_w: 100, source: "custom", model3d: null,
  modes: [{ name: "4ch", channels: [{ attr: "dimmer", offset: 0, fine: null, default: 0, invert: false, capabilities: [] }, { attr: "red", offset: 1, fine: null, default: 0, invert: false, capabilities: [] }, { attr: "green", offset: 2, fine: null, default: 0, invert: false, capabilities: [] }, { attr: "blue", offset: 3, fine: null, default: 0, invert: false, capabilities: [] }] }],
});
const slug = (s: string) => s.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

export function ProfileEditorDialog(props: { onClose: () => void }) {
  const mine = (show.value?.profiles ?? []);
  const [p, setP] = useState<FixtureProfile | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [mi, setMi] = useState(0);
  const [base, setBase] = useState(library.value[0]?.id ?? "");

  if (!p) {
    return (
      <Modal title={t("Fixture profile editor")} width={560} onClose={props.onClose} footer={<button class="btn tall" onClick={props.onClose}>{t("Close")}</button>}>
        <div class="col" style={{ gap: 14 }}>
          <div class="dim">{t("Create a profile for a fixture that is not in the library, or correct a custom one. Built-in profiles cannot be changed: duplicate one to start from it.")}</div>
          <button class="btn tall primary" onClick={() => { setP(blankProfile()); setIsNew(true); setMi(0); }}>+ {t("New profile")}</button>
          <div class="row" style={{ gap: 8 }}>
            <div class="grow"><Select label={t("Duplicate from the library")} value={base} options={library.value.map((x) => ({ value: x.id, label: `${x.manufacturer} ${x.model}` }))} onChange={setBase} /></div>
            <button class="btn tall" disabled={!base} onClick={() => { const b = structuredClone(library.value.find((x) => x.id === base)!); b.id = `custom/${slug(b.model)}`; b.source = "custom"; b.model3d = null; setP(b); setIsNew(true); setMi(0); }}>{t("Duplicate")}</button>
          </div>
          <div class="cap">{t("Custom profiles in this show")}</div>
          {mine.length === 0 && <div class="dim">{t("None yet.")}</div>}
          {mine.map((x) => <button key={x.id} class="btn tall" style={{ justifyContent: "flex-start" }} onClick={() => { setP(structuredClone(x)); setIsNew(false); setMi(0); }}>{x.manufacturer} {x.model} <span class="dim mono">{x.id}</span></button>)}
        </div>
      </Modal>
    );
  }
  const mode = p.modes[Math.min(mi, p.modes.length - 1)];
  const up = (f: (q: FixtureProfile) => void) => { const q = structuredClone(p); f(q); setP(q); };
  const upMode = (f: (m: Mode) => void) => up((q) => f(q.modes[Math.min(mi, q.modes.length - 1)]));
  const save = () => void send({ type: "profile_save", profile: p }).then(() => { toast(t("Profile saved"), "ok", 2000); props.onClose(); }).catch(fail);
  const used = (show.value?.fixtures ?? []).filter((f) => f.profile === p.id).length;
  return (
    <Modal title={isNew ? t("New profile") : `${p.manufacturer} ${p.model}`} width={980} onClose={props.onClose}
      footer={<>
        {!isNew && <button class="btn tall danger" disabled={used > 0} title={used > 0 ? t("Used by {n} fixtures", { n: used }) : ""} onClick={async () => { if (await confirmDialog(t("Delete profile"), t("Delete this profile from the show?"), { danger: true, ok: t("Delete") })) void send({ type: "library_remove", id: p.id }).then(props.onClose).catch(fail); }}>{t("Delete")}</button>}
        <span class="grow" /><button class="btn tall" onClick={() => setP(null)}>{t("Back")}</button><button class="btn tall primary" onClick={save}>{t("Save profile")}</button></>}>
      <div class="col" style={{ gap: 12 }}>
        <div class="grid2">
          <TextField label={t("Manufacturer")} value={p.manufacturer} onChange={(v) => up((q) => { q.manufacturer = v; if (isNew) q.id = `${slug(v)}/${slug(q.model)}`; })} />
          <TextField label={t("Model")} value={p.model} onChange={(v) => up((q) => { q.model = v; if (isNew) q.id = `${slug(q.manufacturer)}/${slug(v)}`; })} />
          <TextField label={t("Id (lowercase, e.g. acme/par-led)")} mono value={p.id} disabled={!isNew} onChange={(v) => up((q) => { q.id = v; })} />
          <div class="field"><label>{t("Category")}</label><input class="input" list="pe-cats" value={p.category} onInput={(e) => up((q) => { q.category = (e.target as HTMLInputElement).value; })} /><datalist id="pe-cats">{["par", "wash", "spot", "profile", "led-bar", "moving-head", "strobe", "blinder", "dimmer", "fog", "other"].map((c) => <option value={c} />)}</datalist></div>
          <NumberField label={t("Power (W)")} value={p.power_w} min={0} max={20000} float onChange={(v) => up((q) => { q.power_w = v; })} />
          <div class="row" style={{ gap: 8 }}><NumberField label={t("Beam min (°)")} value={p.beam_angle[0]} min={1} max={180} float onChange={(v) => up((q) => { q.beam_angle[0] = v; if (q.beam_angle[1] < v) q.beam_angle[1] = v; })} /><NumberField label={t("Beam max (°)")} value={p.beam_angle[1]} min={1} max={180} float onChange={(v) => up((q) => { q.beam_angle[1] = Math.max(v, q.beam_angle[0]); })} /></div>
          <NumberField label={t("Pan range (°, 0 = fixed)")} value={p.pan_range} min={0} max={1440} onChange={(v) => up((q) => { q.pan_range = v; })} />
          <NumberField label={t("Tilt range (°)")} value={p.tilt_range} min={0} max={1440} onChange={(v) => up((q) => { q.tilt_range = v; })} />
        </div>

        <div class="row" style={{ gap: 6, flexWrap: "wrap" }}>
          {p.modes.map((m, i) => <button key={i} class={`btn ${i === mi ? "primary" : ""}`} onClick={() => setMi(i)}>{m.name || t("(no name)")} · {footprint(m)} ch</button>)}
          <button class="btn" onClick={() => { up((q) => q.modes.push({ name: `${q.modes.length + 1}`, channels: [{ attr: "dimmer", offset: 0, fine: null, default: 0, invert: false, capabilities: [] }] })); setMi(p.modes.length); }}>+ {t("Mode")}</button>
          <button class="btn" onClick={() => { up((q) => q.modes.push({ ...structuredClone(q.modes[mi]), name: `${q.modes[mi].name} 2` })); setMi(p.modes.length); }}>{t("Duplicate mode")}</button>
          <button class="btn danger" disabled={p.modes.length < 2} onClick={() => { up((q) => q.modes.splice(mi, 1)); setMi(0); }}>{t("Remove mode")}</button>
        </div>
        <TextField label={t("Mode name")} value={mode.name} onChange={(v) => upMode((m) => { m.name = v; })} />
        <div style={{ overflowX: "auto" }}><table class="tbl">
          <thead><tr><th style={{ width: 70 }}>{t("DMX ch")}</th><th style={{ minWidth: 150 }}>{t("Attribute")}</th><th style={{ width: 90 }}>{t("Fine ch")}</th><th style={{ width: 90 }}>{t("Default %")}</th><th style={{ width: 70 }}>{t("Invert")}</th><th style={{ width: 120 }}>{t("Ranges")}</th><th style={{ width: 150 }} /></tr></thead>
          <tbody>
            {mode.channels.map((c, i) => (
              <tr key={i}>
                <td><input class="input" type="number" min={1} max={512} value={c.offset + 1} onChange={(e) => upMode((m) => { m.channels[i].offset = Math.max(0, Number((e.target as HTMLInputElement).value) - 1); })} /></td>
                <td><input class="input" style={{ minWidth: 140 }} list="pe-attrs" value={c.attr} onInput={(e) => upMode((m) => { m.channels[i].attr = (e.target as HTMLInputElement).value.trim().toLowerCase(); })} /></td>
                <td><input class="input" type="number" min={0} max={512} placeholder="—" value={c.fine === null ? "" : c.fine + 1} onChange={(e) => upMode((m) => { const v = (e.target as HTMLInputElement).value; m.channels[i].fine = v ? Math.max(0, Number(v) - 1) : null; })} /></td>
                <td><input class="input" type="number" min={0} max={100} value={Math.round(c.default * 100)} onChange={(e) => upMode((m) => { m.channels[i].default = Math.max(0, Math.min(100, Number((e.target as HTMLInputElement).value))) / 100; })} /></td>
                <td><input type="checkbox" checked={c.invert} aria-label={t("Invert")} onChange={(e) => upMode((m) => { m.channels[i].invert = (e.target as HTMLInputElement).checked; })} /></td>
                <td><CapsButton c={c} onChange={(caps) => upMode((m) => { m.channels[i].capabilities = caps; })} /></td>
                <td class="row" style={{ gap: 4, flexWrap: "nowrap", minWidth: 150 }}>
                  <button class="btn" aria-label={t("Up")} disabled={i === 0} onClick={() => upMode((m) => { [m.channels[i - 1], m.channels[i]] = [m.channels[i], m.channels[i - 1]]; })}>▲</button>
                  <button class="btn" aria-label={t("Down")} disabled={i === mode.channels.length - 1} onClick={() => upMode((m) => { [m.channels[i + 1], m.channels[i]] = [m.channels[i], m.channels[i + 1]]; })}>▼</button>
                  <button class="btn danger" aria-label={t("Delete")} disabled={mode.channels.length < 2} onClick={() => upMode((m) => { m.channels.splice(i, 1); })}>✕</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table></div>
        <datalist id="pe-attrs">{ALL_ATTRS.map((a) => <option value={a} />)}</datalist>
        <div class="row" style={{ gap: 8 }}><button class="btn" onClick={() => upMode((m) => { m.channels.push(blankChannel(m)); })}>+ {t("Channel")}</button><span class="dim">{t("Footprint of this mode: {n} channels", { n: footprint(mode) })}</span></div>
      </div>
    </Modal>
  );
}

function CapsButton(props: { c: ChannelDef; onChange: (c: Capability[]) => void }) {
  const [open, setOpen] = useState(false);
  const caps = props.c.capabilities;
  const set = (i: number, patch: Partial<Capability>) => props.onChange(caps.map((k, j) => (j === i ? { ...k, ...patch } : k)));
  const split = (n: number) => {
    const w = Math.floor(256 / n);
    props.onChange(Array.from({ length: n }, (_, i) => ({ min: i * w, max: i === n - 1 ? 255 : (i + 1) * w - 1, label: `${i + 1}`, color: null, image: null, wheel: null, slot: i + 1 })));
  };
  return (
    <>
      <button class="btn" onClick={() => setOpen(true)}>{caps.length ? `${caps.length} ${t("ranges")}` : t("Add…")}</button>
      {open && (
        <Modal title={`${t("Ranges of")} ${props.c.attr}`} width={640} onClose={() => setOpen(false)} footer={<button class="btn tall primary" onClick={() => setOpen(false)}>{t("Done")}</button>}>
          <div class="col" style={{ gap: 10 }}>
            <div class="dim">{t("DMX value ranges (0–255) with a name: colour-wheel slots, gobos, shutter / strobe zones…")}</div>
            <div class="row" style={{ gap: 6, flexWrap: "wrap" }}>
              {[4, 6, 8, 12].map((n) => <button key={n} class="btn" onClick={() => split(n)}>{t("Split into {n} equal slots", { n })}</button>)}
              <button class="btn" onClick={() => props.onChange([...caps, { min: caps.length ? Math.min(255, caps[caps.length - 1].max + 1) : 0, max: 255, label: "", color: null, image: null, wheel: null, slot: null }])}>+ {t("Range")}</button>
              <button class="btn danger" disabled={!caps.length} onClick={() => props.onChange([])}>{t("Clear")}</button>
            </div>
            <table class="tbl"><thead><tr><th style={{ width: 80 }}>{t("From")}</th><th style={{ width: 80 }}>{t("To")}</th><th>{t("Name")}</th><th style={{ width: 90 }}>{t("Colour")}</th><th style={{ width: 50 }} /></tr></thead>
              <tbody>{caps.map((k, i) => (
                <tr key={i}>
                  <td><input class="input" type="number" min={0} max={255} value={k.min} onChange={(e) => set(i, { min: Math.max(0, Math.min(255, Number((e.target as HTMLInputElement).value))) })} /></td>
                  <td><input class="input" type="number" min={0} max={255} value={k.max} onChange={(e) => set(i, { max: Math.max(0, Math.min(255, Number((e.target as HTMLInputElement).value))) })} /></td>
                  <td><input class="input" value={k.label} onInput={(e) => set(i, { label: (e.target as HTMLInputElement).value })} /></td>
                  <td><input class="input" type="color" value={k.color ?? "#ffffff"} onInput={(e) => set(i, { color: (e.target as HTMLInputElement).value })} /></td>
                  <td><button class="btn danger" aria-label={t("Delete")} onClick={() => props.onChange(caps.filter((_, j) => j !== i))}>✕</button></td>
                </tr>
              ))}</tbody></table>
          </div>
        </Modal>
      )}
    </>
  );
}
