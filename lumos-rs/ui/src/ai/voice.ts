// Voice dictation through the browser's Web Speech API (push-to-talk). Hidden by the UI when unsupported.
import { useEffect, useRef, useState } from "preact/hooks";
import { lang } from "../i18n";

type Rec = {
  lang: string; interimResults: boolean; continuous: boolean;
  start(): void; stop(): void; abort(): void;
  onresult: ((e: { resultIndex: number; results: ArrayLike<ArrayLike<{ transcript: string }> & { isFinal: boolean }> }) => void) | null;
  onend: (() => void) | null;
  onerror: ((e: { error: string }) => void) | null;
};
const Ctor = (globalThis as unknown as { SpeechRecognition?: new () => Rec; webkitSpeechRecognition?: new () => Rec }).SpeechRecognition
  ?? (globalThis as unknown as { webkitSpeechRecognition?: new () => Rec }).webkitSpeechRecognition;

export const canDictate = !!Ctor;

/** `onText(final, interim)` is called with the text recognised so far in this press. */
export function useDictation(onText: (final: string, interim: string) => void, onDone: () => void, onError: (code: string) => void) {
  const [on, setOn] = useState(false);
  const rec = useRef<Rec | null>(null);
  const cb = useRef({ onText, onDone, onError });
  cb.current = { onText, onDone, onError };

  const stop = () => { try { rec.current?.stop(); } catch { /* not started */ } };
  const start = () => {
    if (!Ctor || rec.current) return;
    const r = new Ctor();
    r.lang = lang.value === "es" ? "es-ES" : "en-US";
    r.interimResults = true;
    r.continuous = true;
    let fin = "";
    r.onresult = (e) => {
      let interim = "";
      for (let i = e.resultIndex; i < e.results.length; i++) {
        const tr = e.results[i][0].transcript;
        if (e.results[i].isFinal) fin += tr; else interim += tr;
      }
      cb.current.onText(fin, interim);
    };
    r.onerror = (e) => { if (e.error !== "no-speech" && e.error !== "aborted") cb.current.onError(e.error); };
    r.onend = () => { rec.current = null; setOn(false); cb.current.onDone(); };
    rec.current = r;
    setOn(true);
    try { r.start(); } catch { rec.current = null; setOn(false); }
  };
  useEffect(() => () => { try { rec.current?.abort(); } catch { /* gone */ } }, []);
  return { on, start, stop };
}
