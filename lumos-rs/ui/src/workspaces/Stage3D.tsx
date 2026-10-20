import { effect } from "@preact/signals";
import { useEffect, useRef } from "preact/hooks";
import "../styles/stage.css";
import { fire, library, selection, send, show, toast } from "../store";
import { releaseDmx, requestDmx } from "../lib/dmxsub";
import { StageScene } from "../stage/scene";
import { LeftPanel, RightPanel, Toolbar, deleteObject, duplicateObject, leftOpen, rightOpen } from "../stage/panels";
import {
  aimTool, draftVersion, editFixture, editMode, editObject, effFixtures, effStage, gizmoMode, hiddenObjs, selObject,
} from "../stage/state";

export function Stage3D() {
  const host = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    requestDmx("stage", null);
    const scene = new StageScene(host.current!, {
      onPick(hit, additive) {
        if (hit?.fixture !== undefined) {
          selObject.value = null;
          const id = hit.fixture;
          selection.value = additive ? (selection.value.includes(id) ? selection.value.filter((x) => x !== id) : [...selection.value, id]) : [id];
        } else if (hit?.object !== undefined) {
          selObject.value = hit.object;
        } else if (!additive) {
          selObject.value = null;
          selection.value = [];
        }
      },
      onTransform(t, pos, rot, scale, final) {
        const delay = final ? 0 : 300;
        if (t.type === "fixture") {
          editFixture(t.id, (f) => { f.pos = pos; f.rot = rot; }, delay);
        } else {
          editObject(t.id, (o) => {
            o.pos = pos; o.rot = rot;
            if (final && gizmoMode.value === "scale" && scale.some((s) => Math.abs(s - 1) > 1e-4)) {
              o.size = [o.size[0] * scale[0], o.size[1] * scale[1], o.size[2] * scale[2]].map((v) => Math.round(Math.max(0.02, v) * 1000) / 1000) as [number, number, number];
            }
          }, delay);
        }
      },
      onAim(results) {
        if (!results.length) { toast("Select at least one moving head to aim", "info"); return; }
        for (const r of results) fire({ type: "prog_set", fixtures: [r.id], attrs: { pan: r.pan, tilt: r.tilt } });
        toast(`Aimed ${results.length} fixture${results.length > 1 ? "s" : ""}`, "ok", 1500);
      },
    });
    (window as unknown as { __stage?: StageScene }).__stage = scene;

    const stop = effect(() => {
      void draftVersion.value; void show.value; void library.value;
      const st = effStage();
      if (st) scene.setStage(st, hiddenObjs.value);
      scene.setFixtures(effFixtures(), library.value);
      scene.setSelection(selection.value, selObject.value);
      scene.setEditing(editMode.value);
      scene.setAim(aimTool.value);
    });

    const key = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.matches?.("input,textarea,select")) return;
      if (e.ctrlKey && e.key.toLowerCase() === "d" && selObject.value !== null) { e.preventDefault(); duplicateObject(selObject.value); return; }
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      switch (e.key.toLowerCase()) {
        case "w": gizmoMode.value = "translate"; aimTool.value = false; break;
        case "e": gizmoMode.value = "rotate"; aimTool.value = false; break;
        case "r": gizmoMode.value = "scale"; aimTool.value = false; break;
        case "a": if (editMode.value) aimTool.value = !aimTool.value; break;
        case "delete": case "backspace": if (selObject.value !== null) deleteObject(selObject.value); break;
        case "escape": aimTool.value = false; selObject.value = null; break;
      }
    };
    window.addEventListener("keydown", key);

    return () => {
      window.removeEventListener("keydown", key);
      stop();
      scene.dispose();
      releaseDmx("stage");
      delete (window as unknown as { __stage?: StageScene }).__stage;
    };
  }, []);

  const edit = editMode.value;
  const fullscreen = () => {
    const el = root.current;
    if (!el) return;
    if (document.fullscreenElement) document.exitFullscreen();
    else el.requestFullscreen?.().catch(() => undefined);
  };
  void send;
  return (
    <div class={`st-root ${edit ? "" : "viewonly"}`} ref={root}>
      {edit && leftOpen.value && <LeftPanel />}
      <div class="st-view">
        <div class="st-canvas" ref={host} />
        <Toolbar onFullscreen={fullscreen} />
        {aimTool.value && <div class="st-banner">AIM — click the stage to point the selected moving heads · Esc to cancel</div>}
        {!show.value && <div class="st-banner">Waiting for the engine…</div>}
      </div>
      {edit && rightOpen.value && <RightPanel />}
    </div>
  );
}
