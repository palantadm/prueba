// Small, safe Markdown renderer. Builds Preact nodes directly (never innerHTML), so model output
// cannot inject markup. Supported: paragraphs, headings, lists, quotes, rules, tables, fenced and inline code,
// bold, italic, strike, links (http/https/mailto only).
import type { ComponentChildren } from "preact";
import { useMemo, useState } from "preact/hooks";
import { t } from "../i18n";

const SAFE_URL = /^(https?:\/\/|mailto:)/i;

function inline(src: string, key = "i"): ComponentChildren[] {
  const out: ComponentChildren[] = [];
  // order matters: code first, then links, bold, strike, italic, bare urls
  const re = /(`+)([^`]|[^`][\s\S]*?[^`])\1(?!`)|\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)|\*\*([^*]+?)\*\*|__([^_]+?)__|~~([^~]+?)~~|(?<![\w*])\*([^*\s][^*]*?)\*(?![\w*])|(?<![\w_])_([^_\s][^_]*?)_(?![\w_])|(https?:\/\/[^\s<>)]+[^\s<>).,;:!?])/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let n = 0;
  while ((m = re.exec(src))) {
    if (m.index > last) out.push(src.slice(last, m.index));
    const k = `${key}${n++}`;
    if (m[2] !== undefined) out.push(<code key={k} class="md-code">{m[2]}</code>);
    else if (m[3] !== undefined) {
      out.push(SAFE_URL.test(m[4])
        ? <a key={k} href={m[4]} target="_blank" rel="noopener noreferrer">{inline(m[3], k)}</a>
        : <span key={k}>{m[3]}</span>);
    }
    else if (m[5] !== undefined) out.push(<strong key={k}>{inline(m[5], k)}</strong>);
    else if (m[6] !== undefined) out.push(<strong key={k}>{inline(m[6], k)}</strong>);
    else if (m[7] !== undefined) out.push(<s key={k}>{inline(m[7], k)}</s>);
    else if (m[8] !== undefined) out.push(<em key={k}>{inline(m[8], k)}</em>);
    else if (m[9] !== undefined) out.push(<em key={k}>{inline(m[9], k)}</em>);
    else if (m[10] !== undefined) out.push(<a key={k} href={m[10]} target="_blank" rel="noopener noreferrer">{m[10]}</a>);
    last = m.index + m[0].length;
  }
  if (last < src.length) out.push(src.slice(last));
  return out;
}

function lines(src: string, key: string): ComponentChildren[] {
  const parts = src.split("\n");
  const out: ComponentChildren[] = [];
  parts.forEach((p, i) => {
    if (i) out.push(<br key={`${key}b${i}`} />);
    out.push(...inline(p, `${key}l${i}`));
  });
  return out;
}

function CodeBlock({ code, lang }: { code: string; lang: string }) {
  const [done, setDone] = useState(false);
  return (
    <div class="md-pre">
      <div class="md-pre-bar">
        <span class="mono">{lang || "text"}</span>
        <button class="btn sm ghost" onClick={() => { void navigator.clipboard?.writeText(code).then(() => { setDone(true); setTimeout(() => setDone(false), 1400); }); }}>
          {done ? t("Copied ✓") : t("Copy")}
        </button>
      </div>
      <pre><code>{code}</code></pre>
    </div>
  );
}

const isTableSep = (l: string) => /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(l);
const cells = (l: string) => l.trim().replace(/^\||\|$/g, "").split("|").map((c) => c.trim());

export function renderBlocks(text: string): ComponentChildren[] {
  const L = text.replace(/\r\n?/g, "\n").split("\n");
  const out: ComponentChildren[] = [];
  let i = 0;
  let k = 0;
  while (i < L.length) {
    const line = L[i];
    if (!line.trim()) { i++; continue; }
    const fence = /^\s*(```+|~~~+)\s*([\w+-]*)\s*$/.exec(line);
    if (fence) {
      const buf: string[] = [];
      i++;
      while (i < L.length && !new RegExp(`^\\s*${fence[1][0]}{${fence[1].length},}\\s*$`).test(L[i])) buf.push(L[i++]);
      i++; // closing fence (may be missing while streaming)
      out.push(<CodeBlock key={k++} code={buf.join("\n")} lang={fence[2]} />);
      continue;
    }
    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    if (h) { const lv = Math.min(4, h[1].length + 1); const Tag = (`h${lv}`) as "h2"; out.push(<Tag key={k++} class="md-h">{inline(h[2], `h${k}`)}</Tag>); i++; continue; }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) { out.push(<hr key={k++} />); i++; continue; }
    if (/^\s*>/.test(line)) {
      const buf: string[] = [];
      while (i < L.length && /^\s*>/.test(L[i])) buf.push(L[i++].replace(/^\s*>\s?/, ""));
      out.push(<blockquote key={k++}>{renderBlocks(buf.join("\n"))}</blockquote>);
      continue;
    }
    if (line.includes("|") && i + 1 < L.length && isTableSep(L[i + 1])) {
      const head = cells(line);
      i += 2;
      const rows: string[][] = [];
      while (i < L.length && L[i].includes("|") && L[i].trim()) rows.push(cells(L[i++]));
      out.push(
        <div key={k++} class="md-table"><table><thead><tr>{head.map((c, j) => <th key={j}>{inline(c, `th${j}`)}</th>)}</tr></thead>
          <tbody>{rows.map((r, a) => <tr key={a}>{r.map((c, j) => <td key={j}>{inline(c, `td${a}${j}`)}</td>)}</tr>)}</tbody></table></div>,
      );
      continue;
    }
    const ul = /^(\s*)[-*+]\s+(.*)$/.exec(line);
    const ol = /^(\s*)(\d+)[.)]\s+(.*)$/.exec(line);
    if (ul || ol) {
      const ordered = !!ol;
      const items: string[] = [];
      const re = ordered ? /^\s*\d+[.)]\s+(.*)$/ : /^\s*[-*+]\s+(.*)$/;
      while (i < L.length) {
        const m = re.exec(L[i]);
        if (m) { items.push(m[1]); i++; }
        else if (/^\s{2,}\S/.test(L[i]) && items.length) { items[items.length - 1] += "\n" + L[i].trim(); i++; }
        else break;
      }
      const Tag = ordered ? "ol" : "ul";
      out.push(<Tag key={k++}>{items.map((it, j) => <li key={j}>{lines(it, `li${k}${j}`)}</li>)}</Tag>);
      continue;
    }
    const buf: string[] = [];
    while (i < L.length && L[i].trim() && !/^\s*(```|~~~|#{1,4}\s|>|[-*+]\s|\d+[.)]\s)/.test(L[i]) && !(L[i].includes("|") && i + 1 < L.length && isTableSep(L[i + 1]))) buf.push(L[i++]);
    if (!buf.length) { buf.push(L[i++]); }
    out.push(<p key={k++}>{lines(buf.join("\n"), `p${k}`)}</p>);
  }
  return out;
}

export function Markdown({ text, streaming }: { text: string; streaming?: boolean }) {
  const blocks = useMemo(() => renderBlocks(text), [text]);
  return <div class={`md ${streaming ? "streaming" : ""}`}>{blocks}</div>;
}
