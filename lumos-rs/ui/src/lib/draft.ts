// Editable draft of Show.settings with dirty tracking.
import { useEffect, useRef, useState } from "preact/hooks";
import { send, show, toast } from "../store";
import type { Settings } from "../types";

export function useSettingsDraft() {
  const server = show.value?.settings;
  const [draft, setDraft] = useState<Settings | null>(server ?? null);
  const base = useRef<string>("");
  const serverJson = server ? JSON.stringify(server) : "";
  const dirty = !!draft && JSON.stringify(draft) !== base.current;

  useEffect(() => {
    // adopt server changes when the user has no unsaved edits
    if (!server) return;
    if (!draft || JSON.stringify(draft) === base.current) setDraft(structuredClone(server));
    base.current = serverJson;
  }, [serverJson]);

  const patch = (f: (s: Settings) => void) => {
    if (!draft) return;
    const c = structuredClone(draft);
    f(c);
    setDraft(c);
  };
  const save = async () => {
    if (!draft) return;
    try { await send({ type: "settings_save", settings: draft }); toast("Settings saved", "ok", 1500); } catch { /* toasted */ }
  };
  const revert = () => server && setDraft(structuredClone(server));
  return { draft, patch, dirty, save, revert };
}
