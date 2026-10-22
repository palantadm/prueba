import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import "../../styles/morph.css";
import type { Fixture, FixtureProfile, MorphPreview } from "../../types";
import { library, send, show, toast } from "../../store";
import { EmptyState, Modal, Select, VirtualList } from "../../ui";
import { collisions, footprint, profileById } from "../../lib/fixtures";
import { ATTRS } from "../../lib/shell-attrs";
import { tMorph } from "../../i18n/morph";
import { t, tp } from "../../i18n";

type Item = { h: string } | { p: FixtureProfile };

const one = (a: string) => ATTRS.find((x) => x.name === a)?.label ?? a;
const label = (a: string) => a.split("+").map(one).join(" + ");
const RANK: Record<string, number> = { direct: 0, emulated: 1, approx: 2 };
const KIND: Record<string, { cls: string; name: string; help: string }> = {
  direct: { cls: "ok", name: "direct", help: "Same attribute in both fixtures: the recorded values carry over unchanged." },
  emulated: { cls: "", name: "emulated", help: "The target has no such channel, so Lumos converts it for you (for example RGB → RGBW, CMY or the nearest colour-wheel slot)." },
  approx: { cls: "warn", name: "approximate", help: "Closest equivalent available; the result can look slightly different." },
};
const kindOf = (k: string) => KIND[k] ?? KIND.approx;

/** Swap the model of the selected fixtures keeping their ids, so cues, scenes, palettes and groups survive. */
export function MorphDialog(props: { ids: number[]; onClose: () => void }) {
  const fixtures = show.value?.fixtures ?? [];
  const chosen = useMemo(() => props.ids.map((id) => fixtures.find((f) => f.id === id)).filter((f): f is Fixture => !!f).sort((a, b) => a.id - b.id), [props.ids.join(","), fixtures]);
  const ids = chosen.map((f) => f.id);
  const profiles = [...(show.value?.profiles ?? []), ...library.value.filter((l) => !show.value?.profiles.some((p) => p.id === l.id))];
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<FixtureProfile | null>(null);
  const [modeName, setModeName] = useState("");
  const [repack, setRepack] = useState(false);
  const [prev, setPrev] = useState<MorphPreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [err, setErr] = useState("");
  const [open, setOpen] = useState<Set<number>>(new Set());
  const [help, setHelp] = useState(false);
  const [busy, setBusy] = useState(false);
  const seq = useRef(0);
  const lastErr = useRef("");

  const items = useMemo<Item[]>(() => {
    const f = q.trim().toLowerCase();
    const list = profiles.filter((p) => !f || `${p.manufacturer} ${p.model} ${p.category}`.toLowerCase().includes(f));
    list.sort((a, b) => a.manufacturer.localeCompare(b.manufacturer) || a.model.localeCompare(b.model));
    const out: Item[] = [];
    let last = "";
    for (const p of list) { if (p.manufacturer !== last) { out.push({ h: p.manufacturer }); last = p.manufacturer; } out.push({ p }); }
    return out;
  }, [q, profiles.length]);

  const mode = sel?.modes.find((m) => m.name === modeName) ?? sel?.modes[0];
  const newFp = footprint(mode);

  // Source summary: "Model · mode × n" groups
  const groups = useMemo(() => {
    const m = new Map<string, { text: string; n: number }>();
    for (const f of chosen) {
      const p = profileById(f.profile);
      const key = `${f.profile}|${f.mode}`;
      const g = m.get(key) ?? { text: `${p ? `${p.manufacturer} ${p.model}` : f.profile} · ${f.mode}`, n: 0 };
      g.n++; m.set(key, g);
    }
    return [...m.values()];
  }, [chosen]);
  const oldFp = chosen.reduce((n, f) => Math.max(n, footprint(profileById(f.profile)?.modes.find((x) => x.name === f.mode))), 0);
  const same = !!sel && !!mode && chosen.length > 0 && chosen.every((f) => f.profile === sel.id && f.mode === mode.name);

  // Address overlap / overflow after the swap (client-side check; the server stays authoritative)
  const addrIssue = useMemo(() => {
    if (!sel || !mode || !chosen.length) return "";
    const moved = new Set(ids);
    const first = chosen[0];
    let next = first.address;
    const after: Fixture[] = fixtures.filter((f) => !moved.has(f.id)).concat(chosen.map((f) => {
      let address = f.address, universe = f.universe;
      if (repack) { address = next; universe = first.universe; next += newFp; }
      return { ...f, profile: sel.id, mode: mode.name, address, universe };
    }));
    if (repack && next - 1 > 512) return t("Re-addressing {n} fixtures of {fp} channels from {addr} runs past channel 512.", { n: chosen.length, fp: newFp, addr: first.address });
    const before = collisions(fixtures);
    const bad = [...collisions(after)].filter((id) => !before.has(id));
    return bad.length ? tp(bad.length, "{n} fixture would end up with overlapping DMX addresses. Tick “Re-address consecutively” to tidy them.", "{n} fixtures would end up with overlapping DMX addresses. Tick “Re-address consecutively” to tidy them.") : "";
  }, [sel?.id, mode?.name, repack, ids.join(","), fixtures, library.value]);

  // Debounced live preview from the engine
  useEffect(() => {
    if (!sel || !mode || !ids.length) { setPrev(null); setErr(""); setLoading(false); return; }
    const my = ++seq.current;
    setLoading(true); setErr("");
    const h = setTimeout(async () => {
      try {
        const r = await fetch("/api/morph/preview", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ ids, profile: sel.id, mode: mode.name }) });
        if (!r.ok) {
          let msg = (await r.text()).trim();
          try { const j = JSON.parse(msg); msg = j.error ?? j.message ?? msg; } catch { /* plain text */ }
          throw new Error(msg || `HTTP ${r.status}`);
        }
        const j = (await r.json()) as MorphPreview;
        if (my === seq.current) { setPrev(j); setOpen(new Set()); }
      } catch (e) {
        if (my !== seq.current) return;
        const msg = tMorph(e instanceof Error ? e.message : String(e), label);
        setPrev(null); setErr(msg);
        if (msg !== lastErr.current) { lastErr.current = msg; toast(msg, "err"); }
      } finally {
        if (my === seq.current) setLoading(false);
      }
    }, 250);
    return () => clearTimeout(h);
  }, [sel?.id, mode?.name, ids.join(",")]);

  const apply = async () => {
    if (!sel || !mode || busy) return;
    setBusy(true);
    try {
      await send({ type: "fixture_morph", ids, profile: sel.id, mode: mode.name, repack });
      toast(t("Morph applied: {n} → {model}", { n: ids.length, model: `${sel.manufacturer} ${sel.model}` }), "ok");
      props.onClose();
    } catch { /* the failure is already toasted by send() */ }
    setBusy(false);
  };

  const cov = prev ? Math.round(Math.max(0, Math.min(1, prev.coverage)) * 100) : 0;
  const covColor = cov >= 90 ? "var(--go)" : cov >= 60 ? "var(--live)" : "var(--stop)";
  const toggle = (i: number) => setOpen((s) => { const n = new Set(s); n.has(i) ? n.delete(i) : n.add(i); return n; });

  return (
    <Modal title={t("Swap fixtures (Morph)")} width={900} onClose={props.onClose}
      footer={<>
        <span class="faint grow" style={{ fontSize: "var(--fs-xs)", textAlign: "left" }}>{t("Morph can't be undone: to go back, morph again to the original model. Cues, scenes, palettes and groups keep their values.")}</span>
        <button class="btn mp-btn" onClick={props.onClose}>{t("Cancel")}</button>
        <button class="btn primary mp-btn" disabled={!sel || !mode || same || busy || !ids.length} onClick={apply}>{t("Apply morph")}</button>
      </>}>
      <div class="mp-root">
        <div class="mp-pick">
          <div class="label">{t("1 · Selected fixtures")}</div>
          <div class="mp-src" aria-label={t("Selected fixtures")}>
            <span class="chip ok">{tp(chosen.length, "{n} fixture", "{n} fixtures")}</span>
            {groups.map((g) => <span key={g.text} class="chip">{g.text} × {g.n}</span>)}
          </div>
          <div class="label" style={{ marginTop: 4 }}>{t("2 · New model")}</div>
          <input class="input" placeholder={t("Search manufacturer, model, type…")} aria-label={t("Search library")} value={q} onInput={(e) => setQ((e.target as HTMLInputElement).value)} />
          <div class="panel">
            {items.length === 0 ? <EmptyState title={t("No match")} /> :
              <VirtualList class="grow" rowHeight={48} items={items} label={t("Fixture library")} render={(it) => "h" in it ?
                <div key={"h" + it.h} class="mp-head">{it.h}</div> :
                <div key={it.p.id} role="listitem" class={`mp-row ${sel?.id === it.p.id ? "sel" : ""}`} tabIndex={0}
                  onClick={() => { setSel(it.p); setModeName(""); }} onKeyDown={(e) => e.key === "Enter" && (setSel(it.p), setModeName(""))}>
                  <span class="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{it.p.model}</span>
                  {it.p.model3d && <span class="chip ok" title={t("Real 3D model and optics (GDTF)")}>3D</span>}
                  <span class="chip">{it.p.source ?? t("builtin")}</span>
                </div>} />}
          </div>
        </div>

        <div class="mp-main">
          {!sel || !mode ? <EmptyState title={t("Choose the new fixture model")} hint={t("Pick a model on the left to see how the current fixtures will be converted.")} /> : <>
            <div>
              <b style={{ fontSize: "var(--fs-lg)" }}>{sel.manufacturer} {sel.model}</b>
              <span class="dim"> · {sel.category}</span>
              <span class="chip" style={{ marginLeft: 8 }}>{sel.source ?? t("builtin")}</span>
              {sel.model3d && <span class="chip ok" style={{ marginLeft: 4 }}>3D</span>}
            </div>
            <Select label={t("Mode")} value={mode.name} options={sel.modes.map((m) => ({ value: m.name, label: t("{name} ({n} ch)", { name: m.name, n: footprint(m) }) }))} onChange={setModeName} />
            <label class="mp-check">
              <input type="checkbox" checked={repack} onChange={(e) => setRepack((e.target as HTMLInputElement).checked)} />
              <span>{t("Re-address consecutively")}<br /><span class="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("Packs the swapped fixtures one after another from the first one's address, in the same universe.")}</span></span>
            </label>

            <div class="row" style={{ justifyContent: "space-between" }}>
              <span class="label">{t("3 · What will change")}</span>
              <span class="mono dim">{t("Channels per fixture")}: {oldFp} → <b style={{ color: newFp > oldFp ? "var(--live)" : "var(--text)" }}>{prev?.footprint ?? newFp}</b></span>
            </div>
            {same && <div class="mp-warn">{t("These fixtures already use this model and mode.")}</div>}
            {addrIssue && <div class="mp-warn" role="alert">⚠ {addrIssue}</div>}
            {loading && !prev && <div class="dim" role="status">{t("Calculating preview…")}</div>}
            {err && <div class="mp-err" role="alert">{t("Preview failed: {msg}", { msg: err })}</div>}

            {prev && <div style={{ opacity: loading ? 0.6 : 1 }}>
              <div class="row" style={{ marginBottom: 6 }}>
                <span style={{ fontSize: "var(--fs-lg)", fontWeight: 700, color: covColor }}>{cov}%</span>
                <span class="dim">{t("of the used attributes survive in the new model")}</span>
              </div>
              <div class="mp-cov" role="progressbar" aria-valuenow={cov} aria-valuemin={0} aria-valuemax={100} aria-label={t("Coverage")}><i style={{ width: `${cov}%`, background: covColor }} /></div>

              {prev.warnings.filter((w) => !(addrIssue && /overlap/i.test(w))).map((w, i) => <div key={i} class="mp-warn" style={{ marginTop: 8 }} role="alert">⚠ {tMorph(w, label)}</div>)}

              <div class="row" style={{ margin: "12px 0 4px" }}>
                <span class="label grow">{t("Attribute mapping")}</span>
                <button class="btn mp-btn" aria-expanded={help} onClick={() => setHelp(!help)}>{help ? t("Hide help") : t("What do these mean?")}</button>
              </div>
              {help && <div class="mp-help">
                {Object.values(KIND).map((k) => <div key={k.name}><span class={`chip ${k.cls}`}>{t(k.name)}</span> {t(k.help)}</div>)}
              </div>}
              {prev.maps.length === 0 ? <div class="dim" style={{ padding: "8px 0" }}>{t("No attribute can be carried over.")}</div> :
                <table class="mp-map"><tbody>
                  {[...prev.maps].sort((a, b) => (RANK[a.kind] ?? 3) - (RANK[b.kind] ?? 3)).map((m, i) => {
                    const k = kindOf(m.kind), expanded = open.has(i);
                    return [
                      <tr key={i} class="mp-m" onClick={() => toggle(i)} aria-expanded={expanded} title={t(k.help)}>
                        <td style={{ width: 24, color: "var(--text-faint)" }}>{expanded ? "▾" : "▸"}</td>
                        <td>{label(m.from)}</td><td style={{ width: 24 }} class="dim">→</td><td>{label(m.to)}</td>
                        <td style={{ width: 110, textAlign: "right" }}><span class={`chip ${k.cls}`}>{t(k.name)}</span></td>
                      </tr>,
                      expanded && <tr key={`n${i}`} class="mp-note"><td colSpan={5}>{m.note ? tMorph(m.note, label) : t(k.help)}</td></tr>,
                    ];
                  })}
                </tbody></table>}

              {prev.dropped.length > 0 && <div style={{ marginTop: 12 }}>
                <div class="label">{t("Lost (the new model can't reproduce them)")}</div>
                <div class="mp-tags">{prev.dropped.map((a) => <span key={a} class="chip err">{label(a)}</span>)}</div>
                <div class="faint" style={{ fontSize: "var(--fs-xs)", marginTop: 4 }}>{t("Their values stay stored, so morphing back restores them.")}</div>
              </div>}
              {prev.unset.length > 0 && <div style={{ marginTop: 12 }}>
                <div class="label">{t("New attributes nobody drives (they stay at their defaults)")}</div>
                <div class="mp-tags">{prev.unset.map((a) => <span key={a} class="chip warn">{label(a)}</span>)}</div>
              </div>}
            </div>}
          </>}
        </div>
      </div>
    </Modal>
  );
}
