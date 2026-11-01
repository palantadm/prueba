import { useEffect, useState } from "preact/hooks";
import type { MergePart, MergeReport } from "../../types";
import { send, show, shows, toast } from "../../store";
import { Modal, Select, Toggle } from "../../ui";
import { t } from "../../i18n";

/* Merge parts of another saved show into this one. Preview first; it is one undo step (Ctrl+Z). */

const PARTS: { part: MergePart; label: () => string }[] = [
  { part: "fixtures", label: () => t("Patch (fixtures)") },
  { part: "groups", label: () => t("Groups") },
  { part: "palettes", label: () => t("Palettes") },
  { part: "scenes", label: () => t("Scenes") },
  { part: "cuelists", label: () => t("Cuelists") },
  { part: "chases", label: () => t("Chases") },
  { part: "effects", label: () => t("Effects") },
  { part: "macros", label: () => t("Macros") },
  { part: "timelines", label: () => t("Timelines") },
  { part: "scripts", label: () => t("Scripts") },
  { part: "locate", label: () => t("Locate settings") },
];

async function preview(name: string, parts: MergePart[]): Promise<MergeReport> {
  const r = await fetch("/api/show/merge/preview", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ name, parts }) });
  if (!r.ok) throw new Error(await r.text());
  return r.json();
}

export function MergeDialog(props: { onClose: () => void }) {
  const others = shows.value.filter((s) => s.name !== show.value?.name);
  const [name, setName] = useState(others[0]?.name ?? "");
  const [avail, setAvail] = useState<Record<string, number>>({});
  const [sel, setSel] = useState<Set<MergePart>>(new Set());
  const [rep, setRep] = useState<MergeReport | null>(null);
  const [err, setErr] = useState("");
  useEffect(() => {
    if (!name) return;
    setErr(""); setRep(null); setSel(new Set());
    preview(name, []).then((r) => setAvail(Object.fromEntries(r.available.map((c) => [c.part, c.count])))).catch((e) => setErr(String(e.message ?? e)));
  }, [name]);
  useEffect(() => {
    if (!name || sel.size === 0) { setRep(null); return; }
    preview(name, [...sel]).then(setRep).catch((e) => setErr(String(e.message ?? e)));
  }, [name, [...sel].sort().join(",")]);
  const toggle = (p: MergePart, on: boolean) => { const n = new Set(sel); if (on) n.add(p); else n.delete(p); setSel(n); };
  const go = async () => {
    await send({ type: "show_merge", name, parts: [...sel] });
    toast(t("Merged from “{n}”. Ctrl+Z undoes it.", { n: name }), "ok", 4000);
    props.onClose();
  };
  return (
    <Modal title={t("Merge from another show")} width={620} onClose={props.onClose}
      footer={<><button class="btn tall" onClick={props.onClose}>{t("Cancel")}</button><button class="btn tall primary" disabled={sel.size === 0 || !!err} onClick={() => void go().catch((e) => setErr(String(e.message ?? e)))}>{t("Merge")}</button></>}>
      {others.length === 0 ? <div class="dim">{t("There are no other saved shows. Save one first (Show → Save as…).")}</div> : (
        <div class="col" style={{ gap: 12 }}>
          <Select label={t("Take from")} value={name} options={others.map((s) => ({ value: s.name, label: s.name }))} onChange={setName} />
          <div class="dim">{t("Pick what to add to the current show. Clashing numbers are renumbered and everything that points to them follows.")}</div>
          <div class="grid2">
            {PARTS.map((x) => {
              const n = avail[x.part] ?? 0;
              return <Toggle key={x.part} checked={sel.has(x.part)} disabled={n === 0} onChange={(v) => toggle(x.part, v)} label={`${x.label()} (${n})`} />;
            })}
          </div>
          {err && <div style={{ color: "var(--red)" }}>⚠ {err}</div>}
          {rep && (
            <div class="panel"><div class="panel-body col" style={{ gap: 6 }}>
              <div><b>{t("It will add:")}</b> {rep.added.map((c) => `${PARTS.find((p) => p.part === c.part)?.label()} ${c.count}`).join(" · ") || t("nothing")}</div>
              {rep.warnings.map((w, i) => <div key={i} style={{ color: "var(--amber)" }}>⚠ {w}</div>)}
            </div></div>
          )}
        </div>
      )}
    </Modal>
  );
}
