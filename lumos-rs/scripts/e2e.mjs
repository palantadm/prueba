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
