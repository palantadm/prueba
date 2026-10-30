import "../styles/ai.css";
import type { ComponentChildren } from "preact";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import { toast } from "../store";
import { Icon, ICONS } from "../ui/Icon";
import { Modal } from "../ui/Modal";
import { t, tp } from "../i18n";
import type { AiMode, AiRisk, AiToolState } from "../types";
import { Markdown } from "./markdown";
import { fileToThumb, isImage, MAX_IMAGES } from "./media";
import { canDictate, useDictation } from "./voice";
import {
  activeModel, activeProvider, aiAnswer, aiCancel, aiConfig, aiConfirm, aiReady, aiReset, aiSend, aiUndo, aiWorkspace, askAssistant, busy,
  canSpeak, changes, draft, focusInput, items, openAiSettings, panelOpen, quickOpen, setMode, setSpeak, speakReplies, statusLine, suggestions,
  usage,
} from "./state";
import type { Item, Thumb } from "./state";

const RISK_ICON: Record<AiRisk, string> = {
  read: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 15a3 3 0 100-6 3 3 0 000 6z",
  edit: "M4 20h4L19 9l-4-4L4 16v4zM13 7l4 4",
  live: ICONS.live,
};
const riskLabel = (r: AiRisk) => (r === "read" ? t("Reads data") : r === "edit" ? t("Edits the show") : t("Acts on live output"));
const SEND = "M5 12l14-7-5 14-2-6-7-1z";
const MIC = "M12 3a3 3 0 00-3 3v6a3 3 0 006 0V6a3 3 0 00-3-3zM6 11a6 6 0 0012 0M12 17v4";
const CLIP = "M20 11l-8.5 8.5a5 5 0 01-7-7L13 4a3.5 3.5 0 015 5l-8.5 8.5a2 2 0 01-3-3L14 7";
const CAM = "M4 8h3l2-3h6l2 3h3v11H4zM12 17a4 4 0 100-8 4 4 0 000 8z";
const SPARK = "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z";
const SPEAK = "M4 9v6h4l5 4V5L8 9zM16 9a4 4 0 010 6M18.5 6.5a8 8 0 010 11";
const NEW = "M12 5v14M5 12h14";

export const ASSISTANT_ICON = SPARK;

const MODES: { id: AiMode; title: () => string; desc: () => string }[] = [
  { id: "ask", title: () => t("Always ask"), desc: () => t("The assistant reads freely but asks before it changes anything, in the show or live.") },
  { id: "smart", title: () => t("Smart"), desc: () => t("It reads and edits the show on its own, and asks before anything that changes the live output.") },
  { id: "auto", title: () => t("Automatic"), desc: () => t("It does everything without asking, live output included. You can still undo its changes.") },
];
const modeName = (m: AiMode) => MODES.find((x) => x.id === m)?.title() ?? m;

// ── cards ───────────────────────────────────────────────

function StateChip({ s }: { s: AiToolState }) {
  if (s === "running") return <span class="chip ai-st"><span class="ai-spin" aria-hidden="true" />{t("Running…")}</span>;
  if (s === "done") return <span class="chip ok ai-st">{t("Done")}</span>;
  if (s === "denied") return <span class="chip warn ai-st">{t("Denied")}</span>;
  return <span class="chip err ai-st">{t("Error")}</span>;
}

function RiskBadge({ risk }: { risk: AiRisk }) {
  return <span class={`ai-risk risk-${risk}`}><Icon d={RISK_ICON[risk]} size={16} />{riskLabel(risk)}</span>;
}

function Expand({ label, children }: { label: string; children: ComponentChildren }) {
  const [open, setOpen] = useState(false);
  return (
    <div class="ai-exp">
      <button class="btn sm ghost" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? "▾" : "▸"} {label}</button>
      {open && children}
    </div>
  );
}

function ToolCard({ it }: { it: Extract<Item, { k: "tool" }> }) {
  return (
    <div class={`ai-card risk-${it.risk}`}>
      <div class="row ai-card-head">
        <span class="ai-ico" title={riskLabel(it.risk)}><Icon d={RISK_ICON[it.risk]} size={18} /><span class="sr">{riskLabel(it.risk)}</span></span>
        <span class="grow ai-sum">{it.summary || it.name.replace(/_/g, " ")}</span>
        <StateChip s={it.state} />
      </div>
      {it.result && <Expand label={t("Result")}><pre class="ai-pre">{it.result}</pre></Expand>}
    </div>
  );
}

function ConfirmCard({ it }: { it: Extract<Item, { k: "confirm" }> }) {
  if (it.resolved) {
    return (
      <div class={`ai-card risk-${it.risk} resolved`}>
        <div class="row ai-card-head">
          <span class="ai-ico"><Icon d={RISK_ICON[it.risk]} size={18} /></span>
          <span class="grow ai-sum">{it.summary}</span>
          <span class={`chip ${it.resolved === "yes" ? "ok" : "warn"} ai-st`}>{it.resolved === "yes" ? t("Allowed") : t("Denied")}</span>
        </div>
      </div>
    );
  }
  return (
    <div class={`ai-card confirm risk-${it.risk}`} role="group" aria-label={t("The assistant asks permission")}>
      <div class="ai-confirm-title">{t("The assistant wants to:")}</div>
      <div class="ai-sum big">{it.summary}</div>
      <RiskBadge risk={it.risk} />
      {it.args && <Expand label={t("Details")}><pre class="ai-pre">{it.args}</pre></Expand>}
      <div class="ai-confirm-btns">
        <button class="btn go lg" onClick={() => aiConfirm(it.call, true)}>{t("Allow")}</button>
        <button class="btn danger lg" onClick={() => aiConfirm(it.call, false)}>{t("Deny")}</button>
      </div>
      <button class="btn tall ghost" onClick={() => aiConfirm(it.call, true, true)}>{t("Allow everything this turn")}</button>
    </div>
  );
}

function AskCard({ it }: { it: Extract<Item, { k: "ask" }> }) {
  const [v, setV] = useState("");
  if (it.answered !== undefined) {
    return (
      <div class="ai-card resolved">
        <div class="ai-sum">{it.question}</div>
        <div class="dim">{t("You answered:")} <b>{it.answered}</b></div>
      </div>
    );
  }
  const go = (a: string) => a.trim() && aiAnswer(it.call, a.trim());
  return (
    <div class="ai-card ask" role="group" aria-label={t("The assistant asks you")}>
      <div class="ai-sum big">{it.question}</div>
      {it.options.length > 0 && (
        <div class="row wrap ai-chips">{it.options.map((o) => <button key={o} class="btn tall" onClick={() => go(o)}>{o}</button>)}</div>
      )}
      <div class="row">
        <input class="input grow" value={v} placeholder={t("Or type your answer…")} aria-label={t("Your answer")}
          onInput={(e) => setV((e.target as HTMLInputElement).value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.isComposing) go(v); }} />
        <button class="btn tall primary" disabled={!v.trim()} onClick={() => go(v)}>{t("Reply")}</button>
      </div>
    </div>
  );
}

function Entry({ it }: { it: Item }) {
  switch (it.k) {
    case "user":
      return (
        <div class="ai-user">
          {it.thumbs.length > 0 && <div class="ai-thumbs">{it.thumbs.map((u, i) => <img key={i} src={u} alt={t("Attached image")} />)}</div>}
          {it.text && <div class="ai-bubble">{it.text}</div>}
        </div>
      );
    case "ai": return <div class="ai-msg"><Markdown text={it.text} streaming={it.streaming} /></div>;
    case "tool": return <ToolCard it={it} />;
    case "confirm": return <ConfirmCard it={it} />;
    case "ask": return <AskCard it={it} />;
    case "error": return <div class="ai-error" role="alert">{it.msg}</div>;
    case "info": return <div class="ai-info">{it.text}</div>;
  }
}

// ── onboarding / empty ──────────────────────────────────

function Onboarding() {
  const c = aiConfig.value;
  return (
    <div class="ai-onboard">
      <div class="ai-onboard-ico" aria-hidden="true"><Icon d={SPARK} size={34} /></div>
      <h3>{c && !c.enabled && c.providers.length ? t("The assistant is turned off") : t("Set up the assistant")}</h3>
      <p class="dim">{t("Choose a provider (Anthropic, OpenAI, Gemini, Ollama… any model works) and paste its key. Then you can ask questions and let it operate Lumos for you.")}</p>
      <button class="btn lg primary" onClick={openAiSettings}>{t("Choose a provider")}</button>
      <p class="faint ai-small">{t("With Ollama or LM Studio nothing leaves your computer.")}</p>
    </div>
  );
}

function Welcome() {
  const ws = aiWorkspace.value;
  return (
    <div class="ai-welcome">
      <h3>{t("What do you want to do?")}</h3>
      <div class="col ai-sugg">
        {suggestions(ws).map((s) => <button key={s} class="btn tall" onClick={() => void aiSend(s)}>{s}</button>)}
      </div>
    </div>
  );
}

// ── transcript ──────────────────────────────────────────

function Transcript() {
  const el = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const [away, setAway] = useState(false);
  const arr = items.value;
  const status = statusLine.value;
  const run = busy.value;

  useLayoutEffect(() => {
    const e = el.current;
    if (e && stick.current) e.scrollTop = e.scrollHeight;
  }, [arr, status, run]);

  const onScroll = () => {
    const e = el.current;
    if (!e) return;
    const near = e.scrollHeight - e.scrollTop - e.clientHeight < 56;
    stick.current = near;
    setAway(!near);
  };
  const toEnd = () => { const e = el.current; if (e) { stick.current = true; e.scrollTo({ top: e.scrollHeight, behavior: "smooth" }); setAway(false); } };

  const streaming = arr.length > 0 && arr[arr.length - 1].k === "ai" && (arr[arr.length - 1] as Extract<Item, { k: "ai" }>).streaming;
  return (
    <div class="ai-scroll-wrap">
      <div class="ai-scroll" ref={el} onScroll={onScroll} role="log" aria-live="polite" aria-label={t("Conversation")}>
        {!aiReady.value ? <Onboarding /> : arr.length === 0 ? <Welcome /> : arr.map((it) => <Entry key={it.id} it={it} />)}
        {run && !streaming && <div class="ai-status"><span class="ai-spin" aria-hidden="true" />{status || t("Thinking…")}</div>}
        {run && streaming && status && <div class="ai-status">{status}</div>}
      </div>
      {away && <button class="btn sm ai-jump" onClick={toEnd}>↓ {t("Jump to end")}</button>}
    </div>
  );
}

// ── composer ────────────────────────────────────────────

function CameraSheet({ onShot, onClose }: { onShot: (f: File) => void; onClose: () => void }) {
  const vid = useRef<HTMLVideoElement>(null);
  const [err, setErr] = useState("");
  useEffect(() => {
    let stream: MediaStream | undefined;
    let dead = false;
    navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment" }, audio: false })
      .then((s) => { if (dead) { s.getTracks().forEach((x) => x.stop()); return; } stream = s; if (vid.current) { vid.current.srcObject = s; void vid.current.play(); } })
      .catch(() => setErr(t("Could not open the camera. Check the browser permission.")));
    return () => { dead = true; stream?.getTracks().forEach((x) => x.stop()); };
  }, []);
  const shot = () => {
    const v = vid.current;
    if (!v || !v.videoWidth) return;
    const c = document.createElement("canvas");
    c.width = v.videoWidth;
    c.height = v.videoHeight;
    c.getContext("2d")?.drawImage(v, 0, 0);
    c.toBlob((b) => { if (b) { onShot(new File([b], "camera.jpg", { type: "image/jpeg" })); onClose(); } }, "image/jpeg", 0.92);
  };
  return (
    <Modal title={t("Take a photo")} onClose={onClose} footer={<><button class="btn tall" onClick={onClose}>{t("Cancel")}</button><button class="btn tall primary" disabled={!!err} onClick={shot}>{t("Capture")}</button></>}>
      {err ? <div class="ai-error">{err}</div> : <video ref={vid} class="ai-video" playsInline muted />}
    </Modal>
  );
}

function Composer() {
  const ta = useRef<HTMLTextAreaElement>(null);
  const fileIn = useRef<HTMLInputElement>(null);
  const camIn = useRef<HTMLInputElement>(null);
  const [thumbs, setThumbs] = useState<Thumb[]>([]);
  const [interim, setInterim] = useState("");
  const [cam, setCam] = useState(false);
  const base = useRef("");
  const vision = !!activeModel.value?.vision;
  const run = busy.value;
  const ready = aiReady.value;

  const dict = useDictation(
    (fin, inter) => { draft.value = (base.current && fin ? base.current + " " : base.current) + fin; setInterim(inter); },
    () => setInterim(""),
    (code) => toast(code === "not-allowed" || code === "service-not-allowed" ? t("Microphone permission denied.") : t("Voice dictation failed ({e}).", { e: code }), "err"),
  );

  // auto-grow up to ~6 lines (no layout jump: height is only recomputed from content)
  const text = draft.value;
  useLayoutEffect(() => {
    const e = ta.current;
    if (!e) return;
    e.style.height = "auto";
    e.style.height = `${Math.min(e.scrollHeight, 150)}px`;
  }, [text, interim]);
  useEffect(() => { ta.current?.focus(); }, [focusInput.value]);

  const addFiles = async (files: File[]) => {
    const imgs = files.filter(isImage);
    if (!imgs.length) return;
    if (!vision) { toast(t("The active model does not accept images. Pick one with vision in Settings → Assistant."), "err", 5000); return; }
    const room = MAX_IMAGES - thumbs.length;
    if (room <= 0) { toast(t("Up to {n} images per message.", { n: MAX_IMAGES }), "err"); return; }
    const out: Thumb[] = [];
    for (const f of imgs.slice(0, room)) {
      try { out.push(await fileToThumb(f)); } catch { toast(t("Could not read the image."), "err"); }
    }
    if (out.length) setThumbs((a) => [...a, ...out].slice(0, MAX_IMAGES));
  };
  // drag & drop is wired by the panel through this window event
  useEffect(() => {
    const h = (e: Event) => void addFiles((e as CustomEvent<File[]>).detail);
    window.addEventListener("lumos-ai-files", h);
    return () => window.removeEventListener("lumos-ai-files", h);
  });

  const submit = () => {
    if (run) return;
    if (!ready) { openAiSettings(); return; }
    const txt = draft.value;
    if (!txt.trim() && !thumbs.length) return;
    draft.value = "";
    const imgs = thumbs;
    setThumbs([]);
    void aiSend(txt, imgs);
  };

  const pttDown = (e: PointerEvent) => { e.preventDefault(); base.current = draft.value.trimEnd(); dict.start(); };
  const pttKey = () => { if (dict.on) dict.stop(); else { base.current = draft.value.trimEnd(); dict.start(); } };

  const shown = interim ? `${text}${text ? " " : ""}${interim}` : text;
  return (
    <div class="ai-composer">
      {thumbs.length > 0 && (
        <div class="ai-thumbs edit">
          {thumbs.map((th, i) => (
            <div key={i} class="ai-thumb">
              <img src={th.url} alt={t("Attached image")} />
              <button class="ai-x" aria-label={t("Remove image")} onClick={() => setThumbs(thumbs.filter((_, j) => j !== i))}>✕</button>
            </div>
          ))}
        </div>
      )}
      <div class="ai-box">
        <textarea ref={ta} class="ai-input" rows={1} value={shown} readOnly={dict.on} aria-label={t("Message to the assistant")}
          placeholder={ready ? t("Ask or tell the assistant what to do…") : t("Set up a provider first…")}
          onInput={(e) => { draft.value = (e.target as HTMLTextAreaElement).value; }}
          onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); submit(); } }}
          onPaste={(e) => {
            const fs = [...(e.clipboardData?.files ?? [])].filter(isImage);
            if (fs.length) { e.preventDefault(); void addFiles(fs); }
          }} />
        <div class="row ai-tools">
          <button class={`btn icon ghost ${vision ? "" : "off"}`} aria-label={t("Attach image")} title={t("Attach image")}
            onClick={() => (vision ? fileIn.current?.click() : toast(t("The active model does not accept images. Pick one with vision in Settings → Assistant."), "err", 5000))}>
            <Icon d={CLIP} size={20} />
          </button>
          <button class={`btn icon ghost ${vision ? "" : "off"}`} aria-label={t("Take a photo")} title={t("Take a photo")}
            onClick={() => {
              if (!vision) { toast(t("The active model does not accept images. Pick one with vision in Settings → Assistant."), "err", 5000); return; }
              if (typeof navigator.mediaDevices?.getUserMedia === "function") setCam(true); else camIn.current?.click();
            }}>
            <Icon d={CAM} size={20} />
          </button>
          {canDictate && (
            <button class={`btn icon ghost ${dict.on ? "rec" : ""}`} aria-pressed={dict.on} aria-label={t("Dictate (hold to talk)")} title={t("Dictate (hold to talk)")}
              onPointerDown={pttDown} onPointerUp={dict.stop} onPointerLeave={() => dict.on && dict.stop()} onPointerCancel={dict.stop}
              onClick={(e) => { if (e.detail === 0) pttKey(); }}>
              <Icon d={MIC} size={20} />
            </button>
          )}
          <span class="grow" />
          {run
            ? <button class="btn tall danger" onClick={aiCancel}>■ {t("Stop")}</button>
            : <button class="btn tall primary" aria-label={t("Send")} disabled={!shown.trim() && !thumbs.length} onClick={submit}><Icon d={SEND} size={20} /><span class="hide-xs">{t("Send")}</span></button>}
        </div>
      </div>
      <input ref={fileIn} type="file" accept="image/*" multiple hidden onChange={(e) => { const i = e.target as HTMLInputElement; void addFiles([...(i.files ?? [])]); i.value = ""; }} />
      <input ref={camIn} type="file" accept="image/*" capture="environment" hidden onChange={(e) => { const i = e.target as HTMLInputElement; void addFiles([...(i.files ?? [])]); i.value = ""; }} />
      {cam && <CameraSheet onShot={(f) => void addFiles([f])} onClose={() => setCam(false)} />}
    </div>
  );
}

// ── mode sheet ──────────────────────────────────────────

function ModeSheet({ onClose }: { onClose: () => void }) {
  const cur = aiConfig.value?.mode ?? "smart";
  return (
    <div class="ai-sheet-back" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div class="ai-sheet" role="dialog" aria-modal="true" aria-label={t("How much should it ask?")}>
        <div class="ai-sheet-head"><b class="grow">{t("How much should it ask?")}</b><button class="btn ghost icon" aria-label={t("Close")} onClick={onClose}>✕</button></div>
        <div class="col" role="radiogroup" aria-label={t("Safety mode")}>
          {MODES.map((m) => (
            <button key={m.id} role="radio" aria-checked={m.id === cur} class={`ai-mode ${m.id === cur ? "on" : ""}`} onClick={() => { void setMode(m.id); onClose(); }}>
              <span class="ai-mode-t">{m.title()}</span>
              <span class="dim">{m.desc()}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

// ── panel ───────────────────────────────────────────────

function PanelInner() {
  const root = useRef<HTMLElement>(null);
  const [sheet, setSheet] = useState(false);
  const [drop, setDrop] = useState(false);
  const prov = activeProvider.value;
  const model = activeModel.value;
  const n = changes.value;
  const u = usage.value;
  const c = aiConfig.value;

  useEffect(() => {
    const back = document.activeElement as HTMLElement | null;
    return () => { if (back && document.contains(back)) back.focus?.(); };
  }, []);

  const hasFiles = (e: DragEvent) => !!e.dataTransfer && [...e.dataTransfer.types].includes("Files");
  return (
    <aside ref={root} class="ai-panel" role="complementary" aria-label={t("Assistant")}
      onKeyDown={(e) => {
        if (e.key !== "Escape" || (e.target as HTMLElement).closest(".modal-back")) return;
        e.stopPropagation();
        if (sheet) setSheet(false); else panelOpen.value = false;
      }}
      onDragOver={(e) => { if (hasFiles(e)) { e.preventDefault(); setDrop(true); } }}
      onDragLeave={(e) => { if (e.target === e.currentTarget) setDrop(false); }}
      onDrop={(e) => {
        setDrop(false);
        if (!hasFiles(e)) return;
        e.preventDefault();
        window.dispatchEvent(new CustomEvent("lumos-ai-files", { detail: [...(e.dataTransfer?.files ?? [])] }));
      }}>
      <header class="ai-head">
        <div class="row">
          <span class="ai-title"><Icon d={SPARK} size={22} />{t("Assistant")}</span>
          <span class="grow" />
          {canSpeak && (
            <button class={`btn icon ghost ${speakReplies.value ? "on" : ""}`} aria-pressed={speakReplies.value} aria-label={t("Read replies aloud")} title={t("Read replies aloud")}
              onClick={() => setSpeak(!speakReplies.value)}><Icon d={SPEAK} size={20} /></button>
          )}
          <button class="btn tall ghost" onClick={aiReset} title={t("New conversation")}><Icon d={NEW} size={18} /><span class="hide-xs">{t("New conversation")}</span></button>
          <button class="btn icon ghost" aria-label={t("Close")} onClick={() => { panelOpen.value = false; }}>✕</button>
        </div>
        {c && aiReady.value && (
          <div class="row wrap ai-meta">
            <button class="chip ai-chip" onClick={openAiSettings} title={t("Change provider or model")}>{prov?.name}<span class="faint"> · </span>{model?.label || c.model}</button>
            {model && !model.tools && <span class="chip warn">{t("Chat only")}</span>}
            <button class="btn sm ai-modebtn" onClick={() => setSheet(true)} aria-haspopup="dialog">
              {t("Mode")}: <b>{modeName(c.mode)}</b> <span aria-hidden="true">ⓘ</span>
            </button>
          </div>
        )}
      </header>

      <Transcript />

      <footer class="ai-foot">
        {n > 0 && !busy.value && (
          <button class="btn tall ai-undo" onClick={() => void aiUndo()}>↶ {t("Undo the AI's changes ({n})", { n })}</button>
        )}
        <Composer />
        <div class="ai-meter faint">
          <span>{u.inp + u.out > 0 ? tp(u.inp + u.out, "{n} token", "{n} tokens") : t("Replies may be wrong. Check before going live.")}</span>
          {u.inp + u.out > 0 && <span class="mono">↑ {u.inp.toLocaleString()} · ↓ {u.out.toLocaleString()}</span>}
        </div>
      </footer>

      {drop && <div class="ai-drop" aria-hidden="true">{t("Drop an image here")}</div>}
      {sheet && <ModeSheet onClose={() => setSheet(false)} />}
    </aside>
  );
}

export function AiPanel() {
  return panelOpen.value ? <PanelInner /> : null;
}

// ── quick ask (Ctrl/Cmd+K) ──────────────────────────────

function QuickInner() {
  const [v, setV] = useState("");
  const close = () => { quickOpen.value = false; };
  const go = (s: string) => s.trim() && askAssistant(s.trim());
  return (
    <div class="modal-back ai-quick-back" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div class="ai-quick" role="dialog" aria-modal="true" aria-label={t("Ask the assistant")}
        onKeyDown={(e) => { if (e.key === "Escape") { e.stopPropagation(); close(); } }}>
        <div class="row">
          <Icon d={SPARK} size={22} />
          <input class="ai-quick-in grow" autofocus value={v} placeholder={t("Ask or tell the assistant what to do…")} aria-label={t("Ask the assistant")}
            onInput={(e) => setV((e.target as HTMLInputElement).value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.isComposing) go(v); }} />
          <button class="btn tall primary" disabled={!v.trim()} onClick={() => go(v)}>↵</button>
        </div>
        <div class="col ai-quick-sugg">
          {suggestions(aiWorkspace.value).slice(0, 3).map((s) => <button key={s} class="btn tall ghost" onClick={() => go(s)}>{s}</button>)}
        </div>
      </div>
    </div>
  );
}

export function QuickAsk() {
  return quickOpen.value ? <QuickInner /> : null;
}
