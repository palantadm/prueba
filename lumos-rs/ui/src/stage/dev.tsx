// Throw-away harness: fake show + animated DMX to exercise the 3D workspace without a server.
import { render } from "preact";
import "../styles/base.css";
import { connect, dmx, library, show } from "../store";
import { Toasts } from "../ui";
import { Stage3D } from "../workspaces/Stage3D";
import type { FixtureProfile, Fixture, Show } from "../types";

const ch = (attr: string, offset: number, extra: object = {}) => ({ attr, offset, fine: null, default: 0, invert: false, capabilities: [], ...extra });
const wheel = [
  { min: 0, max: 15, label: "White", color: "#ffffff" }, { min: 16, max: 31, label: "Red", color: "#ff2020" },
  { min: 32, max: 47, label: "Orange", color: "#ff8a20" }, { min: 48, max: 63, label: "Blue", color: "#2050ff" },
  { min: 64, max: 79, label: "Green", color: "#20d040" }, { min: 80, max: 95, label: "Magenta", color: "#e020c0" },
];
const profiles: FixtureProfile[] = [
  { id: "t/par", manufacturer: "Generic", model: "RGB Par", category: "par", beam_angle: [30, 30], pan_range: 0, tilt_range: 0, power_w: 100,
    modes: [{ name: "4ch", channels: [ch("dimmer", 0), ch("red", 1), ch("green", 2), ch("blue", 3)] }] },
  { id: "t/mh", manufacturer: "Generic", model: "Spot 9ch", category: "spot", beam_angle: [8, 30], pan_range: 540, tilt_range: 270, power_w: 300,
    modes: [{ name: "9ch", channels: [ch("pan", 0, { fine: 1 }), ch("tilt", 2, { fine: 3 }), ch("colorwheel", 4, { capabilities: wheel }), ch("gobo", 5), ch("shutter", 6, { capabilities: [{ min: 0, max: 9, label: "Closed", color: null }, { min: 10, max: 99, label: "Open", color: null }, { min: 100, max: 255, label: "Strobe", color: null }] }), ch("dimmer", 7), ch("zoom", 8)] }] },
  { id: "t/bar", manufacturer: "Generic", model: "LED Bar", category: "bar", beam_angle: [60, 60], pan_range: 0, tilt_range: 0, power_w: 80,
    modes: [{ name: "3ch", channels: [ch("red", 0), ch("green", 1), ch("blue", 2)] }] },
  { id: "t/strobe", manufacturer: "Generic", model: "Strobe", category: "strobe", beam_angle: [90, 90], pan_range: 0, tilt_range: 0, power_w: 500,
    modes: [{ name: "1ch", channels: [ch("dimmer", 0)] }] },
];
const fixtures: Fixture[] = [];
let addr = 1, id = 1;
const place = (profile: string, mode: string, pos: [number, number, number], n: number, name: string, footprint: number, uni = 1) => {
  for (let i = 0; i < n; i++) { fixtures.push({ id: id++, name: `${name} ${i + 1}`, profile, mode, universe: uni, address: addr, pos: [pos[0] + i * 1.6 - ((n - 1) * 0.8), pos[1], pos[2]], rot: [0, 0, 0] }); addr += footprint; }
};
place("t/par", "4ch", [0, 5.6, 2], 6, "Par", 4);
place("t/mh", "9ch", [0, 5.6, -2], 6, "Head", 9);
place("t/bar", "3ch", [0, 0.1, 3], 3, "Bar", 3);
place("t/strobe", "1ch", [0, 5.6, 0], 1, "Strobe", 1);
fixtures[fixtures.length - 1].pos = [3, 5.6, 0];
for (let i = 0; i < 3; i++) { fixtures[fixtures.length - 2 - i].pos[0] = -3 + i * 3; fixtures[fixtures.length - 2 - i].pos[2] = 3; }

if (new URLSearchParams(location.search).has("many")) {
  for (let i = 0; i < 150; i++) {
    const row = Math.floor(i / 30), col = i % 30;
    fixtures.push({ id: id++, name: `Many ${i}`, profile: i % 3 === 0 ? "t/mh" : "t/par", mode: i % 3 === 0 ? "9ch" : "4ch", universe: 1, address: 1 + (i % 50) * 9, pos: [-5.5 + col * 0.38, 5.4 - (row % 2) * 0.1, -2.5 + row * 1.2], rot: [0, 0, 0] });
  }
}
const s: Show = {
  name: "Harness", universes: [{ id: 1, name: "U1", outputs: [] }], fixtures, groups: [], palettes: [], cuelists: [], executors: [], effects: [], bindings: [], profiles: [],
  settings: undefined as never,
  stage: { size: [12, 8, 6], objects: [
    { id: 1, kind: "truss", name: "Front truss", pos: [0, 5.85, 2], rot: [0, 0, 0], size: [11, 0.3, 0.3], color: "#9aa0a8", media: null, screen_type: null, brightness: 1 },
    { id: 2, kind: "truss", name: "Back truss", pos: [0, 5.85, -2], rot: [0, 0, 0], size: [11, 0.3, 0.3], color: "#9aa0a8", media: null, screen_type: null, brightness: 1 },
    { id: 3, kind: "platform", name: "Riser", pos: [0, 0.2, -1], rot: [0, 0, 0], size: [4, 0.4, 2], color: "#2a2d33", media: null, screen_type: null, brightness: 1 },
    { id: 4, kind: "curtain", name: "Back curtain", pos: [0, 2.5, -3.7], rot: [0, 0, 0], size: [10, 5, 0.2], color: "#5a0d14", media: null, screen_type: null, brightness: 1 },
    { id: 5, kind: "screen", name: "LED wall", pos: [-3.6, 2.4, -3.3], rot: [0, 0, 0], size: [3.2, 1.8, 0.1], color: "#111", media: { kind: "none", src: "", looped: true }, screen_type: "led", brightness: 1 },
    { id: 6, kind: "screen", name: "Projection", pos: [3.6, 2.4, -3.3], rot: [0, 0, 0], size: [3.2, 1.8, 0.05], color: "#e8e8e8", media: { kind: "color", src: "#3060ff", looped: true }, screen_type: "projection", brightness: 1 },
    { id: 7, kind: "column", name: "Column", pos: [-5, 1.5, 0], rot: [0, 0, 0], size: [0.4, 3, 0.4], color: "#3a3d44", media: null, screen_type: null, brightness: 1 },
    { id: 8, kind: "tree", name: "Tree", pos: [5, 1.25, 0.5], rot: [0, 0, 0], size: [1.6, 2.5, 1.6], color: "#2f6b34", media: null, screen_type: null, brightness: 1 },
  ] },
};
library.value = profiles;
show.value = s;

const f = new Uint8Array(512);
dmx.set(1, f);
const t0 = performance.now();
setInterval(() => {
  const t = (performance.now() - t0) / 1000;
  for (const fx of fixtures) {
    const a = fx.address - 1;
    if (fx.profile === "t/par") { f[a] = 255; f[a + 1] = 128 + 127 * Math.sin(t + fx.id); f[a + 2] = 40; f[a + 3] = 200 + 55 * Math.sin(t * 0.7 + fx.id); }
    if (fx.profile === "t/mh") {
      const p = 0.5 + 0.18 * Math.sin(t * 0.8 + fx.id); const tl = 0.5 + 0.12 * Math.cos(t * 0.6 + fx.id);
      f[a] = Math.floor(p * 255); f[a + 1] = 0; f[a + 2] = Math.floor(tl * 255); f[a + 3] = 0;
      f[a + 4] = 16 * (1 + (fx.id % 5)); f[a + 6] = 50; f[a + 7] = 255; f[a + 8] = 90;
    }
    if (fx.profile === "t/bar") { f[a] = 255; f[a + 1] = 60; f[a + 2] = 200; }
    if (fx.profile === "t/strobe") f[a] = 0;
  }
}, 25);
class FakeWS {
  readyState = 1; binaryType = ""; onopen: any; onmessage: any; onclose: any; onerror: any;
  static OPEN = 1;
  constructor() { setTimeout(() => this.onopen?.(), 0); }
  close() { /* */ }
  send(data: string) {
    const m = JSON.parse(data);
    const c = m.cmd;
    const cur = show.value!;
    if (c.type === "stage_save") show.value = { ...cur, stage: c.stage };
    if (c.type === "fixture_update") show.value = { ...cur, fixtures: cur.fixtures.map((f) => (f.id === c.fixture.id ? c.fixture : f)) };
    (window as any).__cmds = [...((window as any).__cmds ?? []), c];
    if (m.id) setTimeout(() => this.onmessage?.({ data: JSON.stringify({ t: "ack", id: m.id, ok: true, err: null }) }), 5);
  }
}
(window as any).WebSocket = FakeWS;
connect();
render(<><Stage3D /><Toasts /></>, document.getElementById("app")!);
