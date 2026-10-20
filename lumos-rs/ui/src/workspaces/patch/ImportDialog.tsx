import { useRef, useState } from "preact/hooks";
import { Modal } from "../../ui";
import { toast } from "../../store";
import { t, tp } from "../../i18n";
import { tErr } from "../../i18n/errors";

type Result = { imported: { id: string; name: string }[]; errors: { file: string; error: string }[] };

const ACCEPT = ".gdtf,.qxf,.json";
const okType = (n: string) => /\.(gdtf|qxf|json)$/i.test(n);

function upload(files: File[], onProgress: (p: number) => void): Promise<Result> {
  return new Promise((resolve, reject) => {
    const fd = new FormData();
    files.forEach((f) => fd.append("file", f, f.name));
    const x = new XMLHttpRequest();
    x.open("POST", "/api/library/import");
    x.upload.onprogress = (e) => e.lengthComputable && onProgress(e.loaded / e.total);
    x.onload = () => {
      if (x.status >= 200 && x.status < 300) {
        try { resolve(JSON.parse(x.responseText) as Result); } catch { reject(new Error(t("Bad server response"))); }
      } else reject(new Error(x.responseText || `HTTP ${x.status}`));
    };
    x.onerror = () => reject(new Error(t("Upload failed — is the server running?")));
    x.send(fd);
  });
}

const size = (n: number) => (n > 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`);

/** Import fixture definitions: GDTF (MagicQ / MA3 / manufacturers), QLC+ .qxf, Open Fixture Library .json. */
export function ImportDialog(props: { onClose: () => void }) {
  const [files, setFiles] = useState<File[]>([]);
  const [drag, setDrag] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState(0);
  const [result, setResult] = useState<Result | null>(null);
  const input = useRef<HTMLInputElement>(null);

  const add = (list: FileList | File[] | null) => {
    const picked = Array.from(list ?? []);
    const bad = picked.filter((f) => !okType(f.name));
    if (bad.length) toast(t("Unsupported: {names}", { names: bad.map((b) => b.name).join(", ") }), "err");
    setFiles((cur) => [...cur, ...picked.filter((f) => okType(f.name) && !cur.some((c) => c.name === f.name && c.size === f.size))]);
    setResult(null);
  };
  const run = async () => {
    setBusy(true);
    setProgress(0);
    try {
      const r = await upload(files, setProgress);
      setResult(r);
      if (r.imported.length) toast(tp(r.imported.length, "Imported {n} fixture type", "Imported {n} fixture types"), "ok");
      if (r.errors.length) toast(tp(r.errors.length, "{n} file failed", "{n} files failed"), "err");
      setFiles([]);
    } catch (e) {
      toast((e as Error).message, "err");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={t("Import fixture library")} width={620} onClose={props.onClose}
      footer={<>
        <button class="btn" onClick={props.onClose}>{result ? t("Done") : t("Cancel")}</button>
        <button class="btn primary" disabled={!files.length || busy} onClick={run}>{busy ? t("Uploading {pct}%…", { pct: Math.round(progress * 100) }) : (files.length ? tp(files.length, "Import {n} file", "Import {n} files") : t("Import files"))}</button>
      </>}>
      <div
        class="panel"
        role="button"
        tabIndex={0}
        aria-label={t("Drop fixture files here or press Enter to choose files")}
        style={{ padding: 28, textAlign: "center", borderStyle: "dashed", borderColor: drag ? "var(--accent)" : "var(--line-strong)", background: drag ? "var(--accent-dim)" : undefined, cursor: "pointer" }}
        onClick={() => input.current?.click()}
        onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && input.current?.click()}
        onDragOver={(e) => { e.preventDefault(); setDrag(true); }}
        onDragLeave={() => setDrag(false)}
        onDrop={(e) => { e.preventDefault(); setDrag(false); add(e.dataTransfer?.files ?? null); }}>
        <div style={{ fontSize: "var(--fs-lg)", fontWeight: 700 }}>{t("Drop files here or click to browse")}</div>
        <div class="dim" style={{ marginTop: 6 }}>
          {t(".gdtf (GDTF — MagicQ, grandMA3, manufacturers; includes 3D model and gobos) · .qxf (QLC+) · .json (Open Fixture Library)")}
        </div>
        <input ref={input} type="file" accept={ACCEPT} multiple hidden onChange={(e) => { add((e.target as HTMLInputElement).files); (e.target as HTMLInputElement).value = ""; }} />
      </div>

      {files.length > 0 && (
        <div class="panel scroll" style={{ maxHeight: 160 }}>
          {files.map((f, i) => (
            <div key={f.name + i} class="vrow">
              <span class="chip">{f.name.split(".").pop()?.toLowerCase()}</span>
              <span class="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{f.name}</span>
              <span class="faint mono">{size(f.size)}</span>
              <button class="btn ghost icon sm" aria-label={t("Remove {name}", { name: f.name })} disabled={busy} onClick={() => setFiles(files.filter((_, j) => j !== i))}>✕</button>
            </div>
          ))}
        </div>
      )}

      {busy && <div role="progressbar" aria-valuenow={Math.round(progress * 100)} style={{ height: 6, background: "var(--bg-3)", borderRadius: 3, overflow: "hidden" }}><div style={{ width: `${progress * 100}%`, height: "100%", background: "var(--accent)" }} /></div>}

      {result && (
        <div class="col" aria-live="polite">
          {result.imported.map((i) => <div key={i.id} class="row"><span class="chip ok">{t("imported")}</span><span>{i.name}</span></div>)}
          {result.errors.map((e, k) => <div key={k} class="row" style={{ alignItems: "flex-start" }}><span class="chip err">{t("failed")}</span><span><b>{e.file}</b> <span class="dim">— {tErr(e.error)}</span></span></div>)}
        </div>
      )}
    </Modal>
  );
}
