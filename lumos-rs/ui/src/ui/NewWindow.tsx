import { useEffect, useState } from "preact/hooks";
import { workspaces } from "../workspaces";
import { t } from "../i18n";
import { Modal } from "./Modal";
import { ICONS, Icon } from "./Icon";
import { hasSecondScreen, isDesktopApp, openWindow } from "../lib/windows";

/** Grid of big workspace tiles. */
export function WorkspaceGrid({ value, onPick }: { value?: string; onPick: (id: string) => void }) {
  return (
    <div class="ws-grid" role="listbox" aria-label={t("Workspaces")}>
      {workspaces.map((w) => (
        <button key={w.id} role="option" aria-selected={w.id === value} class={`ws-tile ${w.id === value ? "active" : ""}`} onClick={() => onPick(w.id)}>
          <Icon d={ICONS[w.id] ?? ICONS.patch} size={30} />
          <span>{w.label}</span>
        </button>
      ))}
    </div>
  );
}

/** Sheet to choose what a new window shows and where. */
export function NewWindowSheet({ onClose }: { onClose: () => void }) {
  const [ws, setWs] = useState("stage");
  const [monitor, setMonitor] = useState(hasSecondScreen());
  const [full, setFull] = useState(false);
  const go = async () => {
    const ok = await openWindow({ ws, monitor, fullscreen: full });
    if (ok) onClose();
  };
  return (
    <Modal
      title={t("New window")}
      onClose={onClose}
      width={620}
      footer={
        <>
          <button class="btn lg" onClick={onClose}>{t("Cancel")}</button>
          <button class="btn lg primary" onClick={() => void go()}><Icon d={ICONS.window} size={20} />{t("Open window")}</button>
        </>
      }
    >
      <div class="dim">{t("Choose what the new window shows. All windows share the same show and engine.")}</div>
      <WorkspaceGrid value={ws} onPick={setWs} />
      <div class="row wrap" style={{ gap: 12 }}>
        <label class="switch-row"><input type="checkbox" checked={monitor} onChange={(e) => setMonitor((e.target as HTMLInputElement).checked)} /> <span>{t("On another monitor")}</span></label>
        <label class="switch-row"><input type="checkbox" checked={full} onChange={(e) => setFull((e.target as HTMLInputElement).checked)} /> <span>{t("Fullscreen")}</span></label>
      </div>
      {!isDesktopApp && monitor && !hasSecondScreen() && <div class="faint">{t("Only one screen detected; the window will open on this screen.")}</div>}
    </Modal>
  );
}

/** Top-bar button. Also opens when the desktop menu fires `lumos:new-window`. */
export function NewWindowButton({ compact = false }: { compact?: boolean }) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const h = () => setOpen(true);
    window.addEventListener("lumos:new-window", h);
    return () => window.removeEventListener("lumos:new-window", h);
  }, []);
  return (
    <>
      <button class="btn lg" title={t("Open a new window (for a second monitor)")} aria-label={t("New window")} onClick={() => setOpen(true)}>
        <Icon d={ICONS.window} size={20} />
        {!compact && <span class="hide-narrow">{t("New window")}</span>}
      </button>
      {open && <NewWindowSheet onClose={() => setOpen(false)} />}
    </>
  );
}
