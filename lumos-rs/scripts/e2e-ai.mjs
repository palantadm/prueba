// End-to-end test of the AI assistant: boots the Lumos binary and scripts/mock-anthropic.mjs (no real API is contacted),
// then drives the assistant over WebSocket with both wire formats.
// Usage: node scripts/e2e-ai.mjs [path-to-binary] [path-to-mock]   (Node ≥ 22)
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, statSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const bin = process.argv[2] ?? "target/debug/lumos";
const mockPath = process.argv[3] ?? join(here, "mock-anthropic.mjs");
const data = mkdtempSync(join(tmpdir(), "lumos-e2e-ai-"));
const KEY_A = "sk-ant-e2e-SECRET-111";
const KEY_O = "sk-oai-e2e-SECRET-222";

let failed = 0;
const ok = (c, m) => { if (!c) { failed++; console.log("  FAIL", m); } else console.log("  ok  ", m); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
const cleanup = () => procs.forEach((p) => p.kill("SIGTERM"));
process.on("exit", cleanup);

const start = (cmd, args, re) => new Promise((res, rej) => {
  const p = spawn(cmd, args, { stdio: ["ignore", "pipe", "inherit"] });
  procs.push(p);
  let buf = "";
  p.stdout.on("data", (d) => { buf += d; const m = buf.match(re); if (m) res(m); });
  setTimeout(() => rej(new Error(`${cmd} did not start`)), 15000);
});

const [, mock] = await start("node", [mockPath], /MOCK (http:\/\/127\.0\.0\.1:\d+)/);
const [, port] = await start(bin, ["--port", "0", "--bind", "127.0.0.1", "--data", data], /http:\/\/127\.0\.0\.1:(\d+)/);
const base = `127.0.0.1:${port}`;

const ws = new WebSocket(`ws://${base}/ws`);
const raw = [];
const events = [];
let config = null;
let test = null;
let show = null;
let seq = 1;
const acks = new Map();
const waiters = [];
ws.onmessage = (e) => {
  if (typeof e.data !== "string") return;
  raw.push(e.data);
  const m = JSON.parse(e.data);
  if (m.t === "ai_event") events.push({ session: m.session, ...m.ev });
  else if (m.t === "ai_config") config = m.cfg;
  else if (m.t === "ai_test") test = m;
  else if (m.t === "show") show = m.show;
  else if (m.t === "ack") { acks.get(m.id)?.(m); acks.delete(m.id); }
  waiters.forEach((w) => w());
};
const until = async (pred, what, ms = 10000) => {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (pred()) return true; await new Promise((r) => { waiters.push(r); setTimeout(r, 50); }); }
  console.log("  timeout waiting for", what);
  return false;
};
const cmd = (c) => new Promise((res) => {
  const id = String(seq++); acks.set(id, res); ws.send(JSON.stringify({ t: "cmd", id, cmd: c }));
  setTimeout(() => { if (acks.delete(id)) res({ ok: false, err: "timeout" }); }, 8000);
});
await new Promise((r) => (ws.onopen = r));
await until(() => show && config, "snapshot with ai_config");

console.log("not configured by default");
ok(config.providers.length === 0 && config.active_provider === "", "no provider configured out of the box");
ok(config.presets.some((p) => p.id === "anthropic") && config.presets.some((p) => p.id === "ollama" && !p.needs_key), "presets include Claude and a keyless local server");
let n0 = events.length;
await cmd({ type: "ai_send", session: "s0", text: "hola", images: [], context: { workspace: "live", selection: [], lang: "es" } });
ok(await until(() => events.slice(n0).some((e) => e.session === "s0" && e.type === "error" && /no está configurado/.test(e.msg)), "error"), "unconfigured assistant explains how to set up");

console.log("configure providers");
const prov = (id, kind, key) => ({ type: "ai_configure", cfg: { provider: { id, name: "", kind, base_url: mock, api_key: key, models: null }, remove_provider: null, active_provider: id, model: null, enabled: null, effort: null, mode: "auto" } });
let r = await cmd(prov("anthropic", "anthropic", KEY_A));
ok(r.ok, "configure anthropic");
r = await cmd(prov("openai", "openai_compat", KEY_O));
ok(r.ok, "configure openai-compatible");
await cmd({ type: "ai_configure", cfg: { provider: null, remove_provider: null, active_provider: "anthropic", model: null, enabled: null, effort: null, mode: "auto" } });
await until(() => config.active_provider === "anthropic" && config.providers.length === 2, "config pushed");
ok(config.providers.every((p) => p.has_key), "has_key reported");
ok(!raw.some((m) => m.includes(KEY_A) || m.includes(KEY_O)), "keys are never sent to clients");
const cfgFile = join(data, "ai.json");
ok((statSync(cfgFile).mode & 0o777) === 0o600, "ai.json is 0600");
ok(readFileSync(cfgFile, "utf8").includes(KEY_A), "key stored in ai.json");

console.log("test connection + model list");
test = null;
await cmd({ type: "ai_test_provider", provider: "openai" });
ok(await until(() => test?.provider === "openai", "ai_test") && test.ok, `connection test ok (${test?.message})`);
await cmd({ type: "ai_list_models", provider: "openai" });
ok(await until(() => config.providers.find((p) => p.id === "openai").models.some((m) => m.id === "mock-chat-1"), "models listed"), "model list refreshed from /models");
ok(!config.providers.find((p) => p.id === "openai").models.some((m) => m.id.includes("embedding")), "embedding models filtered out");

console.log("patch + scene turn (anthropic wire)");
await cmd({ type: "fixture_add", profile: "generic/rgb-par", mode: "3ch", count: 4, universe: 1, address: null, name: null });
const turn = async (session, text, extra) => {
  const from = events.length;
  r = await cmd({ type: "ai_send", session, text, images: [], context: { workspace: "live", selection: [1, 2], lang: "es" } });
  if (!r.ok) return { ack: r, evs: [] };
  await until(() => events.slice(from).some((e) => e.session === session && ["done", "error"].includes(e.type) || (extra && extra(events.slice(from)))), `${session} finished`);
  return { ack: r, evs: events.slice(from).filter((e) => e.session === session) };
};
let t = await turn("a", "Crea una escena roja [demo:scene]");
ok(t.evs[0]?.type === "started", "stream starts");
ok(t.evs.some((e) => e.type === "text" && e.delta.includes("Hecho")), "final text streamed");
ok(t.evs.some((e) => e.type === "tool" && e.state === "done" && e.name === "console_command"), "tool executed");
ok(t.evs.at(-1).type === "done" && t.evs.at(-1).changes >= 1, `done with changes=${t.evs.at(-1).changes}`);
ok(await until(() => show.scenes.some((s) => s.name === "Rojo"), "scene in show"), "scene «Rojo» recorded");
await cmd({ type: "ai_undo", session: "a" });
ok(await until(() => !show.scenes.some((s) => s.name === "Rojo"), "undo"), "assistant changes undone");

console.log("same flow on the OpenAI-compatible wire, after switching provider mid-session");
await cmd({ type: "ai_configure", cfg: { provider: null, remove_provider: null, active_provider: "openai", model: null, enabled: null, effort: null, mode: null } });
await until(() => config.active_provider === "openai", "switched");
t = await turn("a", "otra vez [demo:scene]");
ok(t.evs.at(-1).type === "done", "turn completes on the other provider");
ok(await until(() => show.scenes.some((s) => s.name === "Rojo"), "scene"), "scene recorded via OpenAI wire");
const reqs = await (await fetch(`${mock}/__requests`)).json();
const lastOpenAi = reqs.filter((q) => q.wire === "openai").at(-1);
ok(JSON.stringify(lastOpenAi.body).includes("Voy a crear la escena"), "neutral history carried over the provider switch");
ok(!JSON.stringify(lastOpenAi.body).includes("mock-signature"), "provider-specific thinking dropped");

console.log("confirmation flow (smart mode)");
await cmd({ type: "ai_configure", cfg: { provider: null, remove_provider: null, active_provider: "anthropic", model: null, enabled: null, effort: null, mode: "smart" } });
await until(() => config.active_provider === "anthropic" && config.mode === "smart", "smart mode");
let from = events.length;
await cmd({ type: "ai_send", session: "c", text: "[demo:confirm]", images: [], context: { workspace: "live", selection: [], lang: "es" } });
ok(await until(() => events.slice(from).some((e) => e.session === "c" && e.type === "confirm"), "confirm"), "live action asks for confirmation");
const conf = events.slice(from).find((e) => e.session === "c" && e.type === "confirm");
ok(conf.risk === "live", "risk reported as live");
await cmd({ type: "ai_confirm", session: "c", call: conf.call, approve: false });
ok(await until(() => events.slice(from).some((e) => e.session === "c" && e.type === "tool" && e.state === "denied"), "denied"), "denial is honoured");
await until(() => events.slice(from).some((e) => e.session === "c" && e.type === "done"), "done");

console.log("ask_user + cancel");
from = events.length;
await cmd({ type: "ai_send", session: "q", text: "[demo:ask]", images: [], context: { workspace: "live", selection: [], lang: "es" } });
ok(await until(() => events.slice(from).some((e) => e.type === "ask"), "ask"), "question with options");
const ask = events.slice(from).find((e) => e.type === "ask");
await cmd({ type: "ai_answer", session: "q", call: ask.call, answer: "Azul" });
ok(await until(() => events.slice(from).some((e) => e.type === "done"), "done after answer"), "turn continues after the answer");

console.log("refusal and provider errors");
t = await turn("r", "[demo:refusal]");
ok(t.evs.at(-1).type === "error" && /rechazado/.test(t.evs.at(-1).msg), "safety refusal reported");
t = await turn("e", "[demo:error]");
ok(t.evs.at(-1).type === "error" && /error de prueba/.test(t.evs.at(-1).msg), "provider error reported with its message");

console.log("keys never leave the machine");
ok(!raw.some((m) => m.includes(KEY_A) || m.includes(KEY_O)), "no key in any message sent to the client");
const files = readdirSync(data).filter((f) => f !== "ai.json");
ok(files.every((f) => { try { return !readFileSync(join(data, f), "utf8").includes("SECRET"); } catch { return true; } }), "no key in show/autosave files");
ok(!JSON.stringify(reqs).includes("SECRET"), "keys are not part of any prompt/body");

ws.close();
console.log(failed ? `\n${failed} FAILED` : "\nall ok");
process.exit(failed ? 1 : 0);
