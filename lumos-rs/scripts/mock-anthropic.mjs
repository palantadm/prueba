// Mock LLM server for tests: speaks the Anthropic Messages API (POST /v1/messages) AND the OpenAI Chat Completions
// wire format (POST /chat/completions, GET /models), both with SSE streaming. Never contacts any real service.
// Usage: node scripts/mock-anthropic.mjs [port]   → prints "MOCK http://127.0.0.1:<port>"
// Magic words in the last user message:
//   [demo:scene]    tool call: record a red scene through console_command, then a short summary
//   [demo:confirm]  tool call that needs confirmation in Smart mode (blackout on)
//   [demo:ask]      tool call ask_user with quick options
//   [demo:refusal]  safety refusal (Anthropic stop_reason "refusal" / OpenAI finish_reason "content_filter")
//   [demo:error]    HTTP 400 with an error body
// Anything else answers with a short greeting. `GET /__requests` returns the recorded request bodies (keys redacted).
import http from "node:http";

const port = Number(process.argv[2] ?? 0);
const requests = [];

const lastUserText = (body) => {
  const msgs = body.messages ?? [];
  for (let i = msgs.length - 1; i >= 0; i--) {
    const m = msgs[i];
    if (m.role !== "user") continue;
    if (typeof m.content === "string") return m.content;
    if (Array.isArray(m.content)) return m.content.map((p) => p.text ?? "").join("\n");
  }
  return "";
};
const endsWithToolResult = (wire, body) => {
  const last = (body.messages ?? []).at(-1);
  if (!last) return false;
  return wire === "openai" ? last.role === "tool" : Array.isArray(last.content) && last.content.some((b) => b.type === "tool_result");
};

const script = (wire, body) => {
  if (endsWithToolResult(wire, body)) return [{ text: "Hecho." }];
  const t = lastUserText(body);
  if (t.includes("[demo:scene]")) return [{ text: "Voy a crear la escena. " }, { tool: "console_command", args: { text: "fixture 1 thru 2 at 50; red at 100; record scene 1 Rojo; clear" } }];
  if (t.includes("[demo:confirm]")) return [{ tool: "console_command", args: { text: "blackout on" } }];
  if (t.includes("[demo:ask]")) return [{ tool: "ask_user", args: { question: "¿Qué color?", options: ["Rojo", "Azul"] } }];
  return [{ text: "Hola, soy el asistente de prueba." }];
};

const anthropicEvents = (parts, refusal) => {
  const ev = [["message_start", { type: "message_start", message: { id: "msg_mock", role: "assistant", usage: { input_tokens: 12, output_tokens: 1 } } }]];
  let idx = 0;
  ev.push(["content_block_start", { type: "content_block_start", index: idx, content_block: { type: "thinking", thinking: "" } }]);
  ev.push(["content_block_delta", { type: "content_block_delta", index: idx, delta: { type: "thinking_delta", thinking: "Pensando en la petición." } }]);
  ev.push(["content_block_delta", { type: "content_block_delta", index: idx, delta: { type: "signature_delta", signature: "mock-signature" } }]);
  ev.push(["content_block_stop", { type: "content_block_stop", index: idx++ }]);
  let tool = false;
  if (!refusal) {
    for (const p of parts) {
      if (p.text !== undefined) {
        ev.push(["content_block_start", { type: "content_block_start", index: idx, content_block: { type: "text", text: "" } }]);
        for (const w of p.text.match(/.{1,12}/gs) ?? []) ev.push(["content_block_delta", { type: "content_block_delta", index: idx, delta: { type: "text_delta", text: w } }]);
      } else {
        tool = true;
        ev.push(["content_block_start", { type: "content_block_start", index: idx, content_block: { type: "tool_use", id: `toolu_mock_${idx}`, name: p.tool, input: {} } }]);
        const j = JSON.stringify(p.args);
        const h = Math.ceil(j.length / 2);
        for (const c of [j.slice(0, h), j.slice(h)]) ev.push(["content_block_delta", { type: "content_block_delta", index: idx, delta: { type: "input_json_delta", partial_json: c } }]);
      }
      ev.push(["content_block_stop", { type: "content_block_stop", index: idx++ }]);
    }
  }
  ev.push(["message_delta", { type: "message_delta", delta: { stop_reason: refusal ? "refusal" : tool ? "tool_use" : "end_turn", ...(refusal ? { stop_details: { category: "mock" } } : {}) }, usage: { output_tokens: 9 } }]);
  ev.push(["message_stop", { type: "message_stop" }]);
  return ev.map(([e, d]) => `event: ${e}\ndata: ${JSON.stringify(d)}\n\n`).join("");
};

const openaiEvents = (parts, refusal) => {
  const out = [];
  let n = 0;
  let tool = false;
  if (!refusal) {
    for (const p of parts) {
      if (p.text !== undefined) {
        for (const w of p.text.match(/.{1,12}/gs) ?? []) out.push({ choices: [{ index: 0, delta: { role: "assistant", content: w } }] });
      } else {
        tool = true;
        const j = JSON.stringify(p.args);
        const h = Math.ceil(j.length / 2);
        out.push({ choices: [{ index: 0, delta: { tool_calls: [{ index: n, id: `call_mock_${n}`, type: "function", function: { name: p.tool, arguments: j.slice(0, h) } }] } }] });
        out.push({ choices: [{ index: 0, delta: { tool_calls: [{ index: n, function: { arguments: j.slice(h) } }] } }] });
        n++;
      }
    }
  }
  out.push({ choices: [{ index: 0, delta: {}, finish_reason: refusal ? "content_filter" : tool ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 6 } });
  return out.map((d) => `data: ${JSON.stringify(d)}\n\n`).join("") + "data: [DONE]\n\n";
};

const readBody = (req) => new Promise((res) => { let b = ""; req.on("data", (d) => (b += d)); req.on("end", () => res(b)); });

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");
  const json = (code, obj) => { res.writeHead(code, { "content-type": "application/json" }); res.end(JSON.stringify(obj)); };
  if (req.method === "GET" && url.pathname === "/__requests") return json(200, requests);
  if (req.method === "GET" && (url.pathname === "/models" || url.pathname === "/v1/models")) return json(200, { data: [{ id: "mock-chat-1" }, { id: "mock-chat-2" }, { id: "text-embedding-mock" }] });
  const wire = url.pathname === "/v1/messages" ? "anthropic" : url.pathname === "/chat/completions" ? "openai" : null;
  if (req.method !== "POST" || !wire) return json(404, { error: { message: "not found" } });
  const key = req.headers["x-api-key"] ?? (req.headers.authorization ?? "").replace(/^Bearer /, "");
  const body = JSON.parse((await readBody(req)) || "{}");
  requests.push({ wire, authenticated: Boolean(key), body });
  if (wire === "anthropic" && !key) return json(401, { type: "error", error: { type: "authentication_error", message: "missing x-api-key" } });
  const t = lastUserText(body);
  if (t.includes("[demo:error]")) return json(400, wire === "anthropic" ? { type: "error", error: { type: "invalid_request_error", message: "error de prueba" } } : { error: { message: "error de prueba" } });
  const refusal = t.includes("[demo:refusal]");
  res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache" });
  res.end(wire === "anthropic" ? anthropicEvents(script(wire, body), refusal) : openaiEvents(script(wire, body), refusal));
});

server.listen(port, "127.0.0.1", () => console.log(`MOCK http://127.0.0.1:${server.address().port}`));
