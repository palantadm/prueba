import { useEffect, useRef, useState } from "preact/hooks";
import { send } from "../store";
import { t } from "../i18n";
import type { Script, ScriptRt } from "../types";
import { position, words } from "./util";

/* Experimental voice-follow: Web Speech API (es-ES), fuzzy-matches what is heard
   against the next ~40 words of the script and moves the reader with `script_goto`. */
type Rec = { lang: string; continuous: boolean; interimResults: boolean; start(): void; stop(): void; onresult: ((e: any) => void) | null; onend: (() => void) | null; onerror: ((e: any) => void) | null };
const Ctor = (): (new () => Rec) | undefined => (window as any).SpeechRecognition ?? (window as any).webkitSpeechRecognition;
export const voiceSupported = () => !!Ctor();

const norm = (s: string) => s.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[^a-z0-9ñ]+/g, " ").trim();

/** Word index (flat) of the best match for the heard phrase inside the window, or -1. */
export function matchHeard(heard: string[], win: string[]): number {
  const n = Math.min(4, heard.length);
  if (n < 2) return -1;
  const tail = heard.slice(-n);
  let best = -1, bestScore = 0;
  for (let i = 0; i + n <= win.length; i++) {
    let hit = 0;
    for (let k = 0; k < n; k++) if (win[i + k] === tail[k] || (tail[k].length > 3 && win[i + k].startsWith(tail[k].slice(0, 4)))) hit++;
    const score = hit / n;
    if (score > bestScore) { bestScore = score; best = i + n; }
  }
  return bestScore >= 0.66 ? best : -1;
}

export function useVoiceFollow(script: Script, rt: ScriptRt | undefined, enabled: boolean, fireCues: boolean) {
  const [heard, setHeard] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const live = useRef({ script, rt, fireCues });
  live.current = { script, rt, fireCues };
  useEffect(() => {
    const C = Ctor();
    if (!enabled || !C) return;
    const rec = new C();
    rec.lang = "es-ES"; rec.continuous = true; rec.interimResults = true;
    let stop = false;
    rec.onresult = (e: any) => {
      const r = e.results[e.results.length - 1];
      const txt = norm(String(r[0].transcript));
      setHeard(txt.split(" ").slice(-8).join(" "));
      const { script: s, rt: cur, fireCues: fire } = live.current;
      const pos = position(s, cur);
      const bi0 = Math.max(0, pos.bi);
      // window: the next ~40 words starting at the current block
      const win: string[] = []; const owner: number[] = [];
      for (let bi = bi0; bi < s.blocks.length && win.length < 40; bi++) {
        const b = s.blocks[bi];
        if (b.kind === "direction" || b.kind === "note") continue;
        for (const w of words(b.text)) { win.push(norm(w)); owner.push(bi); }
      }
      const hit = matchHeard(txt.split(" ").filter(Boolean), win);
      if (hit < 0) return;
      const target = owner[Math.min(hit, owner.length) - 1];
      if (target == null || target === pos.bi) return;
      const blk = s.blocks[target];
      // cues of skipped blocks fire only if enabled; otherwise just move the reader
      if (fire) {
        const nx = pos.next;
        if (nx && nx.bi <= target) void send({ type: "script_go", id: s.id });
      } else void send({ type: "script_goto", id: s.id, block: blk.id, mark: null });
    };
    rec.onerror = (e: any) => setErr(String(e?.error ?? "error"));
    rec.onend = () => { if (!stop) { try { rec.start(); } catch { /* already started */ } } };
    try { rec.start(); setErr(null); } catch (e) { setErr(String(e)); }
    return () => { stop = true; try { rec.stop(); } catch { /* noop */ } };
  }, [enabled, script.id]);
  return { heard, err };
}

export function VoiceBar(props: { heard: string; err: string | null; fireCues: boolean; onFireCues: (b: boolean) => void }) {
  return (
    <div class="sc-voice-bar" role="status">
      <span class="sc-voice on">● {t("Listening (experimental)")}</span>
      <span class="grow dim" style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{props.err ? `⚠ ${props.err}` : props.heard ? `“${props.heard}”` : t("Say the next line…")}</span>
      <button class="switch-row" role="switch" aria-checked={props.fireCues} onClick={() => props.onFireCues(!props.fireCues)}>
        <span class={`switch ${props.fireCues ? "on" : ""}`} />{t("Also fire the cues")}
      </button>
    </div>
  );
}
