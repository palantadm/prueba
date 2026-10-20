import "../styles/live.css";
import { selection, send } from "../store";
import { useDmxTick, useKey } from "../lib/hooks";
import { activeCuelist } from "../lib/state";
import { show, rt } from "../store";
import { releaseAttrs } from "../lib/program";
import { FixturePool } from "./live/FixturePool";
import { AttrPanels } from "./live/AttrPanels";
import { Pools } from "./live/Pools";
import { ProgBar } from "./live/ProgBar";

export function Live() {
  useDmxTick(10);
  useKey((e) => {
    if (e.key === "Escape") selection.value = [];
    else if ((e.ctrlKey || e.metaKey) && e.key === "Backspace") { e.preventDefault(); releaseAttrs(null, null); }
    else if (e.key === " ") {
      e.preventDefault();
      const id = activeCuelist.value ?? rt.value?.cuelists.find((c) => c.state === "running")?.id ?? show.value?.cuelists[0]?.id;
      if (id !== undefined) send({ type: "go", cuelist: id });
    }
  });
  return (
    <div class="lv-live">
      <div class="lv-live-main">
        <FixturePool />
        <AttrPanels />
        <Pools />
      </div>
      <ProgBar />
    </div>
  );
}
