// End-to-end smoke test: boots the server binary on a free port and drives it over WebSocket/REST.
// Usage: node scripts/e2e.mjs [path-to-binary]   (Node ≥ 22: uses the built-in WebSocket client)
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

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
const state = { show: null, rt: null, library: [], hello: null, shows: null, frames: new Map(), learned: [] };
const waiters = [];
let seq = 1; const acks = new Map();
ws.onmessage = (e) => {
  if (typeof e.data !== "string") { const b = new Uint8Array(e.data); state.frames.set((b[1] << 8) | b[2], b.slice(3)); }
  else {
    const m = JSON.parse(e.data);
    if (m.t === "show") state.show = m.show; else if (m.t === "rt") state.rt = m.rt; else if (m.t === "library") state.library = m.profiles;
    else if (m.t === "hello") state.hello = m; else if (m.t === "shows") state.shows = m.shows; else if (m.t === "learned") state.learned.push(m.source);
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
