import "../styles/auto.css";
import { signal } from "@preact/signals";
import { useEffect } from "preact/hooks";
import { rt, send, show } from "../store";
import type { Timeline as Tl } from "../types";
import { EmptyState, confirmDialog, promptDialog } from "../ui";
import { lsGet, lsSet } from "../lib/state";
import { t } from "../i18n";
import { TimelineEditor } from "./timeline/TimelineEditor";

const activeTl = signal<number | null>(lsGet("tl.active", null));
const select = (id: number | null) => { activeTl.value = id; lsSet("tl.active", id); };

export function Timeline() {
  const list = show.value?.timelines ?? [];
  const cur = list.find((x) => x.id === activeTl.value) ?? list[0];
  useEffect(() => { if (cur && activeTl.value !== cur.id) select(cur.id); }, [cur?.id]);

  const create = async () => {
    const name = await promptDialog(t("New timeline"), t("Name"), t("Timeline {n}", { n: list.length + 1 }));
    if (!name) return;
    const id = Math.max(0, ...list.map((x) => x.id)) + 1;
    const tl: Tl = { id, name, length: 60, events: [], looped: false, tc_start: null, audio: null };
    await send({ type: "timeline_save", timeline: tl }).then(() => select(id)).catch(() => {});
  };
  const rename = async () => {
    if (!cur) return;
    const name = await promptDialog(t("Rename timeline"), t("Name"), cur.name);
    if (name) void send({ type: "timeline_save", timeline: { ...cur, name } });
  };
  const del = async () => {
    if (!cur) return;
    if (!(await confirmDialog(t("Delete timeline"), t("Delete timeline “{name}” and its {n} event(s)?", { name: cur.name, n: cur.events.length }), { danger: true, ok: t("Delete") }))) return;
    await send({ type: "timeline_remove", id: cur.id }).catch(() => {});
    select(null);
  };

  return (
    <div class="tl-root">
      <div class="panel lv-col">
        <div class="panel-head"><span>{t("Timelines")}</span><span class="sp" />
          <button class="btn sm primary" style={{ minHeight: 36 }} onClick={create} aria-label={t("New timeline")}>{t("+ New")}</button></div>
        <div class="lv-scroll" role="listbox" aria-label={t("Timelines")}>
          {list.length === 0 && <div class="dim" style={{ padding: 12 }}>{t("No timelines yet.")}</div>}
          {list.map((x) => {
            const r = rt.value?.timelines.find((y) => y.id === x.id);
            return (
              <div key={x.id} role="option" aria-selected={x.id === cur?.id} class={`lv-cl ${x.id === cur?.id ? "sel" : ""}`} style={{ minHeight: 48 }} onClick={() => select(x.id)}>
                <span class={`dot ${r?.running ? "ok" : ""}`} />
                <span class="grow" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{x.name}</span>
                <span class="faint mono">#{x.id} · {x.events.length}</span>
              </div>
            );
          })}
        </div>
      </div>
      {cur ? <TimelineEditor key={cur.id} tl={cur} onRename={rename} onDelete={del} />
        : <EmptyState title={t("No timeline selected")} hint={t("A timeline fires command-line statements at fixed times — cues, scenes, macros — optionally locked to show timecode.")}>
          <button class="btn au-btn primary" onClick={create}>{t("+ New timeline")}</button></EmptyState>}
    </div>
  );
}
