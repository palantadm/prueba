// E2E for the data/network features: PDF reports, patch CSV, MVR round trip, backups, Art-Net node table,
// output test and identify. Boots the binary on a free port with a temp data dir.
// Usage: node scripts/e2e-data.mjs [binary] [path/to/fixture.gdtf]
import { spawn } from "node:child_process";
import dgram from "node:dgram";
import { mkdtempSync, existsSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const bin = process.argv[2] ?? "target/debug/lumos";
const gdtfPath = process.argv[3] ?? "/tmp/gdtf-samples/MegaPointe.gdtf";
const data = mkdtempSync(join(tmpdir(), "lumos-data-"));
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
const st = { show: null, library: [], rt: null, backups: [], nodes: [], frames: new Map() };
let seq = 1; const acks = new Map();
ws.onmessage = (e) => {
  if (typeof e.data !== "string") { const b = new Uint8Array(e.data); st.frames.set((b[1] << 8) | b[2], b.slice(3)); return; }
  const m = JSON.parse(e.data);
  if (m.t === "show") st.show = m.show; else if (m.t === "library") st.library = m.profiles;
  else if (m.t === "rt") st.rt = m.rt; else if (m.t === "backups") st.backups = m.items; else if (m.t === "nodes") st.nodes = m.nodes;
  else if (m.t === "ack") { acks.get(m.id)?.(m); acks.delete(m.id); }
};
const cmd = (c) => new Promise((res) => { const id = String(seq++); acks.set(id, res); ws.send(JSON.stringify({ t: "cmd", id, cmd: c })); });
const until = async (p, what, ms = 5000) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { if (p()) return true; await new Promise((r) => setTimeout(r, 40)); } console.log("  timeout:", what); return false; };
const http = (p, opt) => fetch(`http://${base}${p}`, opt);
await new Promise((r) => (ws.onopen = r));
await until(() => st.show && st.library.length && st.rt, "snapshot");
await cmd({ type: "subscribe_dmx", universes: null });

console.log("setup: patch a rig");
await cmd({ type: "show_rename", name: "Función E2E" });
await cmd({ type: "fixture_add", profile: "generic/rgb-par", mode: "3ch", count: 3, universe: 1, address: 1, name: "Par" });
await cmd({ type: "fixture_add", profile: "generic/mh-spot", mode: st.library.find((p) => p.id === "generic/mh-spot").modes[0].name, count: 1, universe: 1, address: 100, name: "Mover" });
await until(() => st.show.fixtures.length === 4, "4 fixtures");
await cmd({ type: "fixture_update", fixture: { ...st.show.fixtures[0], pos: [-1.5, 5, 0.25], rot: [-30, 10, 0] } });
await cmd({ type: "palette_save", palette: { id: 0, name: "Rojo", kind: "color", values: { red: 1 }, per_fixture: {} } });
await cmd({ type: "cuelist_save", cuelist: { id: 0, name: "Principal", cues: [], looped: false, tracking: true, priority: 0, auto_mark: false } });
await until(() => st.show.cuelists.length === 1, "cuelist");

console.log("reports");
for (const k of ["patch", "channels", "cues", "power", "scenes"]) {
  const r = await http(`/api/report/${k}.pdf`);
  const b = Buffer.from(await r.arrayBuffer());
  ok(r.ok && r.headers.get("content-type") === "application/pdf" && b.subarray(0, 8).toString() === "%PDF-1.4" && b.subarray(-6).toString() === "%%EOF\n", `${k}.pdf (${b.length} B)`);
  if (k === "patch") ok(b.includes(Buffer.from("Lista de parcheo")) && b.includes(Buffer.from("(Mover)")), "patch report lists fixtures (Spanish)");
}
const en = Buffer.from(await (await http("/api/report/power.pdf?lang=en")).arrayBuffer());
ok(en.includes(Buffer.from("Power report")), "lang=en");
ok((await http("/api/report/nope.pdf")).status === 404, "unknown report → 404");

console.log("csv");
const csv = await (await http("/api/export/patch.csv")).text();
ok(csv.startsWith("id,name,profile,mode,universe,address,x,y,z") && csv.split(/\r?\n/).filter(Boolean).length === 5, "export has header + 4 rows");
const edited = csv.replace("Par 1", "Frontal 1").replace(/\r?\n$/, "\n") + `9,Nuevo,generic/rgb-par,3ch,2,1,0,0,0\n`;
const form = new FormData(); form.append("file", new Blob([edited]), "patch.csv");
let r = await http("/api/import/patch.csv", { method: "POST", body: form });
let j = await r.json();
ok(r.ok && j.created === 1 && j.updated === 4, `import upsert ${JSON.stringify({ c: j.created, u: j.updated })}`);
await until(() => st.show.fixtures.some((f) => f.name === "Frontal 1") && st.show.fixtures.some((f) => f.id === 9), "show updated");
const f1 = st.show.fixtures.find((f) => f.name === "Frontal 1");
ok(Math.abs(f1.pos[0] + 1.5) < 1e-3 && f1.rot[0] === -30, "position/rotation kept");
ok(st.show.universes.some((u) => u.id === 2), "universe 2 created");
const bad = new FormData(); bad.append("file", new Blob(["id,name,profile,mode,universe,address\n1,A,generic/rgb-par,3ch,1,10\n2,B,nope,,1,5\nx,C,generic/rgb-par,3ch,1,1\n"]), "bad.csv");
r = await http("/api/import/patch.csv", { method: "POST", body: bad });
j = await r.json();
ok(r.status === 422 && j.imported === 0 && j.errors.length === 2 && j.errors[0].line === 3 && j.errors[1].line === 4, `atomic errors ${JSON.stringify(j.errors?.map((e) => e.line))}`);
ok(st.show.fixtures.find((f) => f.id === 1).address === 1, "show untouched by the failed import");
const rep = new FormData(); rep.append("file", new Blob(["id,profile,universe,address\n1,generic/rgb-par,1,1\n"]), "r.csv");
r = await http("/api/import/patch.csv?replace=1", { method: "POST", body: rep });
j = await r.json();
ok(r.ok && j.removed === 4, "replace=1 removes the others");
await until(() => st.show.fixtures.length === 1, "replaced");
// restore the rig for later checks
const back = new FormData(); back.append("file", new Blob([csv]), "back.csv");
await http("/api/import/patch.csv", { method: "POST", body: back });
await until(() => st.show.fixtures.length === 4, "rig restored");

console.log("mvr");
if (existsSync(gdtfPath)) {
  const lib = new FormData(); lib.append("file", new Blob([readFileSync(gdtfPath)]), "MegaPointe.gdtf");
  const li = await (await http("/api/library/import", { method: "POST", body: lib })).json();
  const gid = li.imported[0].id;
  await until(() => st.library.some((p) => p.id === gid), "gdtf in library");
  const mode = st.library.find((p) => p.id === gid).modes[0].name;
  await cmd({ type: "fixture_add", profile: gid, mode, count: 2, universe: 1, address: 200, name: "MP" });
  await until(() => st.show.fixtures.length === 6, "6 fixtures");
  const mp = st.show.fixtures.find((f) => f.name === "MP 1");
  await cmd({ type: "fixture_update", fixture: { ...mp, pos: [2, 6, -1], rot: [-45, 15, 0] } });
  await until(() => st.show.fixtures.find((f) => f.id === mp.id).pos[0] === 2, "mp positioned");
  const mr = await http("/api/export/show.mvr");
  const zip = Buffer.from(await mr.arrayBuffer());
  ok(mr.ok && zip.subarray(0, 2).toString() === "PK" && zip.includes(Buffer.from("GeneralSceneDescription.xml")), `export.mvr (${zip.length} B)`);
  ok(zip.includes(Buffer.from(".gdtf")), "GDTF embedded in the archive");
  // import into a brand-new show (fixtures replaced)
  await cmd({ type: "show_new", name: "Vacío" });
  await until(() => st.show.name === "Vacío" && st.show.fixtures.length === 0, "new show");
  const mf = new FormData(); mf.append("file", new Blob([zip]), "rig.mvr");
  r = await http("/api/import/mvr", { method: "POST", body: mf });
  j = await r.json();
  ok(r.ok && j.created === 6 && j.unmatched.length === 0, `import.mvr ${JSON.stringify({ created: j.created, unmatched: j.unmatched?.length })}`);
  await until(() => st.show.fixtures.length === 6, "fixtures imported");
  const m2 = st.show.fixtures.find((f) => f.name === "MP 1");
  ok(m2 && m2.profile === gid && Math.abs(m2.pos[0] - 2) < 1e-3 && Math.abs(m2.pos[1] - 6) < 1e-3 && Math.abs(m2.pos[2] + 1) < 1e-3, `MP 1 position ${JSON.stringify(m2?.pos)}`);
  ok(m2.universe === 1 && m2.address === 200, "MP 1 address");
  ok(!!st.show.fixtures.find((f) => f.name === "Par 1"), "builtin-profile fixtures matched by name");
  const bogus = new FormData(); bogus.append("file", new Blob(["not a zip"]), "x.mvr");
  ok((await http("/api/import/mvr", { method: "POST", body: bogus })).status === 400, "garbage MVR → 400");
} else console.log("  skip: no GDTF sample at", gdtfPath);

console.log("backups");
await cmd({ type: "show_rename", name: "Estado A" });
let a = await cmd({ type: "backup_create", label: "  punto A " });
ok(a.ok, "backup_create");
await until(() => st.backups.some((b) => b.label === "punto A" && b.kind === "manual"), "backups message");
const idA = st.backups.find((b) => b.label === "punto A").id;
await cmd({ type: "show_rename", name: "Estado B" });
a = await cmd({ type: "backup_restore", id: idA });
ok(a.ok, "backup_restore");
await until(() => st.show.name === "Estado A", "show restored");
await until(() => st.backups.some((b) => b.kind === "auto" && b.label === "Antes de restaurar"), "auto snapshot of replaced state");
ok(readdirSync(join(data, "backups")).filter((f) => f.endsWith(".json") && !f.endsWith(".meta.json")).length >= 2, "files in <data>/backups");
a = await cmd({ type: "backup_delete", id: idA });
await until(() => !st.backups.some((b) => b.id === idA), "deleted");
ok(!(await cmd({ type: "backup_restore", id: "../etc" })).ok, "path traversal rejected");

console.log("output test / identify");
await cmd({ type: "subscribe_dmx", universes: [1] });
a = await cmd({ type: "output_test", mode: { type: "level", universe: 1, level: 90 } });
ok(a.ok, "output_test level");
ok(await until(() => st.frames.get(1) && st.frames.get(1).every((v) => v === 90) && st.rt.testing, "all channels 90 + rt.testing"), "universe filled");
await cmd({ type: "output_test", mode: { type: "channel", universe: 1, channel: 7, level: 200 } });
ok(await until(() => st.frames.get(1)?.[6] === 200 && st.frames.get(1).filter((v) => v).length === 1, "single channel"), "channel test");
ok(!(await cmd({ type: "output_test", mode: { type: "level", universe: 77, level: 1 } })).ok, "unknown universe rejected");
await cmd({ type: "output_test", mode: { type: "off" } });
ok(await until(() => !st.rt.testing, "testing off"), "test off");
await cmd({ type: "blackout", on: true });
await cmd({ type: "identify", fixture: st.show.fixtures[0].id, on: true });
ok(await until(() => st.rt.identifying.includes(st.show.fixtures[0].id), "rt.identifying"), "identifying reported");
const addr = st.show.fixtures[0].address - 1;
ok(await until(() => { const f = st.frames.get(st.show.fixtures[0].universe); return f && f[addr] === 255 && f[addr + 1] === 255; }, "identify lights the fixture in blackout", 3000), "identify strobe on (blackout)");
await cmd({ type: "identify", fixture: st.show.fixtures[0].id, on: false });
await cmd({ type: "blackout", on: false });
ok(!(await cmd({ type: "identify", fixture: 4242, on: true })).ok, "unknown fixture rejected");

console.log("art-net nodes");
await cmd({ type: "universe_save", universe: { id: 1, name: "Main", outputs: [{ enabled: true, kind: { type: "art_net", host: "127.0.0.1", port: 6454, net: 0, subnet: 0, universe: 0 } }] } });
await new Promise((r) => setTimeout(r, 400)); // the listener binds :6454 after configure
const reply = Buffer.alloc(239);
reply.write("Art-Net\0", 0, "latin1"); reply.writeUInt16LE(0x2100, 8); reply[10] = 10; reply[11] = 9; reply[12] = 8; reply[13] = 7;
reply.write("Rack Node", 26, "latin1"); reply.write("Rack Node - 4 port gateway", 44, "latin1");
reply[173] = 2; reply[174] = 0x80; reply[175] = 0x80; reply[190] = 3; reply[191] = 4; reply[211] = 1;
const sock = dgram.createSocket("udp4");
await cmd({ type: "nodes_scan" });
for (let i = 0; i < 5; i++) { sock.send(reply, 6454, "127.0.0.1"); await new Promise((r) => setTimeout(r, 100)); }
ok(await until(() => st.nodes.some((n) => n.ip === "10.9.8.7"), "nodes message", 4000), "ArtPollReply → nodes message");
const nd = st.nodes.find((n) => n.ip === "10.9.8.7");
ok(nd && nd.name === "Rack Node" && JSON.stringify(nd.universes) === "[3,4]", `node ${JSON.stringify(nd)}`);
sock.close();

console.log(failed ? `\nFAILED: ${failed}` : "\nALL OK");
process.exit(failed ? 1 : 0);
