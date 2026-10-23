import { useRef, useState } from "preact/hooks";
import { backups, send, show, toast } from "../../store";
import { Modal, Toggle, confirmDialog, promptDialog } from "../../ui";
import { lang, t, tp } from "../../i18n";
import { tErr } from "../../i18n/errors";
import { absoluteUrl, fmtBytes, fmtWhen, reportPath, saveFromUrl, uploadFile, type CsvResult, type MvrResult, type ReportKind } from "../../lib/data";

const BTN = { minHeight: 44 } as const;

// ── Reports ──────────────────────────────────────────────────────────
const REPORTS: { kind: ReportKind; title: string; hint: string }[] = [
  { kind: "patch", title: "Patch list", hint: "Every fixture with universe, address, channels and position." },
  { kind: "channels", title: "Channel hookup", hint: "DMX address map per universe." },
  { kind: "cues", title: "Cue sheets", hint: "Cues of every cuelist with times and notes." },
  { kind: "power", title: "Power report", hint: "Wattage per universe and total, with amps at 230 V." },
  { kind: "scenes", title: "Scenes & palettes", hint: "Overview of scenes and palettes." },
];

export function ReportsDialog(props: { onClose: () => void }) {
  const [busy, setBusy] = useState<string | null>(null);
  const name = (show.value?.name ?? "show").replace(/[^\w\-]+/g, "_");
  return (
    <Modal title={t("Reports")} width={640} onClose={props.onClose} footer={<button class="btn tall" style={BTN} onClick={props.onClose}>{t("Close")}</button>}>
      <div class="dim">{t("PDF reports are generated from the current show.")}</div>
      {REPORTS.map((r) => {
        const path = reportPath(r.kind, lang.value);
        return (
          <div key={r.kind} class="panel panel-body row" style={{ gap: 12, flexWrap: "wrap" }}>
            <div class="grow" style={{ minWidth: 180 }}><b>{t(r.title)}</b><div class="dim" style={{ fontSize: "var(--fs-sm)" }}>{t(r.hint)}</div></div>
            <a class="btn tall" style={BTN} href={path} target="_blank" rel="noreferrer">{t("Open PDF")}</a>
            <button class="btn tall" style={BTN} disabled={busy === r.kind} onClick={async () => {
              setBusy(r.kind);
              try { await saveFromUrl(path, `${name}-${r.kind}.pdf`); } catch (e) { toast(tErr((e as Error).message), "err"); }
              setBusy(null);
            }}>{busy === r.kind ? t("Working…") : t("Download")}</button>
            <button class="btn tall" style={BTN} title={absoluteUrl(path)} onClick={() => window.open(absoluteUrl(path), "_blank")}>{t("Open in browser")}</button>
          </div>
        );
      })}
    </Modal>
  );
}

// ── CSV patch import ────────────────────────────────────────────────
export function CsvImportDialog(props: { onClose: () => void }) {
  const file = useRef<HTMLInputElement>(null);
  const [res, setRes] = useState<CsvResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [replace, setReplace] = useState(false);
  const pick = async (f: File | undefined) => {
    if (!f) return;
    setBusy(true); setRes(null);
    try {
      const r = await uploadFile<CsvResult>(`/api/import/patch.csv${replace ? "?replace=1" : ""}`, f);
      setRes({ imported: r.imported ?? 0, created: r.created, updated: r.updated, removed: r.removed, errors: r.errors ?? [] });
      if (!(r.errors?.length)) toast(tp(r.imported ?? 0, "{n} fixture imported", "{n} fixtures imported"), "ok");
    } catch (e) {
      // 4xx bodies may carry {errors:[…]}
      try { const j = JSON.parse((e as Error).message); setRes({ imported: j.imported ?? 0, errors: j.errors ?? [{ line: 0, error: String(j.error ?? j) }] }); }
      catch { setRes({ imported: 0, errors: [{ line: 0, error: tErr((e as Error).message) }] }); }
    }
    setBusy(false);
  };
  return (
    <Modal title={t("Import patch CSV")} width={640} onClose={props.onClose} footer={<button class="btn tall" style={BTN} onClick={props.onClose}>{t("Close")}</button>}>
      <div class="dim">{t("Columns: id, name, profile, mode, universe, address, x, y, z. The import is all-or-nothing: if any line has an error nothing is changed.")}</div>
      <Toggle checked={replace} onChange={setReplace} label={t("Replace the current patch (otherwise the fixtures are added)")} />
      <div class="row" style={{ gap: 8, flexWrap: "wrap" }}>
        <button class="btn tall primary" style={BTN} disabled={busy} onClick={() => file.current?.click()}>{busy ? t("Working…") : t("Choose CSV file…")}</button>
        <button class="btn tall" style={BTN} onClick={() => saveFromUrl("/api/export/patch.csv", `${(show.value?.name ?? "show").replace(/[^\w\-]+/g, "_")}-patch.csv`).catch((e) => toast(tErr(e.message), "err"))}>{t("Download current patch as CSV")}</button>
      </div>
      <input ref={file} type="file" accept=".csv,text/csv" hidden onChange={(e) => { const el = e.target as HTMLInputElement; const f = el.files?.[0]; el.value = ""; void pick(f); }} />
      {res && (
        <div class="col" style={{ gap: 8 }}>
          <div class={`chip ${res.errors.length ? "err" : "ok"}`} style={{ alignSelf: "flex-start", minHeight: 32 }}>
            {res.errors.length ? t("Not imported — {n} error(s)", { n: res.errors.length }) : tp(res.imported, "{n} fixture imported", "{n} fixtures imported")}
          </div>
          {!res.errors.length && res.created !== undefined && (
            <div class="dim">{t("{c} created · {u} updated · {r} removed", { c: res.created ?? 0, u: res.updated ?? 0, r: res.removed ?? 0 })}</div>
          )}
          {res.errors.length > 0 && (
            <div class="scroll" style={{ maxHeight: 260 }}>
              <table class="tbl"><thead><tr><th style={{ width: 70 }}>{t("Line")}</th><th>{t("Error")}</th></tr></thead>
                <tbody>{res.errors.map((e, i) => <tr key={i}><td class="mono">{e.line || "—"}</td><td>{e.error}</td></tr>)}</tbody></table>
            </div>
          )}
        </div>
      )}
    </Modal>
  );
}

// ── MVR import / export ─────────────────────────────────────────────
export function MvrDialog(props: { onClose: () => void }) {
  const file = useRef<HTMLInputElement>(null);
  const [res, setRes] = useState<MvrResult | null>(null);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const list = (v: unknown): string[] => (Array.isArray(v) ? v.map((x) => (typeof x === "string" ? x : JSON.stringify(x))) : []);
  return (
    <Modal title={t("MVR (My Virtual Rig)")} width={640} onClose={props.onClose} footer={<button class="btn tall" style={BTN} onClick={props.onClose}>{t("Close")}</button>}>
      <div class="dim">{t("MVR exchanges the rig (fixtures, addresses, positions and GDTF files) with Vectorworks, Capture, Depence and consoles.")}</div>
      <div class="row" style={{ gap: 8, flexWrap: "wrap" }}>
        <button class="btn tall" style={BTN} onClick={() => saveFromUrl("/api/export/show.mvr", `${(show.value?.name ?? "show").replace(/[^\w\-]+/g, "_")}.mvr`).catch((e) => toast(tErr(e.message), "err"))}>{t("Export MVR")}</button>
        <button class="btn tall primary" style={BTN} disabled={busy} onClick={() => file.current?.click()}>{busy ? t("Working…") : t("Import MVR…")}</button>
        <button class="btn tall" style={BTN} onClick={() => window.open(absoluteUrl("/api/export/show.mvr"), "_blank")}>{t("Open in browser")}</button>
      </div>
      <input ref={file} type="file" accept=".mvr,application/zip" hidden onChange={async (e) => {
        const el = e.target as HTMLInputElement; const f = el.files?.[0]; el.value = "";
        if (!f) return;
        setBusy(true); setRes(null); setErr("");
        try { setRes(await uploadFile<MvrResult>("/api/import/mvr", f)); } catch (x) { setErr(tErr((x as Error).message)); }
        setBusy(false);
      }} />
      {err && <div class="chip err" style={{ alignSelf: "flex-start", height: "auto", padding: "6px 10px", whiteSpace: "normal" }}>{err}</div>}
      {res && (
        <div class="col" style={{ gap: 8 }}>
          <div class="chip ok" style={{ alignSelf: "flex-start", minHeight: 32 }}>{tp(Number(res.imported ?? 0), "{n} fixture imported", "{n} fixtures imported")}</div>
          {list(res.unmatched).length > 0 && <>
            <b>{t("Unmatched fixtures")} ({list(res.unmatched).length})</b>
            <div class="dim" style={{ fontSize: "var(--fs-sm)" }}>{t("No library profile matched. Import the GDTF files first (Patch → Import fixtures) and import the MVR again.")}</div>
            <div class="scroll mono" style={{ maxHeight: 180, fontSize: "var(--fs-sm)" }}>{list(res.unmatched).map((u, i) => <div key={i}>{u}</div>)}</div>
          </>}
          {res.created !== undefined && <div class="dim">{t("{c} created · {u} updated · {r} removed", { c: res.created ?? 0, u: res.updated ?? 0, r: 0 })}</div>}
          {list(res.skipped).length > 0 && <>
            <b>{t("Skipped")} ({list(res.skipped).length})</b>
            <div class="scroll mono" style={{ maxHeight: 120, fontSize: "var(--fs-sm)" }}>{list(res.skipped).map((u, i) => <div key={i}>{u}</div>)}</div>
          </>}
          {list(res.warnings).length > 0 && <>
            <b>{t("Warnings")} ({list(res.warnings).length})</b>
            <div class="scroll mono" style={{ maxHeight: 120, color: "var(--live)", fontSize: "var(--fs-sm)" }}>{list(res.warnings).map((u, i) => <div key={i}>{u}</div>)}</div>
          </>}
          {list(res.errors).length > 0 && <div class="scroll mono" style={{ maxHeight: 140, color: "var(--stop)", fontSize: "var(--fs-sm)" }}>{list(res.errors).map((u, i) => <div key={i}>{u}</div>)}</div>}
        </div>
      )}
    </Modal>
  );
}

// ── Backups ─────────────────────────────────────────────────────────
export function BackupsDialog(props: { onClose: () => void }) {
  const items = [...backups.value].sort((a, b) => b.at - a.at);
  return (
    <Modal title={t("Backups")} width={680} onClose={props.onClose}
      footer={<><button class="btn tall" style={BTN} onClick={props.onClose}>{t("Close")}</button>
        <button class="btn tall primary" style={BTN} onClick={async () => {
          const n = await promptDialog(t("Create backup"), t("Name"), show.value?.name ?? "");
          if (n) await send({ type: "backup_create", label: n }).then(() => toast(t("Backup created"), "ok", 1500)).catch(() => {});
        }}>{t("Create named backup")}</button></>}>
      <div class="dim">{t("Automatic backups are kept every few minutes (last 20). Named backups are never deleted automatically.")}</div>
      {items.length === 0 && <div class="faint" style={{ padding: 12 }}>{t("No backups yet")}</div>}
      <div class="scroll col" style={{ maxHeight: 360, gap: 6 }}>
        {items.map((b) => (
          <div key={b.id} class="panel panel-body row" style={{ gap: 10, flexWrap: "wrap" }}>
            <span class={`chip ${b.kind === "manual" ? "ok" : ""}`} style={{ minHeight: 28 }}>{b.kind === "manual" ? t("manual") : t("auto")}</span>
            <div class="grow" style={{ minWidth: 160 }}><b>{b.label || "—"}</b><div class="faint" style={{ fontSize: "var(--fs-sm)" }}>{fmtWhen(b.at)} · {fmtBytes(b.size)}</div></div>
            <button class="btn tall" style={BTN} onClick={async () => {
              if (await confirmDialog(t("Restore backup"), t("Restore “{name}”? A backup of the current state is saved first.", { name: b.label || fmtWhen(b.at) }), { ok: t("Restore") }))
                await send({ type: "backup_restore", id: b.id }).then(() => toast(t("Backup restored"), "ok")).catch(() => {});
            }}>{t("Restore")}</button>
            <button class="btn tall danger" style={BTN} aria-label={t("Delete")} onClick={async () => {
              if (await confirmDialog(t("Delete backup"), t("Delete “{name}”?", { name: b.label || fmtWhen(b.at) }), { danger: true, ok: t("Delete") }))
                await send({ type: "backup_delete", id: b.id }).catch(() => {});
            }}>{t("Delete")}</button>
          </div>
        ))}
      </div>
    </Modal>
  );
}
