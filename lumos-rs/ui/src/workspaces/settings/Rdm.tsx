import { useState } from "preact/hooks";
import type { RdmDevice } from "../../types";
import { fire, rt, show } from "../../store";
import { EmptyState } from "../../ui";
import { SettingsPage } from "./Common";
import { t } from "../../i18n";

/* RDM (ANSI E1.20) over Art-Net: find the fixtures on the DMX lines and read or change their address, name and mode. */

function Row({ d }: { d: RdmDevice }) {
  const [addr, setAddr] = useState<string | null>(null);
  const [label, setLabel] = useState<string | null>(null);
  const fx = d.fixture != null ? show.value?.fixtures.find((f) => f.id === d.fixture) : undefined;
  const commitAddr = () => {
    const n = Number(addr);
    if (addr !== null && Number.isInteger(n) && n >= 1 && n <= 512 && n !== d.address) fire({ type: "rdm_set_address", uid: d.uid, address: n });
    setAddr(null);
  };
  const commitLabel = () => {
    if (label !== null && label !== d.label) fire({ type: "rdm_set_label", uid: d.uid, label });
    setLabel(null);
  };
  return (
    <tr class={d.status === "lost" ? "off" : ""}>
      <td class="mono">{d.uid}<div class="muted">{d.node} · {d.port_address}</div></td>
      <td>{d.manufacturer || "—"}<div class="muted">{d.model || (d.status === "loading" ? t("Reading…") : "—")}</div></td>
      <td><input class="input" value={label ?? d.label} maxLength={32} onInput={(e) => setLabel((e.target as HTMLInputElement).value)} onBlur={commitLabel} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} aria-label={t("Name")} /></td>
      <td><input class="input" type="number" min={1} max={512} style={{ width: 72 }} value={addr ?? d.address} onInput={(e) => setAddr((e.target as HTMLInputElement).value)} onBlur={commitAddr} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} aria-label={t("Address")} /></td>
      <td>
        {d.personalities > 1 ? (
          <select class="input" value={d.personality} onChange={(e) => fire({ type: "rdm_set_personality", uid: d.uid, personality: Number((e.target as HTMLSelectElement).value) })} aria-label={t("Mode")}>
            {Array.from({ length: d.personalities }, (_, i) => <option key={i} value={i + 1}>{t("Mode {n}", { n: i + 1 })}</option>)}
          </select>
        ) : d.personalities === 1 ? t("Mode {n}", { n: 1 }) : "—"}
        <div class="muted">{d.footprint} {t("ch")}</div>
      </td>
      <td>{fx ? `#${fx.id} ${fx.name}` : <span class="muted">{t("Not in the patch")}</span>}</td>
      <td class="row" style={{ gap: 4 }}>
        <button class={`btn ${d.identify ? "sel" : ""}`} onClick={() => fire({ type: "rdm_identify", uid: d.uid, on: !d.identify })} title={t("Make the fixture flash so you can find it")}>{t("Identify")}</button>
        <button class="btn" onClick={() => fire({ type: "rdm_refresh", uid: d.uid })} title={t("Read it again")}>↻</button>
      </td>
      <td>{d.status === "lost" ? <span class="err">{t("No answer")}</span> : d.error ? <span class="err" title={d.error}>{d.error}</span> : null}</td>
    </tr>
  );
}

export function RdmPage() {
  const [flush, setFlush] = useState(false);
  const r = rt.value?.rdm;
  if (!r) return <EmptyState title={t("Waiting for the engine…")} />;
  return (
    <SettingsPage title="RDM" sub={t("Find the fixtures on the DMX lines and set their address, name and mode from here. Needs Art-Net nodes with RDM.")}>
      <div class="panel">
        <div class="panel-body row wrap" style={{ gap: 8, alignItems: "center" }}>
          <button class="btn primary" onClick={() => fire({ type: "rdm_scan", flush })}>{t("Scan")}</button>
          <label class="row" style={{ gap: 6 }}><input type="checkbox" checked={flush} onChange={(e) => setFlush((e.target as HTMLInputElement).checked)} />{t("Run discovery again on the nodes (slower)")}</label>
          <span class="grow" />
          {r.busy && <span class="muted">{t("Talking to the fixtures…")}</span>}
          <button class="btn" disabled={r.devices.length === 0} onClick={() => fire({ type: "rdm_clear" })}>{t("Clear")}</button>
        </div>
        {r.error && <div class="panel-body err">{r.error}</div>}
      </div>
      {r.devices.length === 0 ? (
        <EmptyState title={t("No RDM fixtures yet")} hint={t("Press Scan. The Art-Net node must have RDM on and the fixtures must support it; the output universes need an Art-Net output to that node.")} />
      ) : (
        <div class="panel">
          <div class="panel-body" style={{ overflowX: "auto" }}>
            <table class="tbl">
              <thead><tr><th>UID</th><th>{t("Fixture")}</th><th>{t("Name")}</th><th>{t("Address")}</th><th>{t("Mode")}</th><th>{t("Patch")}</th><th /><th /></tr></thead>
              <tbody>{r.devices.map((d) => <Row key={d.uid} d={d} />)}</tbody>
            </table>
          </div>
        </div>
      )}
    </SettingsPage>
  );
}
