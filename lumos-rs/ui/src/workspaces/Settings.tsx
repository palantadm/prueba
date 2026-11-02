import { signal } from "@preact/signals";
import { UniversesPage } from "./settings/Universes";
import { MidiPage } from "./settings/Midi";
import { BindingsPage } from "./settings/Bindings";
import { ArtNetInPage, OscPage } from "./settings/Inputs";
import { GeneralPage, NetworkPage, TimecodePage } from "./settings/Misc";
import { MscPage } from "./settings/Msc";
import { AiPage } from "./settings/Ai";
import { AudioPage } from "./settings/Audio";
import { SchedulePage } from "./settings/Schedule";
import { RdmPage } from "./settings/Rdm";
import { ProgramsPage } from "./settings/Programs";
import { TrackingPage } from "./settings/Tracking";
import { Icon } from "../ui/Icon";
import { lang, setLang, t } from "../i18n";

const cats = [
  { id: "universes", icon: "M3 12h4l3-8 4 16 3-8h4", label: "Universes & outputs", c: UniversesPage },
  { id: "audio", icon: "M4 10v4M8 7v10M12 4v16M16 8v8M20 11v2", label: "Audio", c: AudioPage },
  { id: "midi", icon: "M4 7h16v10H4zM8 7v6M12 7v6M16 7v6", label: "MIDI", c: MidiPage },
  { id: "controls", icon: "M6 3v18M12 3v18M18 3v18M4 8h4M10 15h4M16 6h4", label: "Controls", c: BindingsPage },
  { id: "osc", icon: "M4 12c2-6 4-6 6 0s4 6 6 0 3-4 4-2", label: "OSC", c: OscPage },
  { id: "artnet", icon: "M4 12h12M12 6l6 6-6 6", label: "Art-Net input", c: ArtNetInPage },
  { id: "timecode", icon: "M12 7v5l3 2M12 3a9 9 0 100 18 9 9 0 000-18z", label: "Timecode", c: TimecodePage },
  { id: "schedule", icon: "M7 3v3M17 3v3M4 8h16M5 5h14v15H5zM9 13h2v2H9z", label: "Schedule", c: SchedulePage },
  { id: "tracking", icon: "M12 12m-2 0a2 2 0 104 0 2 2 0 10-4 0M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2 2M16 16l2 2", label: "Person tracking", c: TrackingPage },
  { id: "programs", icon: "M5 5h6v6H5zM13 5h6v6h-6zM5 13h6v6H5zM16 13v6M13 16h6", label: "External programs", c: ProgramsPage },
  { id: "rdm", icon: "M4 6h16M4 12h10M4 18h16M18 10l3 2-3 2", label: "RDM", c: RdmPage },
  { id: "msc", icon: "M5 4h14v16H5zM9 9h6M9 13h6", label: "Show Control (MSC)", c: MscPage },
  { id: "network", icon: "M3 12a9 9 0 0018 0 9 9 0 00-18 0zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18", label: "Network", c: NetworkPage },
  { id: "ai", icon: "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM18 16l.8 2.2L21 19l-2.2.8L18 22l-.8-2.2L15 19l2.2-.8z", label: "Assistant", c: AiPage },
  { id: "general", icon: "M12 15a3 3 0 100-6 3 3 0 000 6zM4 12h2M18 12h2M12 4v2M12 18v2M6.3 6.3l1.4 1.4M16.3 16.3l1.4 1.4M17.7 6.3l-1.4 1.4M7.7 16.3l-1.4 1.4", label: "General", c: GeneralPage },
];
const cat = signal(localStorage.getItem("settings.cat") ?? "universes");

export function setSettingsCat(id: string) {
  cat.value = id;
  localStorage.setItem("settings.cat", id);
}

export function Settings() {
  const cur = cats.find((c) => c.id === cat.value) ?? cats[0];
  const C = cur.c;
  return (
    <div class="set-wrap">
      <nav class="set-rail" aria-label={t("Settings categories")}>
        {cats.map((c) => (
          <button key={c.id} class={`side-item ${c.id === cur.id ? "active" : ""}`} aria-current={c.id === cur.id} title={t(c.label)}
            onClick={() => setSettingsCat(c.id)}>
            <span class="ico"><Icon d={c.icon} size={22} /></span><span class="lbl">{t(c.label)}</span>
          </button>
        ))}
        <div class="grow-sp" />
        <div class="set-lang" role="radiogroup" aria-label={t("Language")}>
          <div class="cap">{t("Language")}</div>
          <div class="opts">
            <button role="radio" aria-checked={lang.value === "es"} class={lang.value === "es" ? "on" : ""} onClick={() => setLang("es")}><span class="full">Español</span><span class="short">ES</span></button>
            <button role="radio" aria-checked={lang.value === "en"} class={lang.value === "en" ? "on" : ""} onClick={() => setLang("en")}><span class="full">English</span><span class="short">EN</span></button>
          </div>
        </div>
      </nav>
      <div class="set-main"><div class="set-card"><C /></div></div>
    </div>
  );
}
