// E2E for the Guion (script / running order): runtime pointer, mark actions (console, OSC, MIDI, HTTP), link + follow mode,
// command-line verbs, text / PDF import (incl. scanned → 422) and keyword suggestions.
// Boots the binary on a free port with a temp data dir.
// Usage: node scripts/e2e-script.mjs [binary]
import { spawn } from "node:child_process";
import dgram from "node:dgram";
import http from "node:http";
import { mkdtempSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const bin = process.argv[2] ?? "target/debug/lumos";
const data = mkdtempSync(join(tmpdir(), "lumos-script-"));
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
const st = { show: null, rt: null, midiOut: [], logs: [] };
let seq = 1; const acks = new Map();
ws.onmessage = (e) => {
  if (typeof e.data !== "string") return;
  const m = JSON.parse(e.data);
  if (m.t === "show") st.show = m.show;
  else if (m.t === "rt") st.rt = m.rt;
  else if (m.t === "midi_out") st.midiOut.push(m);
  else if (m.t === "log") st.logs.push(m);
  else if (m.t === "ack") { acks.get(m.id)?.(m); acks.delete(m.id); }
};
const cmd = (c) => new Promise((res) => { const id = String(seq++); acks.set(id, res); ws.send(JSON.stringify({ t: "cmd", id, cmd: c })); });
const until = async (p, what, ms = 5000) => { const t0 = Date.now(); while (Date.now() - t0 < ms) { if (p()) return true; await new Promise((r) => setTimeout(r, 40)); } console.log("  timeout:", what); return false; };
const api = (p, opt) => fetch(`http://${base}${p}`, opt);
await new Promise((r) => (ws.onopen = r));
await until(() => st.show && st.rt, "snapshot");

// listeners for the external actions
const udp = dgram.createSocket("udp4");
const oscIn = [];
udp.on("message", (b) => oscIn.push(b));
await new Promise((r) => udp.bind(0, "127.0.0.1", r));
const oscPort = udp.address().port;
const hits = [];
const srv = http.createServer((req, res) => {
  let body = ""; req.on("data", (d) => (body += d));
  req.on("end", () => { hits.push({ method: req.method, url: req.url, body }); res.end("ok"); });
});
await new Promise((r) => srv.listen(0, "127.0.0.1", r));
const httpPort = srv.address().port;
const oscAddr = (b) => b.toString("latin1").split("\0")[0];

console.log("setup: a dimmer, a cuelist with two cues");
await cmd({ type: "fixture_add", profile: "generic/dimmer", mode: "1ch", count: 1, universe: 1, address: 1, name: "Dim" });
await until(() => st.show.fixtures.length === 1, "fixture");
ok((await cmd({ type: "command_line", text: "fixture 1 at 100; record cue 1 cuelist 1 name Entrada; fixture 1 at 30; record cue 2 cuelist 1 name Salida; clear" })).ok, "record two cues");
await until(() => st.show.cuelists.some((c) => c.id === 1 && c.cues.length === 2), "cuelist 1 has 2 cues");

console.log("script with marks");
const mk = (id, at_word, actions, extra = {}) => ({ id, label: `M${id}`, kind: "light", at_word, actions, link: null, standby_blocks: 0, note: "", ...extra });
const script = {
  id: 0, name: "Obra E2E", pdf: null,
  blocks: [
    { id: 1, kind: "heading", speaker: "", text: "ACTO I", page: 0, marks: [] },
    { id: 2, kind: "speech", speaker: "ANA", text: "Buenas noches a todos", page: 0, marks: [
      mk(1, 0, [{ type: "console", text: "blackout on" }]),
      mk(2, 2, [
        { type: "osc", host: "127.0.0.1", port: oscPort, address: "/qlab/go", args: ["12", "0.5", "hola"] },
        { type: "midi", port: null, kind: "note", channel: 1, number: 60, value: 100 },
        { type: "http", method: "POST", url: `http://127.0.0.1:${httpPort}/hook`, body: "{\"x\":1}" },
        { type: "console", text: "script 99 go" },
        { type: "console", text: "blackout off" },
      ]),
    ] },
    { id: 3, kind: "direction", speaker: "", text: "(Cae el telón)", page: 0, marks: [mk(3, 0, [], { link: { cuelist: 1, number: 2 }, kind: "set" })] },
  ],
};
{ const a = await cmd({ type: "script_save", script }); ok(a.ok, `script_save ${a.err ?? ""}`); }
await until(() => st.show.scripts.length === 1, "script stored");
const sid = st.show.scripts[0].id;
const rts = () => st.rt.scripts.find((s) => s.id === sid);
ok(await until(() => rts()?.next?.[1] === 1, "next = mark 1"), "pointer starts at the first mark");

console.log("go / back / actions");
await cmd({ type: "script_go", id: sid });
ok(await until(() => st.rt.blackout && rts().mark === 1, "mark 1 fired"), "GO fires mark 1 (console blackout on)");
await cmd({ type: "script_go", id: sid });
ok(await until(() => oscIn.length === 1, "osc received"), "OSC action sent");
ok(oscAddr(oscIn[0]) === "/qlab/go" && oscIn[0].toString("latin1").includes(",ifs"), "OSC address + typed args (int, float, string)");
ok(await until(() => st.midiOut.length === 1, "midi_out"), "MIDI action relayed to the browser");
ok(JSON.stringify(st.midiOut[0].bytes) === "[144,60,100]", `MIDI bytes ${JSON.stringify(st.midiOut[0]?.bytes)}`);
ok(await until(() => hits.length === 1, "http hit"), "HTTP action sent");
ok(hits[0].method === "POST" && hits[0].url === "/hook" && hits[0].body === "{\"x\":1}", `HTTP ${JSON.stringify(hits[0])}`);
ok(await until(() => !st.rt.blackout, "blackout off"), "a failing action did not stop the rest (blackout off ran)");
ok(st.rt.cmd_log.some((l) => !l.ok), "failure recorded in cmd_log");
await cmd({ type: "script_back", id: sid });
ok(await until(() => rts().next?.[1] === 2, "next = mark 2 again"), "BACK re-arms the previous mark without firing");
ok(oscIn.length === 1 && hits.length === 1, "back fired nothing");
await cmd({ type: "script_goto", id: sid, block: 3, mark: null });
ok(await until(() => rts().next?.[1] === 3, "goto block 3"), "GOTO moves the pointer");
ok(oscIn.length === 1, "goto fired nothing");
await cmd({ type: "script_go", id: sid });
ok(await until(() => st.rt.cuelists.find((c) => c.id === 1)?.current === 2, "linked cue 2"), "mark with a link GOes its cue");
await cmd({ type: "script_reset", id: sid });
ok(await until(() => rts().mark === null && rts().next?.[1] === 1, "reset"), "reset rewinds");

console.log("follow mode");
await cmd({ type: "script_follow", id: sid, on: true });
ok(await until(() => rts().follow, "follow on"), "follow on");
const before = oscIn.length;
await cmd({ type: "command_line", text: "go cuelist 1" }); // cue 1 (current is 2 → wraps/advances) — any start moves the reader if linked
await cmd({ type: "command_line", text: "goto cue 2 cuelist 1" });
ok(await until(() => rts().mark === 3, "follow → mark 3"), "starting the linked cue elsewhere moves the reader to its mark");
ok(oscIn.length === before, "follow does not fire marks");
await cmd({ type: "script_follow", id: sid, on: false });

console.log("command line");
await cmd({ type: "command_line", text: "script reset" });
ok(await until(() => rts().next?.[1] === 1, "cl reset"), "script reset");
await cmd({ type: "command_line", text: "guion go" });
ok(await until(() => rts().mark === 1, "cl go"), "guion go (Spanish alias)");
await cmd({ type: "command_line", text: "blackout off" });
const n0 = oscIn.length;
ok((await cmd({ type: "command_line", text: `osc 127.0.0.1:${oscPort} /cl/test 7 "a b"` })).ok, "osc verb accepted");
ok(await until(() => oscIn.length === n0 + 1 && oscAddr(oscIn[n0]) === "/cl/test", "cl osc"), "osc verb sends");
const m0 = st.midiOut.length;
await cmd({ type: "command_line", text: "midi cc 2 7 64" });
ok(await until(() => st.midiOut.length === m0 + 1 && JSON.stringify(st.midiOut[m0].bytes) === "[177,7,64]", "cl midi"), "midi verb sends");
await cmd({ type: "command_line", text: `http get http://127.0.0.1:${httpPort}/ping?x=1#f` });
ok(await until(() => hits.some((h) => h.method === "GET" && h.url === "/ping?x=1"), "cl http"), "http verb sends");
const bad = await cmd({ type: "command_line", text: "osc nonsense" });
ok(!bad.ok && /host/.test(bad.err ?? ""), `syntax error reported: ${bad.err}`);
const help = await (await api("/api/cmdline/help")).json();
ok(["script", "osc", "midi", "http"].every((n) => help.statements.some((s) => s.name === n)), "help documents the new verbs");

console.log("http failure is reported");
await cmd({ type: "command_line", text: "http get http://127.0.0.1:1/nope" });
ok(await until(() => st.logs.some((l) => l.level === "error" && /127\.0\.0\.1/.test(l.msg)), "error log", 8000), "unreachable HTTP target → error log");

console.log("import");
const form = (name, bytes, type = "application/octet-stream") => { const f = new FormData(); f.append("file", new Blob([bytes], { type }), name); return f; };
const txt = "ACTO I\nESCENA 1\n\n(Una cocina oscura. Suena un teléfono.)\n\nDOÑA INÉS.- ¡Qué hora es!\nJUAN: Las doce.\n";
let r = await api("/api/script/import", { method: "POST", body: form("la casa.txt", txt) });
let j = await r.json();
ok(r.status === 200 && j.blocks.length === 5, `txt import ${r.status} blocks=${j.blocks?.length}`);
ok(j.blocks[3].speaker === "DOÑA INÉS" && j.blocks[4].speaker === "JUAN" && j.blocks[2].kind === "direction", "speakers + directions detected");

function pdf(pages) {
  const objs = [];
  const add = (s) => { objs.push(s); return objs.length; };
  add("<< /Type /Catalog /Pages 2 0 R >>");
  add(`<< /Type /Pages /Kids [${pages.map((_, i) => `${5 + i * 2} 0 R`).join(" ")}] /Count ${pages.length} >>`);
  add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>");
  add("<< >>");
  pages.forEach((lines, i) => {
    const content = lines.map((l, k) => `BT /F1 11 Tf 60 ${780 - k * 16} Td (${l}) Tj ET`).join("\n");
    add(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R >> >> /Contents ${6 + i * 2} 0 R >>`);
    add(`<< /Length ${content.length} >>\nstream\n${content}\nendstream`);
  });
  let out = "%PDF-1.4\n"; const offs = [];
  objs.forEach((o, i) => { offs.push(out.length); out += `${i + 1} 0 obj\n${o}\nendobj\n`; });
  const x = out.length;
  out += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n${offs.map((o) => `${String(o).padStart(10, "0")} 00000 n \n`).join("")}trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${x}\n%%EOF\n`;
  return Buffer.from(out, "latin1");
}
r = await api("/api/script/import", { method: "POST", body: form("obra.pdf", pdf([["ACTO I", "ANA.- Buenas noches.", "LUIS.- Hola."], ["(Se apaga la luz.)", "ANA.- Que oscuro."]]), "application/pdf") });
j = await r.json();
ok(r.status === 200 && j.pages === 2 && j.blocks.length === 5, `pdf import ${r.status} pages=${j.pages} blocks=${j.blocks?.length} ${JSON.stringify(j).slice(0, 200)}`);
ok(j.blocks[3]?.page === 2 && j.blocks[1]?.speaker === "ANA", "per-block page + speaker");
ok(typeof j.pdf === "string" && j.pdf.startsWith("/media/") && readdirSync(join(data, "media")).some((f) => j.pdf.endsWith(f)), "original PDF stored in media");
ok((await api(j.pdf)).status === 200, "stored PDF is served");
r = await api("/api/script/import", { method: "POST", body: form("scan.pdf", pdf([[], []]), "application/pdf") });
j = await r.json();
ok(r.status === 422 && j.code === "scanned_pdf", `scanned PDF → 422 (${r.status} ${j.code})`);
r = await api("/api/script/import", { method: "POST", body: form("x.docx", "nope") });
ok(r.status === 415, `unsupported type → 415 (${r.status})`);

console.log("suggest");
r = await api("/api/script/suggest", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ blocks: [
  { id: 1, kind: "direction", speaker: "", text: "Se apaga la luz y suena el teléfono", page: 0, marks: [] },
  { id: 2, kind: "speech", speaker: "ANA", text: "la luz de tus ojos", page: 0, marks: [] },
] }) });
j = await r.json();
const kinds = j.suggestions.map((s) => `${s.block}:${s.kind}`).sort();
ok(JSON.stringify(kinds) === JSON.stringify(["1:light", "1:sound"]), `suggestions ${kinds}`);
r = await api("/api/script/suggest", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ script_id: sid }) });
j = await r.json();
ok(r.status === 200 && j.suggestions.some((s) => s.block === 1 && s.kind === "set") && !j.suggestions.some((s) => s.block === 3), "suggest by script_id (heading yes, already-marked kind no)");
r = await api("/api/script/suggest", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ script_id: 999 }) });
ok(r.status === 404, "unknown script_id → 404");

console.log("undo / remove");
await cmd({ type: "script_remove", id: sid });
ok(await until(() => st.show.scripts.length === 0 && !rts(), "removed"), "script_remove clears show + runtime");
await cmd({ type: "undo" });
ok(await until(() => st.show.scripts.length === 1, "undo"), "undo restores the script");

udp.close(); srv.close();
console.log(failed ? `\nFAILED: ${failed}` : "\nALL OK");
process.exit(failed ? 1 : 0);
