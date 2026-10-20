import { useState } from "preact/hooks";
import { rt, send, show } from "../../store";
import type { ExecKind, Executor } from "../../types";
import { Modal } from "../../ui";
import { ExecutorStrip } from "../../widgets";
import { t } from "../../i18n";

const SLOTS = 10;
const TYPE_LABEL = { cuelist: "Cuelist", group: "Group", effect: "Effect", master: "Grand master" } as const;

function kindLabel(k: ExecKind): string {
  const s = show.value;
  switch (k.type) {
    case "cuelist": return s?.cuelists.find((c) => c.id === k.cuelist)?.name ?? `CL ${k.cuelist}`;
    case "group": return s?.groups.find((g) => g.id === k.group)?.name ?? `Grp ${k.group}`;
    case "effect": return s?.effects.find((e) => e.id === k.effect)?.name ?? `FX ${k.effect}`;
    case "master": return t("Grand Master");
    default: return "";
  }
}

export function Executors() {
  const execs = show.value?.executors ?? [];
  const maxPage = Math.max(4, ...execs.map((e) => e.page));
  const [page, setPage] = useState(0);
  const [assign, setAssign] = useState<{ slot: number; exec?: Executor } | null>(null);
  const running = new Set(rt.value?.effects_running ?? []);
  return (
    <div class="panel lv-execs">
      <div class="row" style={{ marginBottom: 6 }}>
        <span class="label">{t("Executors")}</span>
        <div class="tabs" role="tablist" aria-label={t("Executor page")}>
          {Array.from({ length: maxPage + 1 }, (_, i) => (
            <button key={i} role="tab" aria-selected={i === page} class={`tab ${i === page ? "active" : ""}`} onClick={() => setPage(i)}>{t("Page {n}", { n: i + 1 })}</button>
          ))}
        </div>
      </div>
      <div class="lv-exrow">
        {Array.from({ length: SLOTS }, (_, slot) => {
          const e = execs.find((x) => x.page === page && x.slot === slot);
          const r = e ? rt.value?.executors.find((x) => x.id === e.id) : undefined;
          const effOn = e?.kind.type === "effect" ? running.has(e.kind.effect) : false;
          return (
            <ExecutorStrip key={slot} slot={slot} exec={e} rt={r} effectOn={effOn} kindLabel={e ? kindLabel(e.kind) : ""}
              onAssign={() => setAssign({ slot, exec: e })}
              onGo={() => {
                if (!e) return;
                if (e.kind.type === "effect") send({ type: "effect_toggle", id: e.kind.effect, on: !effOn });
                else send({ type: "exec_go", id: e.id });
              }} />
          );
        })}
      </div>
      {assign && <AssignDialog page={page} slot={assign.slot} exec={assign.exec} onClose={() => setAssign(null)} />}
    </div>
  );
}

function AssignDialog({ page, slot, exec, onClose }: { page: number; slot: number; exec?: Executor; onClose: () => void }) {
  const s = show.value!;
  const k = exec?.kind;
  const [type, setType] = useState<ExecKind["type"]>(k && k.type !== "empty" ? k.type : "cuelist");
  const [cuelist, setCuelist] = useState(k?.type === "cuelist" ? k.cuelist : s.cuelists[0]?.id ?? 0);
  const [group, setGroup] = useState(k?.type === "group" ? k.group : s.groups[0]?.id ?? 0);
  const [effect, setEffect] = useState(k?.type === "effect" ? k.effect : s.effects[0]?.id ?? 0);
  const [label, setLabel] = useState(exec?.label ?? "");
  const kind = (): ExecKind =>
    type === "cuelist" ? { type, cuelist } : type === "group" ? { type, group } : type === "effect" ? { type, effect } : { type: "master", master: "grand" };
  const valid = type === "master" || (type === "cuelist" && cuelist) || (type === "group" && group) || (type === "effect" && effect);
  const save = () => {
    const id = exec?.id ?? Math.max(0, ...s.executors.map((e) => e.id)) + 1;
    send({ type: "exec_save", executor: { id, page, slot, label, kind: kind() } });
    onClose();
  };
  const sel = (v: number, set: (n: number) => void, opts: { id: number; name: string }[]) => (
    <select class="input" value={v} onChange={(e) => set(Number((e.target as HTMLSelectElement).value))}>
      {opts.length === 0 && <option value={0}>{t("— none available —")}</option>}
      {opts.map((o) => <option key={o.id} value={o.id}>{o.name}</option>)}
    </select>
  );
  return (
    <Modal title={t("Executor {p}.{s}", { p: page + 1, s: slot + 1 })} onClose={onClose} footer={<>
      {exec && <button class="btn danger" onClick={() => { send({ type: "exec_remove", id: exec.id }); onClose(); }}>{t("Clear")}</button>}
      <span class="sp" />
      <button class="btn" onClick={onClose}>{t("Cancel")}</button>
      <button class="btn primary" disabled={!valid} onClick={save}>{t("Assign")}</button>
    </>}>
      <div class="row wrap" role="tablist" aria-label={t("Executor type")}>
        {(["cuelist", "group", "effect", "master"] as const).map((k) => (
          <button key={k} role="tab" class={`btn ${type === k ? "sel" : ""}`} onClick={() => setType(k)}>{t(TYPE_LABEL[k])}</button>
        ))}
      </div>
      {type === "cuelist" && <div class="field"><label>{t("Cuelist")}</label>{sel(cuelist, setCuelist, s.cuelists)}</div>}
      {type === "group" && <div class="field"><label>{t("Group (dimmer master)")}</label>{sel(group, setGroup, s.groups)}</div>}
      {type === "effect" && <div class="field"><label>{t("Effect")}</label>{sel(effect, setEffect, s.effects)}</div>}
      <div class="field"><label>{t("Label")}</label><input class="input" value={label} placeholder={t("(auto)")} onInput={(e) => setLabel((e.target as HTMLInputElement).value)} /></div>
    </Modal>
  );
}
