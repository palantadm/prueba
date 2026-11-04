// Gobo textures: real wheel images when the library provides them, procedural patterns otherwise.
// Textures are white-where-light-passes on black, square, shared across fixtures.
import * as THREE from "three";
import type { Capability } from "../types";
import { isOpenCap } from "../lib/stage-dmx";

const cache = new Map<string, THREE.Texture>();
const loader = new THREE.TextureLoader();

type Painter = (g: CanvasRenderingContext2D, s: number, rnd: () => number) => void;

const PATTERNS: Painter[] = [
  // dots
  (g, s) => { for (let y = 0; y < 5; y++) for (let x = 0; x < 5; x++) { g.beginPath(); g.arc(s * (0.12 + x * 0.19), s * (0.12 + y * 0.19), s * 0.055, 0, 7); g.fill(); } },
  // star burst
  (g, s) => { g.translate(s / 2, s / 2); for (let i = 0; i < 12; i++) { g.rotate(Math.PI / 6); g.beginPath(); g.moveTo(0, -s * 0.06); g.lineTo(s * 0.05, -s * 0.46); g.lineTo(-s * 0.05, -s * 0.46); g.fill(); } g.beginPath(); g.arc(0, 0, s * 0.1, 0, 7); g.fill(); },
  // leaf breakup
  (g, s, r) => { for (let i = 0; i < 26; i++) { g.save(); g.translate(s * (0.1 + r() * 0.8), s * (0.1 + r() * 0.8)); g.rotate(r() * 6.28); g.beginPath(); g.ellipse(0, 0, s * (0.03 + r() * 0.05), s * (0.09 + r() * 0.07), 0, 0, 7); g.fill(); g.restore(); } },
  // venetian lines
  (g, s) => { for (let i = 0; i < 7; i++) g.fillRect(0, s * (0.06 + i * 0.14), s, s * 0.065); },
  // ring + cross
  (g, s) => { g.lineWidth = s * 0.07; g.beginPath(); g.arc(s / 2, s / 2, s * 0.32, 0, 7); g.stroke(); g.fillRect(s * 0.46, s * 0.1, s * 0.08, s * 0.8); g.fillRect(s * 0.1, s * 0.46, s * 0.8, s * 0.08); },
  // triangles
  (g, s) => { for (let i = 0; i < 3; i++) { g.save(); g.translate(s / 2, s / 2); g.rotate((i * Math.PI * 2) / 3); g.beginPath(); g.moveTo(0, -s * 0.42); g.lineTo(s * 0.2, -s * 0.08); g.lineTo(-s * 0.2, -s * 0.08); g.fill(); g.restore(); } },
  // spiral
  (g, s) => { g.lineWidth = s * 0.06; g.beginPath(); for (let a = 0; a < 18; a += 0.15) { const rr = s * 0.02 * a + s * 0.03; g.lineTo(s / 2 + Math.cos(a) * rr, s / 2 + Math.sin(a) * rr); } g.stroke(); },
  // cloud breakup
  (g, s, r) => { for (let i = 0; i < 60; i++) { g.globalAlpha = 0.25 + r() * 0.5; g.beginPath(); g.arc(s * r(), s * r(), s * (0.03 + r() * 0.08), 0, 7); g.fill(); } g.globalAlpha = 1; },
  // grid / mesh
  (g, s) => { g.lineWidth = s * 0.035; for (let i = 1; i < 6; i++) { g.beginPath(); g.moveTo(s * (i / 6), 0); g.lineTo(s * (i / 6), s); g.moveTo(0, s * (i / 6)); g.lineTo(s, s * (i / 6)); g.stroke(); } },
  // half moon + dots
  (g, s) => { g.beginPath(); g.arc(s * 0.5, s * 0.5, s * 0.36, -1.2, 1.2 + Math.PI * 0.35); g.arc(s * 0.62, s * 0.5, s * 0.3, 1.2 + Math.PI * 0.35, -1.2, true); g.fill(); for (let i = 0; i < 5; i++) { g.beginPath(); g.arc(s * (0.2 + i * 0.15), s * 0.86, s * 0.03, 0, 7); g.fill(); } },
];

function hash(str: string): number {
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) { h ^= str.charCodeAt(i); h = Math.imul(h, 16777619); }
  return h >>> 0;
}

function proceduralGobo(label: string): THREE.Texture {
  const key = "proc:" + label.toLowerCase().replace(/\d+/g, "#");
  let t = cache.get(key);
  if (t) return t;
  const h = hash(label);
  const s = 256;
  const c = document.createElement("canvas");
  c.width = c.height = s;
  const g = c.getContext("2d")!;
  g.fillStyle = "#000"; g.fillRect(0, 0, s, s);
  g.fillStyle = "#fff"; g.strokeStyle = "#fff";
  let seed = h || 1;
  const rnd = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296; };
  g.save();
  PATTERNS[h % PATTERNS.length](g, s, rnd);
  g.restore();
  // keep the edge dark so the cookie never leaks outside the circle
  const edge = g.createRadialGradient(s / 2, s / 2, s * 0.38, s / 2, s / 2, s * 0.5);
  edge.addColorStop(0, "rgba(0,0,0,0)"); edge.addColorStop(1, "rgba(0,0,0,1)");
  g.fillStyle = edge; g.fillRect(0, 0, s, s);
  t = new THREE.CanvasTexture(c);
  t.colorSpace = THREE.NoColorSpace;
  t.anisotropy = 4;
  cache.set(key, t);
  return t;
}

/** Texture for a gobo capability, or null for open/none. */
export function goboTexture(cap: Capability | null, assets?: string): THREE.Texture | null {
  if (!cap || isOpenCap(cap)) return null;
  if (cap.image) {
    const url = cap.image.startsWith("/") || /^https?:/.test(cap.image) ? cap.image : (assets ?? "") + cap.image;
    let t = cache.get(url);
    if (!t) {
      t = loader.load(url);
      t.colorSpace = THREE.NoColorSpace;
      t.anisotropy = 4;
      cache.set(url, t);
    }
    return t;
  }
  return proceduralGobo(cap.label);
}

/** Texture from a wheel slot image (GDTF), relative to the model assets. */
export function slotTexture(image: string, assets: string): THREE.Texture {
  return goboTexture({ min: 0, max: 0, label: image, color: null, image, wheel: null, slot: null }, assets)!;
}

let white: THREE.Texture | null = null;
export function whiteTexture(): THREE.Texture {
  if (!white) { const d = new Uint8Array([255, 255, 255, 255]); white = new THREE.DataTexture(d, 1, 1); white.needsUpdate = true; }
  return white;
}
