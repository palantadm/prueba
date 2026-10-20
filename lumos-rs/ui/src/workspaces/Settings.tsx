import { signal } from "@preact/signals";
import { UniversesPage } from "./settings/Universes";
import { MidiPage } from "./settings/Midi";
import { BindingsPage } from "./settings/Bindings";
import { ArtNetInPage, OscPage } from "./settings/Inputs";
import { GeneralPage, NetworkPage, TimecodePage } from "./settings/Misc";

const cats = [
  { id: "universes", icon: "⎍", label: "Universes & outputs", c: UniversesPage },
  { id: "midi", icon: "🎹", label: "MIDI", c: MidiPage },
  { id: "controls", icon: "🎛", label: "Controls", c: BindingsPage },
  { id: "osc", icon: "≋", label: "OSC", c: OscPage },
  { id: "artnet", icon: "⇄", label: "Art-Net input", c: ArtNetInPage },
  { id: "timecode", icon: "⏱", label: "Timecode", c: TimecodePage },
  { id: "network", icon: "🌐", label: "Network", c: NetworkPage },
  { id: "general", icon: "⚙", label: "General", c: GeneralPage },
];
const cat = signal(localStorage.getItem("settings.cat") ?? "universes");

export function Settings() {
  const cur = cats.find((c) => c.id === cat.value) ?? cats[0];
  const C = cur.c;
  return (
    <div class="row" style={{ height: "100%", alignItems: "stretch", gap: 0 }}>
      <nav class="side" aria-label="Settings categories">
        {cats.map((c) => (
          <button key={c.id} class={`side-item ${c.id === cur.id ? "active" : ""}`} aria-current={c.id === cur.id} title={c.label}
            onClick={() => { cat.value = c.id; localStorage.setItem("settings.cat", c.id); }}>
            <span aria-hidden="true">{c.icon}</span><span class="lbl">{c.label}</span>
          </button>
        ))}
      </nav>
      <div class="grow" style={{ minWidth: 0 }}><C /></div>
    </div>
  );
}
