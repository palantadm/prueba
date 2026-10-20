// E2E for the fixture-library import: boots the binary, uploads a real GDTF (and QXF) through
// POST /api/library/import, patches it, checks DMX output, extracted assets and LibraryRemove.
// Usage: node scripts/e2e-library.mjs [binary] [path/to/fixture.gdtf]
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const bin = process.argv[2] ?? "target/debug/lumos";
const gdtfPath = process.argv[3] ?? "/tmp/gdtf-samples/MegaPointe.gdtf";
if (!existsSync(gdtfPath)) { console.log(`skip: ${gdtfPath} not found`); process.exit(0); }
const data = mkdtempSync(join(tmpdir(), "lumos-lib-"));
const proc = spawn(bin, ["--port", "0", "--bind", "127.0.0.1", "--data", data], { stdio: ["ignore", "pipe", "inherit"] });
let failed = 0;
const ok = (c, m) => { if (!c) { failed++; console.log("  FAIL", m); } else console.log("  ok  ", m); };
process.on("exit", () => proc.kill("SIGTERM"));
const base = await new Promise((res, rej) => {
  let buf = "";
  proc.stdout.on("data", (d) => { buf += d; const m = buf.match(/http:\/\/127\.0\.0\.1:(\d+)/); if (m) res(`127.0.0.1:${m[1]}`); });
  setTimeout(() => rej(new Error("server did not start")), 15000);
});

const ws = new WebSocket(`ws://${base}/ws`); ws.binaryType = "arraybuffer";
const st = { show: null, library: [], frames: new Map() };
let seq = 1; const acks = new Map();
ws.onmessage = (e) => {
  if (typeof e.data !== "string") { const b = new Uint8Array(e.data); st.frames.set((b[1] << 8) | b[2], b.slice(3)); return; }
  const m = JSON.parse(e.data);
  if (m.t === "show") st.show = m.show; else if (m.t === "library") st.library = m.profiles;
  else if (m.t === "ack") { acks.get(m.id)?.(m); acks.delete(m.id); }
};
const cmd = (c) => new Promise((res) => { const id = String(seq++); acks.set(id, res); ws.send(JSON.stringify({ t: "cmd", id, cmd: c })); });
const until = async (p, what, ms = 5000) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { if (p()) return true; await new Promise((r) => setTimeout(r, 40)); } console.log("  timeout:", what); return false; };
await new Promise((r) => (ws.onopen = r));
await until(() => st.show && st.library.length, "snapshot");

console.log("import");
const form = new FormData();
form.append("file", new Blob([readFileSync(gdtfPath)]), "MegaPointe.gdtf");
form.append("file", new Blob(['<FixtureDefinition xmlns="http://www.qlcplus.org/FixtureDefinition"><Manufacturer>Acme</Manufacturer><Model>Q1</Model><Type>Dimmer</Type><Channel Name="D"><Group Byte="0">Intensity</Group></Channel><Mode Name="1ch"><Channel Number="0">D</Channel></Mode></FixtureDefinition>']), "q1.qxf");
form.append("file", new Blob(["nope"]), "x.txt");
const res = await fetch(`http://${base}/api/library/import`, { method: "POST", body: form });
const body = await res.json();
ok(res.ok && body.imported.length === 2, `imported ${body.imported.map((i) => i.name).join(", ")}`);
ok(body.errors.length === 1 && body.errors[0].file === "x.txt", "unsupported file reported");
const gid = body.imported.find((i) => i.id.startsWith("gdtf/"))?.id;
ok(gid, `gdtf id ${gid}`);
await until(() => st.library.some((p) => p.id === gid), "library message");
const prof = st.library.find((p) => p.id === gid);
ok(prof?.source === "gdtf" && prof.model3d?.root, "profile has model3d");
ok(prof?.model3d.assets.startsWith("/library/"), `assets base ${prof?.model3d.assets}`);
ok(st.library.some((p) => p.source === "qxf"), "qxf profile in library");

console.log("assets");
const dirUrl = `http://${base}${prof.model3d.assets}`;
const modelRel = prof.model3d.root.model;
const glb = await fetch(dirUrl + encodeURI(modelRel));
const glbBytes = new Uint8Array(await glb.arrayBuffer());
ok(glb.ok && String.fromCharCode(...glbBytes.slice(0, 4)) === "glTF", `GET ${modelRel} → GLB (${glbBytes.length} B)`);
const r3ds = await fetch(dirUrl + encodeURI(modelRel.replace(/\.glb$/, ".3ds").replace("models/", "models/3ds/")));
ok(r3ds.ok && (await r3ds.arrayBuffer()).byteLength > 1000, "GET original .3ds");
const trav = await fetch(`http://${base}/library/..%2F..%2Fautosave.json`);
ok(!trav.ok || !(await trav.text()).includes('"universes"'), "path traversal blocked");

console.log("patch + DMX");
const mode = prof.modes[0];
await cmd({ type: "subscribe_dmx", universes: null });
let r = await cmd({ type: "fixture_add", profile: gid, mode: mode.name, count: 1, universe: 1, address: 1, name: "MP" });
ok(r.ok, "fixture_add with imported GDTF profile " + (r.err ?? ""));
await until(() => st.show.fixtures.length === 1, "fixture");
const fid = st.show.fixtures[0].id;
const ch = (a) => mode.channels.find((c) => c.attr === a);
await cmd({ type: "prog_set", fixtures: [fid], attrs: { pan: 0.75, tilt: 0.25, dimmer: 1 } });
await until(() => { const f = st.frames.get(1); return f && f[ch("dimmer").offset] === 255; }, "dimmer frame");
const f1 = st.frames.get(1);
const v16 = (c) => (f1[c.offset] << 8) | f1[c.fine];
ok(Math.abs(v16(ch("pan")) / 65535 - 0.75) < 0.002, `pan 16-bit = ${v16(ch("pan"))}`);
ok(Math.abs(v16(ch("tilt")) / 65535 - 0.25) < 0.002, `tilt 16-bit = ${v16(ch("tilt"))}`);
ok(f1[ch("dimmer").offset] === 255, "dimmer coarse 255");

console.log("remove");
r = await cmd({ type: "library_remove", id: gid });
ok(!r.ok, "remove refused while patched");
await cmd({ type: "fixture_remove", ids: [fid] });
await until(() => st.show.fixtures.length === 0, "fixture removed");
r = await cmd({ type: "library_remove", id: gid });
ok(r.ok, "remove after unpatch");
await until(() => !st.library.some((p) => p.id === gid), "profile gone");
ok(!st.library.some((p) => p.id === gid), "library updated");
ok(!(await fetch(dirUrl + encodeURI(modelRel))).ok, "assets deleted");

console.log(failed ? `\n${failed} FAILED` : "\nALL OK");
process.exit(failed ? 1 : 0);
