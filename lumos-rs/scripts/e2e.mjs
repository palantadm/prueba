// End-to-end smoke test: boots the server binary on a free port and drives it over WebSocket/REST.
// Usage: node scripts/e2e.mjs [path-to-binary]   (Node ≥ 22: uses the built-in WebSocket client)
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import dgram from "node:dgram";

const bin = process.argv[2] ?? "target/debug/lumos";
const data = mkdtempSync(join(tmpdir(), "lumos-e2e-"));
const proc = spawn(bin, ["--port", "0", "--bind", "127.0.0.1", "--data", data], { stdio: ["ignore", "pipe", "inherit"] });
let failed = 0;
const ok = (c, m) => { if (!c) { failed++; console.log("  FAIL", m); } else console.log("  ok  ", m); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const cleanup = () => proc.kill("SIGTERM");
process.on("exit", cleanup);

const base = await new Promise((res, rej) => {
  let buf = "";
  proc.stdout.on("data", (d) => { buf += d; const m = buf.match(/http:\/\/127\.0\.0\.1:(\d+)/); if (m) res(`127.0.0.1:${m[1]}`); });
  setTimeout(() => rej(new Error("server did not start")), 15000);
});

const ws = new WebSocket(`ws://${base}/ws`);
ws.binaryType = "arraybuffer";
const state = { show: null, rt: null, library: [], hello: null, shows: null, frames: new Map(), learned: [], midiOut: [] };
const waiters = [];
let seq = 1; const acks = new Map();
ws.onmessage = (e) => {
  if (typeof e.data !== "string") { const b = new Uint8Array(e.data); state.frames.set((b[1] << 8) | b[2], b.slice(3)); }
  else {
    const m = JSON.parse(e.data);
    if (m.t === "show") state.show = m.show; else if (m.t === "rt") state.rt = m.rt; else if (m.t === "library") state.library = m.profiles;
    else if (m.t === "hello") state.hello = m; else if (m.t === "shows") state.shows = m.shows; else if (m.t === "learned") state.learned.push(m.source); else if (m.t === "midi_out") state.midiOut.push(m);
    else if (m.t === "ack") { if (m.id === "") console.log("  unmatched ack:", m.err); acks.get(m.id)?.(m); acks.delete(m.id); }
  }
  waiters.forEach((w) => w());
};
const until = async (pred, what, ms = 5000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (pred()) return true; await new Promise((r) => { waiters.push(r); setTimeout(r, 50); }); }
  console.log("  timeout waiting for", what); return false;
};
const cmd = (c) => new Promise((res) => {
  const id = String(seq++); acks.set(id, res); ws.send(JSON.stringify({ t: "cmd", id, cmd: c }));
  setTimeout(() => { if (acks.delete(id)) { console.log("  no ack for", JSON.stringify(c).slice(0, 120)); res({ ok: false, err: "timeout" }); } }, 5000);
});
await new Promise((r) => (ws.onopen = r));

console.log("snapshot");
await until(() => state.hello && state.show && state.rt && state.shows && state.library.length, "snapshot");
ok(state.hello?.version, "hello");
ok(state.library.length >= 20, `library has ${state.library.length} profiles`);
ok(state.show?.universes.length === 4, "default show has 4 universes");
ok(typeof state.rt?.fps === "number", "runtime present");

console.log("patch + programmer → DMX");
await cmd({ type: "subscribe_dmx", universes: null });
let r = await cmd({ type: "fixture_add", profile: "generic/rgb-par", mode: "4ch", count: 2, universe: 1, address: null, name: null });
ok(r.ok, "fixture_add");
await until(() => state.show.fixtures.length === 2, "2 fixtures in show");
ok(state.show.fixtures[1].address === 5, "auto address 5");
r = await cmd({ type: "fixture_add", profile: "generic/mh-spot", mode: "16ch", count: 1, universe: 1, address: 100, name: "Mover" });
ok(r.ok, "add mover at 100");
await cmd({ type: "prog_set", fixtures: [1], attrs: { dimmer: 1, red: 1 } });
ok(await until(() => state.frames.get(1)?.[0] === 255 && state.frames.get(1)?.[1] === 255, "DMX 255,255 on U1 ch1-2"), "DMX frame reflects programmer");
ok(state.frames.get(1)?.[99] === 128, "mover pan centre default (ch100=128)");
ok(await until(() => state.rt.programmer["1"]?.dimmer === 1, "rt programmer"), "runtime shows programmer");

console.log("cue record + go");
r = await cmd({ type: "prog_record_cue", cuelist: 1, number: null, mode: "merge", label: "Open", release: true });
ok(r.ok, "prog_record_cue");
await until(() => state.show.cuelists[0]?.cues.length === 1, "cue in show");
await until(() => Object.keys(state.rt.programmer).length === 0, "programmer released");
ok(await until(() => state.frames.get(1)?.[0] === 0, "dark after release"), "output dark after programmer release");
const cue = { ...state.show.cuelists[0].cues[0], fade_in: 0, fade_out: 0 };
await cmd({ type: "cue_save", cuelist: 1, cue });
r = await cmd({ type: "go", cuelist: 1 });
ok(r.ok, "go");
ok(await until(() => state.frames.get(1)?.[0] === 255 && state.rt.cuelists[0]?.current === 1, "cue output"), "cue plays 255 + rt current=1");
r = await cmd({ type: "exec_save", executor: { id: 0, page: 0, slot: 0, label: "Main", kind: { type: "cuelist", cuelist: 1 } } });
await until(() => state.show.executors.length === 1, "executor");
await cmd({ type: "exec_level", id: 1, level: 0.5 });
ok(await until(() => state.frames.get(1)?.[0] === 128, "level 0.5"), "executor fader scales dimmer to 128");
await cmd({ type: "grand_master", level: 0.5 });
ok(await until(() => state.frames.get(1)?.[0] === 64, "gm"), "grand master halves it (64)");
await cmd({ type: "blackout", on: true });
ok(await until(() => state.frames.get(1)?.[0] === 0, "blackout"), "blackout");
await cmd({ type: "blackout", on: false });

console.log("bindings + learn");
await cmd({ type: "learn_start", binding: null });
await cmd({ type: "midi_in", port: "testpad", bytes: [0xb0, 21, 99] });
ok(await until(() => state.learned.length === 1 && state.learned[0].number === 21, "learned"), "learn captures MIDI CC 21");
await cmd({ type: "binding_save", binding: { id: 0, label: "GM", enabled: true, source: state.learned[0], action: { type: "grand_master" }, out_min: 0, out_max: 1 } });
await cmd({ type: "midi_in", port: "testpad", bytes: [0xb0, 21, 127] });
ok(await until(() => state.rt.grand_master === 1, "gm via midi"), "MIDI CC drives grand master");

console.log("errors");
r = await cmd({ type: "go", cuelist: 77 });
ok(!r.ok && r.err, "unknown cuelist → ack ok:false");
ws.send("not json");
await sleep(100);
ok(ws.readyState === 1, "garbage message does not drop the connection");

console.log("show save / new / load");
r = await cmd({ type: "show_save", name: "E2E Show" });
ok(r.ok, "show_save");
await until(() => state.shows.some((s) => s.name === "E2E Show"), "shows list");
await cmd({ type: "show_new", name: "Empty" });
await until(() => state.show.fixtures.length === 0, "empty show");
r = await cmd({ type: "show_load", name: "E2E Show" });
ok(r.ok, "show_load");
ok(await until(() => state.show.fixtures.length === 3 && state.show.cuelists.length === 1, "restored"), "fixtures + cuelist restored");
r = await cmd({ type: "show_load", name: "../../etc/passwd" });
ok(!r.ok, "path traversal rejected");

console.log("REST");
let j = await (await fetch(`http://${base}/api/show`)).json();
ok(j.fixtures.length === 3, "GET /api/show");
const put = await fetch(`http://${base}/api/show`, { method: "PUT", headers: { "content-type": "application/json" }, body: JSON.stringify(j) });
ok(put.ok, "PUT /api/show");
const net = await (await fetch(`http://${base}/api/net`)).json();
ok(Array.isArray(net), "GET /api/net");
const ports = await (await fetch(`http://${base}/api/ports`)).json();
ok(Array.isArray(ports), "GET /api/ports");
const fd = new FormData(); fd.append("file", new Blob(["hello"]), "t e$t.png");
const up = await (await fetch(`http://${base}/api/media`, { method: "POST", body: fd })).json();
ok(up.url?.startsWith("/media/"), `upload → ${up.url}`);
ok((await (await fetch(`http://${base}${up.url}`)).text()) === "hello", "media served back");
const idx = await fetch(`http://${base}/`);
ok(idx.ok && (await idx.text()).includes("<"), "GET / serves UI");
ok((await fetch(`http://${base}/some/spa/route`)).ok, "SPA fallback");

console.log("virtual console");
const oscSock = dgram.createSocket("udp4"); const oscGot = [];
oscSock.on("message", (m) => oscGot.push(m.toString("latin1")));
await new Promise((r) => oscSock.bind(0, "127.0.0.1", r));
const oscPort = oscSock.address().port;
const W = (id, kind, action, mode, extra = {}) => ({ id, kind, label: `w${id}`, x: 0, y: 0, w: 2, h: 2, color: "#35c46a", action, mode, orientation: "", inputs: [], feedback: [], out_min: 0, out_max: 1, ...extra });
const page = { id: 1, name: "Main", cols: 8, rows: 6, widgets: [
  W(11, "button", { type: "blackout" }, "toggle", {
    inputs: [{ source: { type: "midi", port: null, kind: "note", channel: 1, number: 36 }, invert: false, in_min: 0, in_max: 1 }],
    feedback: [{ type: "midi", port: "APC", kind: "note", channel: 1, number: 36, on: 127, off: 0, scale: false },
               { type: "osc", host: "127.0.0.1", port: oscPort, address: "/vc/bo", on: 1, off: 0 }] }),
  W(12, "slider", { type: "grand_master" }, "trigger", {
    inputs: [{ source: { type: "midi", port: null, kind: "cc", channel: 1, number: 7 }, invert: false, in_min: 0, in_max: 1 }],
    feedback: [{ type: "midi", port: null, kind: "cc", channel: 1, number: 7, on: 127, off: 0, scale: true }] }),
  W(13, "button", { type: "dmx", universe: 2, channel: 9 }, "momentary"),
] };
r = await cmd({ type: "vc_save", pages: [page] });
ok(r.ok, "vc_save");
await until(() => state.show.vconsole?.[0]?.widgets.length === 3, "vconsole in show");
await until(() => state.rt.vc?.length === 3, "widget states in runtime");
ok(state.rt.vc.length === 3, "rt.vc has 3 widget states");
r = await cmd({ type: "vc_press", widget: 13, down: true });
ok(r.ok, "vc_press down");
await until(() => state.frames.get(2)?.[8] === 255, "momentary button drives DMX 2:9", 3000);
ok(state.frames.get(2)?.[8] === 255, "DMX u2 ch9 = 255 while held");
await cmd({ type: "vc_press", widget: 13, down: false });
await until(() => state.frames.get(2)?.[8] === 0, "release", 3000);
ok(state.frames.get(2)?.[8] === 0, "DMX u2 ch9 released");
state.midiOut.length = 0;
await cmd({ type: "midi_in", port: "APC", bytes: [0x90, 36, 100] });
await until(() => state.rt.blackout === true, "blackout via MIDI note", 3000);
ok(state.rt.blackout === true, "MIDI note toggles blackout widget");
await until(() => state.midiOut.some((m) => m.port === "APC" && m.bytes.join() === "144,36,127"), "LED feedback", 3000);
ok(state.midiOut.some((m) => m.port === "APC" && m.bytes.join() === "144,36,127"), "MidiOut LED on (note 36 vel 127)");
await until(() => oscGot.some((m) => m.startsWith("/vc/bo")), "OSC feedback", 3000);
ok(oscGot.some((m) => m.startsWith("/vc/bo")), "OSC feedback datagram received");
await cmd({ type: "midi_in", port: "APC", bytes: [0x90, 36, 100] });
await until(() => state.rt.blackout === false, "blackout off", 3000);
await cmd({ type: "midi_in", port: "APC", bytes: [0xB0, 7, 127] });
await until(() => Math.abs(state.rt.grand_master - 1) < 1e-3, "GM", 3000);
await cmd({ type: "vc_value", widget: 12, value: 0.25 });
await until(() => Math.abs(state.rt.grand_master - 0.25) < 1e-3, "GM via vc_value", 3000);
ok(Math.abs(state.rt.grand_master - 0.25) < 1e-3, "vc_value sets grand master");
await until(() => state.midiOut.some((m) => m.bytes.join() === "176,7,32"), "fader feedback", 3000);
ok(state.midiOut.some((m) => m.bytes.join() === "176,7,32"), "MidiOut scaled fader value (0.25 → 32)");
await cmd({ type: "vc_value", widget: 12, value: 1 });
r = await cmd({ type: "vc_press", widget: 999, down: true });
ok(!r.ok, "unknown widget rejected");
r = await cmd({ type: "learn_start", binding: null });
await cmd({ type: "midi_in", port: "APC", bytes: [0xB0, 21, 64] });
await until(() => state.learned.length > 0, "learned source", 3000);
ok(state.learned.at(-1)?.number === 21, "learn returns the source for the UI to store in a widget");
oscSock.close();

console.log("scenes");
await cmd({ type: "prog_release", fixtures: null, attrs: null });
await cmd({ type: "prog_set", fixtures: [2], attrs: { dimmer: 1, red: 1, green: 0, blue: 0 } });
r = await cmd({ type: "scene_record", id: null, name: "Red", from_output: false, fixtures: null, merge: false });
ok(r.ok, "scene_record from programmer");
await until(() => state.show.scenes?.length === 1, "scene in show");
const sidA = state.show.scenes[0].id;
r = await cmd({ type: "scene_record", id: null, name: "x", from_output: false, fixtures: [99], merge: false });
await cmd({ type: "prog_release", fixtures: null, attrs: null });
await cmd({ type: "scene_save", scene: { ...state.show.scenes[0], fade_in: 0, fade_out: 0 } });
ok(await until(() => state.frames.get(1)?.[4] === 0 && state.frames.get(1)?.[5] === 0, "dark again"), "programmer released, fixture 2 dark");
await cmd({ type: "scene_go", id: sidA, on: true });
ok(await until(() => state.frames.get(1)?.[4] === 255 && state.frames.get(1)?.[5] === 255, "scene lit"), "scene_go lights the recorded look (fixture 2 dimmer+red)");
ok(await until(() => state.rt.scenes?.find((x) => x.id === sidA)?.active === true, "rt scene active"), "runtime reports the scene active");
await cmd({ type: "scene_level", id: sidA, level: 0.5 });
ok(await until(() => state.frames.get(1)?.[4] === 128, "half level"), "scene_level 0.5 → dimmer 128");
await cmd({ type: "scene_level", id: sidA, level: 0 });
await until(() => state.frames.get(1)?.[4] === 0, "scene off");
await cmd({ type: "prog_set", fixtures: [2], attrs: { dimmer: 1, red: 0, green: 0, blue: 1 } });
await cmd({ type: "scene_record", id: null, name: "Blue", from_output: false, fixtures: null, merge: false });
await cmd({ type: "prog_release", fixtures: null, attrs: null });
await until(() => state.show.scenes?.length === 2, "second scene");
const sidB = state.show.scenes[1].id;
await cmd({ type: "scene_save", scene: { ...state.show.scenes[1], fade_in: 0, fade_out: 0 } });
await cmd({ type: "scene_morph", a: sidA, b: sidB, t: 0.5 });
ok(await until(() => state.frames.get(1)?.[5] === 128 && state.frames.get(1)?.[7] === 128, "morph half"), "scene_morph t=0.5 → red 128, blue 128");
ok(await until(() => state.rt.morph && state.rt.morph[2] === 0.5, "rt morph"), "runtime reports the morph");
await cmd({ type: "scene_morph", a: sidA, b: sidB, t: null });
ok(await until(() => state.frames.get(1)?.[5] === 0 && state.rt.morph === null, "morph released"), "morph released");
r = await cmd({ type: "scene_morph", a: sidA, b: sidA, t: 0.5 });
ok(!r.ok, "morph with the same scene rejected");
await cmd({ type: "scene_go", id: sidA, on: true });
await until(() => state.frames.get(1)?.[5] === 255, "scene A on");
r = await cmd({ type: "scene_record", id: null, name: "Snapshot", from_output: true, fixtures: null, merge: false });
ok(r.ok, "scene_record from output");
await until(() => state.show.scenes?.length === 3, "snapshot scene");
ok(state.show.scenes[2].values["2"]?.red === 1, "snapshot captured the lit fixture (red=1)");
r = await cmd({ type: "exec_save", executor: { id: 0, page: 7, slot: 0, label: "Scene A", kind: { type: "scene", scene: sidA } } });
ok(r.ok, "exec_save with a scene executor");
await cmd({ type: "scenes_release" });
ok(await until(() => state.frames.get(1)?.[5] === 0, "released all"), "scenes_release");
await until(() => state.show.executors?.some((x) => x.kind.type === "scene"), "executor");
const exId = state.show.executors.find((x) => x.kind.type === "scene").id;
await cmd({ type: "exec_level", id: exId, level: 1 });
ok(await until(() => state.frames.get(1)?.[5] === 255, "exec fader"), "scene executor fader lights the scene");
ok(await until(() => state.rt.executors.find((x) => x.id === exId)?.active === true, "rt exec active"), "scene executor reported active");
await cmd({ type: "scene_remove", id: sidA });
await until(() => state.show.scenes.length === 2, "scene removed");
ok(state.show.executors.find((x) => x.id === exId).kind.type === "empty", "removing a scene empties its executor");
await cmd({ type: "scenes_release" });

console.log("fixture morph");
const preview = async (b) => (await fetch(`http://${base}/api/morph/preview`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b) }));
let pr = await preview({ ids: [1, 2], profile: "generic/rgbw-par", mode: "5ch" });
const pv = await pr.json();
ok(pr.status === 200 && pv.footprint === 5 && pv.dropped.length === 0, "preview: rgb(4ch) → rgbw(5ch) loses nothing");
ok(pv.warnings.some((w) => w.includes("overlaps")), "preview warns that the row overlaps without re-address");
ok(pv.maps.some((m) => m.to === "white" && m.kind === "emulated"), "preview lists the emulated white extraction");
pr = await preview({ ids: [3], profile: "generic/rgb-par", mode: "3ch" });
const pv2 = await pr.json();
ok(pv2.dropped.includes("pan") && pv2.warnings.some((w) => w.includes("Pan/tilt")), "preview: mover → par drops pan/tilt with a warning");
pr = await preview({ ids: [1], profile: "nope", mode: "x" });
ok(pr.status === 400, "preview: unknown profile → 400");
r = await cmd({ type: "fixture_morph", ids: [1, 2], profile: "generic/rgbw-par", mode: "5ch", repack: true });
ok(r.ok, "fixture_morph with repack");
await until(() => state.show.fixtures.find((f) => f.id === 1)?.profile === "generic/rgbw-par", "morphed");
ok(state.show.fixtures.find((f) => f.id === 1).address === 1 && state.show.fixtures.find((f) => f.id === 2).address === 6, "repacked addresses 1 and 6");
ok(state.show.fixtures.find((f) => f.id === 2).name !== undefined && state.show.fixtures.length === 3, "ids and fixture count preserved");
await cmd({ type: "prog_set", fixtures: [1], attrs: { dimmer: 1, red: 1, green: 1, blue: 1 } });
ok(await until(() => state.frames.get(1)?.[0] === 255 && state.frames.get(1)?.[4] === 255 && state.frames.get(1)?.[1] === 0, "white extracted"), "white look lands on the white LED (red=0, white=255)");
r = await cmd({ type: "fixture_morph", ids: [1], profile: "nope", mode: "x", repack: false });
ok(!r.ok, "fixture_morph with unknown profile rejected");
await cmd({ type: "prog_release", fixtures: null, attrs: null });

console.log("programming: palette links, undo/redo, blind, park, cue update, manual crossfade");
{
  const J = (number, changes) => ({ number, label: "", fade_in: 0, fade_out: 0, delay: 0, follow: null, timecode: null, changes, block: false, refs: {}, timing: {}, curve: "linear", stagger: 0, notes: "" });
  await cmd({ type: "show_new", name: "prog" });
  await until(() => state.show.name === "prog" && state.show.fixtures.length === 0, "fresh show");
  r = await cmd({ type: "fixture_add", profile: "generic/rgb-par", mode: "4ch", count: 1, universe: 1, address: 1, name: null });
  ok(r.ok, "prog: patch an RGB par (dimmer, r, g, b on ch1-4)");
  await until(() => state.show.fixtures.length === 1, "fixture");
  await cmd({ type: "prog_set", fixtures: [1], attrs: { red: 1, dimmer: 1 } });
  r = await cmd({ type: "prog_record_palette", id: null, fixtures: [1], kind: "color", name: "Red" });
  ok(r.ok, "prog: record colour palette");
  await until(() => state.show.palettes.length === 1, "palette");
  const pid = state.show.palettes[0].id;
  await cmd({ type: "prog_release", fixtures: null, attrs: null });
  await cmd({ type: "prog_apply_palette", fixtures: [1], palette: pid });
  await cmd({ type: "prog_set", fixtures: [1], attrs: { dimmer: 1 } });
  r = await cmd({ type: "prog_record_cue", cuelist: 1, number: null, mode: "merge", label: "", release: true, refs: true });
  ok(r.ok, "prog: record cue with palette links");
  await until(() => state.show.cuelists[0]?.cues.length === 1, "cue");
  ok(state.show.cuelists[0].cues[0].refs["1"]?.red === pid, "cue stores the palette link (refs)");
  const c1 = { ...state.show.cuelists[0].cues[0], fade_in: 0, fade_out: 0 };
  await cmd({ type: "cue_save", cuelist: 1, cue: c1 });
  await cmd({ type: "go", cuelist: 1 });
  ok(await until(() => state.frames.get(1)?.[1] === 255, "red 255"), "cue plays the palette colour");
  const pal = { ...state.show.palettes[0], values: { ...state.show.palettes[0].values, red: 0.5 } };
  await cmd({ type: "palette_save", palette: pal });
  ok(await until(() => state.frames.get(1)?.[1] === 128, "red 128"), "editing the palette changes the running cue live (128)");
  ok(await until(() => state.rt.undo >= 1, "undo count"), "rt.undo counts the edit");
  r = await cmd({ type: "undo" });
  ok(r.ok && await until(() => state.frames.get(1)?.[1] === 255 && state.rt.redo >= 1, "undone"), "undo restores the palette value (255) and enables redo");
  r = await cmd({ type: "redo" });
  ok(r.ok && await until(() => state.frames.get(1)?.[1] === 128, "redone"), "redo re-applies it (128)");

  await cmd({ type: "prog_set", fixtures: [1], attrs: { green: 1 } });
  ok(await until(() => state.frames.get(1)?.[2] === 255, "green"), "programmer green on output");
  await cmd({ type: "blind_set", on: true });
  ok(await until(() => state.rt.blind === true && state.frames.get(1)?.[2] === 0, "blind"), "blind hides the programmer from the output");
  await cmd({ type: "blind_set", on: false });
  ok(await until(() => state.frames.get(1)?.[2] === 255, "unblind"), "unblind brings it back");
  await cmd({ type: "prog_release", fixtures: null, attrs: null });

  await cmd({ type: "dmx_set", universe: 1, channel: 20, value: 77 });
  ok(await until(() => state.rt.parked?.some((p) => p.channel === 20 && p.value === 77), "parked"), "dmx_set shows up as parked");
  await cmd({ type: "unpark", universe: null, channel: null });
  ok(await until(() => state.rt.parked?.length === 0, "unparked"), "unpark clears it");

  r = await cmd({ type: "cue_load", cuelist: 1, number: 1 });
  ok(r.ok && await until(() => state.rt.programmer["1"]?.red === 0.5, "loaded"), "cue_load puts the cue look into the programmer");
  await cmd({ type: "prog_set", fixtures: [1], attrs: { blue: 0.4 } });
  r = await cmd({ type: "cue_update", cuelist: 1, number: 1, mode: "merge", refs: true });
  ok(r.ok && await until(() => state.show.cuelists[0].cues[0].changes["1"]?.blue === 0.4, "updated"), "cue_update merges the programmer into the cue");
  await cmd({ type: "prog_release", fixtures: null, attrs: null });
  r = await cmd({ type: "cue_update", cuelist: 1, number: 99, mode: "merge", refs: true });
  ok(!r.ok, "cue_update on a missing cue is rejected");

  // manual crossfade
  await cmd({ type: "cuelist_save", cuelist: { id: 2, name: "XF", cues: [J(1, { 1: { dimmer: 0.2 } }), J(2, { 1: { dimmer: 1 } })], looped: false, tracking: true, priority: 0, auto_mark: false } });
  await cmd({ type: "go_to", cuelist: 2, number: 1 });
  await cmd({ type: "cuelist_manual", cuelist: 2, progress: 0.5 });
  ok(await until(() => state.rt.cuelists.find((c) => c.id === 2)?.manual === true, "manual"), "cuelist_manual starts a manual crossfade");
  const p = state.rt.cuelists.find((c) => c.id === 2).progress;
  ok(Math.abs(p - 0.5) < 0.05, `manual progress follows the fader (${p.toFixed(2)})`);
  await cmd({ type: "cuelist_manual", cuelist: 2, progress: 1 });
  ok(await until(() => state.rt.cuelists.find((c) => c.id === 2)?.manual === false && state.rt.cuelists.find((c) => c.id === 2)?.current === 2, "manual done"), "fader at the end completes the cue");
}

console.log("idle CPU");
const cpu = () => { const f = readFileSync(`/proc/${proc.pid}/stat`, "utf8").split(") ")[1].split(" "); return (+f[11] + +f[12]) / 100; };
await cmd({ type: "prog_release", fixtures: null, attrs: null });
await sleep(1500);
const c0 = cpu(); await sleep(3000); const c1 = cpu();
const pct = ((c1 - c0) / 3) * 100;
ok(pct < 8, `idle CPU ${pct.toFixed(1)}% (debug build)`);

await sleep(300);
console.log("autosave file:", readdirSync(data).join(", "));
cleanup();
console.log(failed ? `\n${failed} FAILED` : "\nALL OK");
process.exit(failed ? 1 : 0);
