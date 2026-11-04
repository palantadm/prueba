import { useEffect, useRef, useState } from "preact/hooks";
import { Markdown } from "../ai/markdown";
import { t } from "../i18n";
import { toast } from "../store";
import { Modal } from "../ui";
import { closeHelp, helpPage } from "./state";

/* The user manual. Everything comes from the Lumos engine (embedded in the app), so it works offline and without any AI. */

type Topic = { id: string; title: string };
type Cmd = { type: string; what: string; example: string; console: string | null };
type Group = { name: string; commands: Cmd[] };
type Index = { topics: Topic[]; groups: Group[] };
type Hit = { topics: { id: string; title: string; snippet: string }[]; commands: { type: string; what: string; console: string | null }[] };

export const SHORTCUTS: [string, string][] = [
  ["1 … 9 · 0 · G", "Switch workspace"],
  ["Ctrl/⌘ + B", "Toggle blackout"],
  ["Ctrl/⌘ + S", "Save show"],
  ["Ctrl/⌘ + Z", "Undo"],
  ["Ctrl/⌘ + Y  ·  Ctrl/⌘ + Shift + Z", "Redo"],
  ["Ctrl/⌘ + K", "Ask the assistant"],
  ["/", "Command line"],
  ["?", "Help"],
  ["Esc", "Close dialogs"],
];

const copy = (text: string) => { void navigator.clipboard?.writeText(text).then(() => toast(t("Copied"), "ok", 1500), () => toast(t("Could not copy"), "err")); };

function CommandCard({ c }: { c: Cmd }) {
  return (
    <div class="hc-card">
      <div class="row" style={{ gap: 8 }}><code class="hc-type">{c.type}</code>{c.console && <span class="chip">{t("console")}</span>}</div>
      <div>{c.what}</div>
      {c.console && <div class="row" style={{ gap: 8 }}><code class="hc-code grow">{c.console}</code><button class="btn" onClick={() => copy(c.console!)}>{t("Copy")}</button></div>}
      <details>
        <summary class="dim">{t("Technical form (JSON)")}</summary>
        <div class="row" style={{ gap: 8, alignItems: "flex-start" }}><pre class="hc-code grow">{JSON.stringify(JSON.parse(c.example), null, 1)}</pre><button class="btn" onClick={() => copy(c.example)}>{t("Copy")}</button></div>
      </details>
    </div>
  );
}

function Topic({ id, nav }: { id: string; nav: (p: string) => void }) {
  const [md, setMd] = useState<{ title: string; markdown: string } | null>(null);
  const [err, setErr] = useState("");
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    setMd(null);
    setErr("");
    fetch(`/api/help/topic/${encodeURIComponent(id)}`).then(async (r) => (r.ok ? setMd(await r.json()) : setErr((await r.json().catch(() => ({}))).error ?? String(r.status)))).catch((e) => setErr(String(e)));
    box.current?.scrollTo?.(0, 0);
  }, [id]);
  if (err) return <div class="dim">{err}</div>;
  if (!md) return <div class="dim">{t("Loading…")}</div>;
  // «tema `salidas`» → a link to that page
  const linked = md.markdown;
  return (
    <div ref={box} class="hc-md" onClick={(e) => {
      const el = (e.target as HTMLElement).closest("code");
      const id2 = el?.textContent ?? "";
      if (el && /^[a-z]+(-[a-z]+)*$/.test(id2) && el.parentElement && /tema/.test(el.parentElement.textContent ?? "")) nav(`topic:${id2}`);
    }}>
      <h2>{md.title}</h2>
      <Markdown text={linked} />
    </div>
  );
}

export function HelpCenter() {
  const page = helpPage.value;
  const [index, setIndex] = useState<Index | null>(null);
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<Hit | null>(null);
  const [fail, setFail] = useState("");
  const nav = (p: string) => { helpPage.value = p; };
  useEffect(() => {
    if (!page || index) return;
    fetch("/api/help").then((r) => r.json()).then(setIndex).catch((e) => setFail(String(e)));
  }, [!!page]);
  useEffect(() => {
    if (page?.startsWith("search:")) setQ(page.slice(7));
  }, [page]);
  useEffect(() => {
    const text = q.trim();
    if (text.length < 2) { setHits(null); return; }
    const h = setTimeout(() => fetch(`/api/help/search?q=${encodeURIComponent(text)}`).then((r) => r.json()).then(setHits).catch(() => setHits(null)), 150);
    return () => clearTimeout(h);
  }, [q]);
  if (!page) return null;
  const cur = page.startsWith("topic:") ? page.slice(6) : page;
  const searching = q.trim().length >= 2;
  return (
    <Modal title={t("Help")} onClose={closeHelp} width={1100}>
      <div class="hc">
        <nav class="hc-nav" aria-label={t("Help topics")}>
          <input class="input" type="search" value={q} placeholder={t("Search the manual…")} aria-label={t("Search the manual…")} onInput={(e) => setQ((e.target as HTMLInputElement).value)} />
          <button class={`side-item ${cur === "shortcuts" && !searching ? "active" : ""}`} onClick={() => { setQ(""); nav("shortcuts"); }}>{t("Keyboard shortcuts")}</button>
          {fail && <div class="dim">{fail}</div>}
          {index?.topics.map((x) => <button key={x.id} class={`side-item ${cur === x.id && !searching ? "active" : ""}`} onClick={() => { setQ(""); nav(`topic:${x.id}`); }}>{x.title}</button>)}
          <button class={`side-item ${cur === "commands" && !searching ? "active" : ""}`} onClick={() => { setQ(""); nav("commands"); }}>{t("Command reference")}</button>
        </nav>
        <div class="hc-body">
          {searching ? (
            <div class="col" style={{ gap: 12 }}>
              {hits && hits.topics.length === 0 && hits.commands.length === 0 && <div class="dim">{t("Nothing found. Try other words.")}</div>}
              {hits?.topics.map((x) => (
                <button key={x.id} class="hc-hit" onClick={() => { setQ(""); nav(`topic:${x.id}`); }}><b>{x.title}</b><span class="dim">{x.snippet}</span></button>
              ))}
              {!!hits?.commands.length && <div class="cap">{t("Commands")}</div>}
              {hits?.commands.map((c) => <button key={c.type} class="hc-hit" onClick={() => { setQ(""); nav("commands"); }}><b class="mono">{c.console ?? c.type}</b><span class="dim">{c.what}</span></button>)}
            </div>
          ) : cur === "shortcuts" ? (
            <div><h2>{t("Keyboard shortcuts")}</h2><table class="tbl"><tbody>{SHORTCUTS.map(([k, d]) => <tr key={k}><td><kbd class="kbd">{k}</kbd></td><td>{t(d)}</td></tr>)}</tbody></table></div>
          ) : cur === "commands" ? (
            <div class="col" style={{ gap: 14 }}>
              <h2>{t("Command reference")}</h2>
              <div class="dim">{t("Every command Lumos understands: what it does, the console form when it has one, and the technical form used by controls, macros and the API.")}</div>
              {index?.groups.map((g) => (
                <details key={g.name} open={false}>
                  <summary><b>{g.name}</b> <span class="dim">({g.commands.length})</span></summary>
                  <div class="col" style={{ gap: 10, marginTop: 8 }}>{g.commands.map((c) => <CommandCard key={c.type} c={c} />)}</div>
                </details>
              ))}
            </div>
          ) : (
            <Topic id={cur} nav={nav} />
          )}
        </div>
      </div>
    </Modal>
  );
}
