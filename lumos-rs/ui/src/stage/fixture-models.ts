// Detailed procedural PBR fixture models. Fixtures hang from the origin and emit along local -Y.
import * as THREE from "three";
import { RoundedBoxGeometry } from "three/examples/jsm/geometries/RoundedBoxGeometry.js";
import { mergeGeometries } from "three/examples/jsm/utils/BufferGeometryUtils.js";
import type { FixtureProfile } from "../types";
import type { Plan } from "../lib/stage-dmx";

export type FixtureKind =
  | "moving-spot" | "moving-wash" | "moving-beam" | "scanner"
  | "par" | "ledpar" | "fresnel" | "profile" | "bar" | "strobe" | "blinder" | "fogger" | "generic";

export interface FixtureModel {
  kind: FixtureKind;
  root: THREE.Group;
  /** rotates about Y (moving heads) */
  pan: THREE.Object3D | null;
  /** rotates about X */
  tilt: THREE.Object3D | null;
  /** beam origin; beam travels along local -Y of this node */
  lens: THREE.Object3D;
  lensRadius: number;
  /** emissive surfaces driven by the DMX colour */
  emissive: THREE.Mesh[];
  /** selection outline box (centre, size) in root space */
  selCenter: [number, number, number];
  selSize: [number, number, number];
  /** height of the label above the root */
  dispose(): void;
}

// ── shared materials (PBR) ───────────────────────────────
const mat = (color: string, rough: number, metal: number) => new THREE.MeshStandardMaterial({ color, roughness: rough, metalness: metal });
export const M = {
  black: mat("#15171b", 0.5, 0.55),
  dark: mat("#23262c", 0.45, 0.6),
  grey: mat("#3a3e46", 0.4, 0.65),
  alu: mat("#a9aeb6", 0.28, 1.0),
  chrome: mat("#d4d8de", 0.14, 1.0),
  rubber: mat("#0c0d0f", 0.9, 0.0),
  white: mat("#d9dce1", 0.5, 0.2),
  glass: new THREE.MeshPhysicalMaterial({ color: "#c8d8e8", roughness: 0.05, metalness: 0, transmission: 0, transparent: true, opacity: 0.35 }),
};

// ── shared geometry (built lazily) ───────────────────────
const gcache = new Map<string, THREE.BufferGeometry>();
function geo(key: string, make: () => THREE.BufferGeometry): THREE.BufferGeometry {
  let g = gcache.get(key);
  if (!g) { g = make(); gcache.set(key, g); }
  return g;
}

function lathe(points: [number, number][], seg = 40): THREE.BufferGeometry {
  return new THREE.LatheGeometry(points.map(([r, y]) => new THREE.Vector2(r, y)), seg);
}

function ring(r: number, h: number, y: number, seg = 40): THREE.BufferGeometry {
  const g = new THREE.CylinderGeometry(r, r, h, seg, 1, false);
  g.translate(0, y, 0);
  return g;
}

function merge(parts: THREE.BufferGeometry[]): THREE.BufferGeometry {
  const norm = parts.map((p) => (p.index ? p.toNonIndexed() : p));
  const g = mergeGeometries(norm, false)!;
  parts.forEach((p) => p.dispose());
  return g;
}

let dotsTex: THREE.CanvasTexture | null = null;
/** LED emitter cell pattern. */
function emitterTexture(): THREE.CanvasTexture {
  if (dotsTex) return dotsTex;
  const c = document.createElement("canvas");
  c.width = c.height = 128;
  const g = c.getContext("2d")!;
  g.fillStyle = "#1b1d22"; g.fillRect(0, 0, 128, 128);
  for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) {
    const gr = g.createRadialGradient(8 + x * 16, 8 + y * 16, 0, 8 + x * 16, 8 + y * 16, 7);
    gr.addColorStop(0, "#fff"); gr.addColorStop(0.7, "#d8d8d8"); gr.addColorStop(1, "#2a2d33");
    g.fillStyle = gr; g.beginPath(); g.arc(8 + x * 16, 8 + y * 16, 7, 0, 7); g.fill();
  }
  dotsTex = new THREE.CanvasTexture(c);
  dotsTex.colorSpace = THREE.SRGBColorSpace;
  return dotsTex;
}

function lensMaterial(textured = false): THREE.MeshBasicMaterial {
  return new THREE.MeshBasicMaterial({ color: "#ffffff", toneMapped: false, map: textured ? emitterTexture() : null });
}

// ── classification ───────────────────────────────────────
export function classify(prof: FixtureProfile | undefined, plan: Plan | null): FixtureKind {
  const id = `${prof?.id ?? ""} ${prof?.manufacturer ?? ""} ${prof?.model ?? ""}`.toLowerCase();
  const cat = prof?.category ?? "other";
  const has = (a: string) => !!plan?.channels.some((c) => c.attr === a);
  if (plan?.moving) {
    if (/scanner/.test(id)) return "scanner";
    if (cat === "wash" || (/wash/.test(id) && !/beam|spot/.test(id))) return "moving-wash";
    if (cat === "beam" || /beam/.test(id)) return "moving-beam";
    return "moving-spot";
  }
  if (cat === "bar" || /\b(bar|strip|batten|tube)\b/.test(id)) return "bar";
  if (cat === "strobe" || /strobe/.test(id)) return "strobe";
  if (/blinder/.test(id)) return "blinder";
  if (/fog|haze/.test(id)) return "fogger";
  if (/fresnel/.test(id)) return "fresnel";
  if (/profile|ellips|source ?four|leko/.test(id)) return "profile";
  if (cat === "par" || cat === "wash" || cat === "spot" || cat === "beam" || cat === "effect") {
    return has("red") || has("green") || has("blue") || /led/.test(id) ? "ledpar" : "par";
  }
  if (cat === "dimmer") return "par";
  return "generic";
}

// ── builders ─────────────────────────────────────────────
interface Ctx { root: THREE.Group; add(g: THREE.BufferGeometry, m: THREE.Material, parent?: THREE.Object3D, x?: number, y?: number, z?: number): THREE.Mesh; }

function lensDisc(r: number, y: number, parent: THREE.Object3D, lensMat: THREE.MeshBasicMaterial, emissive: THREE.Mesh[], textured = false): THREE.Mesh {
  const g = geo(`lensdisc${textured ? "t" : ""}`, () => { const c = new THREE.CircleGeometry(1, 40); c.rotateX(Math.PI / 2); return c; });
  const m = new THREE.Mesh(g, lensMat);
  m.scale.setScalar(r);
  m.position.y = y;
  parent.add(m);
  emissive.push(m);
  return m;
}

/** Hanging clamp + hook shared by most fixtures. */
function clamp(c: Ctx, y = 0) {
  const g = geo("clamp", () => merge([
    (() => { const b = new THREE.BoxGeometry(0.1, 0.012, 0.05); b.translate(0, 0.0, 0); return b; })(),
    (() => { const b = new THREE.CylinderGeometry(0.03, 0.03, 0.04, 16); b.rotateZ(Math.PI / 2); b.translate(0, 0.03, 0); return b; })(),
    (() => { const b = new THREE.CylinderGeometry(0.012, 0.012, 0.09, 8); b.translate(0, 0.05, 0); return b; })(),
  ]));
  c.add(g, M.alu, c.root, 0, y);
}

export function buildProcedural(kind: FixtureKind, prof: FixtureProfile | undefined): FixtureModel {
  const root = new THREE.Group();
  root.rotation.order = "YXZ";
  const meshes: THREE.Mesh[] = [];
  const emissive: THREE.Mesh[] = [];
  const lensMat = lensMaterial(kind === "ledpar" || kind === "bar" || kind === "strobe" || kind === "blinder");
  const own: THREE.Material[] = [lensMat];
  const add: Ctx["add"] = (g, m, parent = root, x = 0, y = 0, z = 0) => {
    const mesh = new THREE.Mesh(g, m);
    mesh.position.set(x, y, z);
    parent.add(mesh);
    meshes.push(mesh);
    return mesh;
  };
  const ctx: Ctx = { root, add };

  let pan: THREE.Object3D | null = null;
  let tilt: THREE.Object3D | null = null;
  let lens: THREE.Object3D = root;
  let lensR = 0.08;
  let selC: [number, number, number] = [0, -0.12, 0];
  let selS: [number, number, number] = [0.3, 0.3, 0.3];

  switch (kind) {
    case "moving-spot":
    case "moving-wash":
    case "moving-beam": {
      const wash = kind === "moving-wash";
      const beam = kind === "moving-beam";
      const R = wash ? 0.15 : beam ? 0.105 : 0.125;      // body radius
      const lr = wash ? 0.135 : beam ? 0.085 : 0.1;        // lens radius
      const len = wash ? 0.3 : 0.36;                         // head length
      const armX = R + 0.055;
      // base (rounded housing with fan vents)
      add(geo("mh-base", () => merge([
        new RoundedBoxGeometry(0.34, 0.1, 0.26, 4, 0.03).translate(0, -0.055, 0),
        ring(0.075, 0.04, -0.12, 24),
      ])), M.dark, root, 0, 0, 0);
      add(geo("mh-baseplate", () => new THREE.BoxGeometry(0.2, 0.012, 0.1)), M.alu, root, 0, 0.004, 0);
      clamp(ctx, 0.01);
      pan = new THREE.Group();
      pan.position.y = -0.105;
      root.add(pan);
      // yoke: two arms + bridge
      const yoke = geo(`mh-yoke-${R.toFixed(3)}`, () => merge([
        new RoundedBoxGeometry(0.05, 0.36, 0.11, 3, 0.02).translate(armX, -0.17, 0),
        new RoundedBoxGeometry(0.05, 0.36, 0.11, 3, 0.02).translate(-armX, -0.17, 0),
        new RoundedBoxGeometry(armX * 2 + 0.05, 0.05, 0.1, 3, 0.02).translate(0, 0.0, 0),
        (() => { const c = new THREE.CylinderGeometry(0.03, 0.03, 0.04, 20); c.rotateZ(Math.PI / 2); c.translate(armX + 0.045, -0.3, 0); return c; })(),
        (() => { const c = new THREE.CylinderGeometry(0.03, 0.03, 0.04, 20); c.rotateZ(Math.PI / 2); c.translate(-armX - 0.045, -0.3, 0); return c; })(),
      ]));
      add(yoke, M.black, pan);
      tilt = new THREE.Group();
      tilt.position.y = -0.3;
      pan.add(tilt);
      // head: lathe body with cooling fins + front barrel
      const head = geo(`mh-head-${kind}`, () => {
        const prof2: [number, number][] = [
          [0.0, len * 0.5], [R * 0.7, len * 0.5], [R * 0.78, len * 0.46], [R, len * 0.4], [R, -len * 0.18],
          [R * 0.92, -len * 0.26], [lr + 0.025, -len * 0.4], [lr + 0.02, -len * 0.5], [lr, -len * 0.5], [lr, -len * 0.48],
        ];
        const parts: THREE.BufferGeometry[] = [lathe(prof2, 48)];
        for (let i = 0; i < 6; i++) parts.push(ring(R + 0.012, 0.012, len * 0.3 - i * 0.032, 48));
        // handles
        parts.push(new THREE.TorusGeometry(0.035, 0.007, 8, 16, Math.PI).rotateY(Math.PI / 2).translate(0, len * 0.05, R + 0.01));
        parts.push(new THREE.TorusGeometry(0.035, 0.007, 8, 16, Math.PI).rotateY(-Math.PI / 2).translate(0, len * 0.05, -R - 0.01));
        return merge(parts);
      });
      add(head, M.dark, tilt);
      add(geo(`mh-trim-${kind}`, () => ring(lr + 0.012, 0.014, -len * 0.5 + 0.005, 40)), M.alu, tilt);
      const lensNode = new THREE.Object3D();
      lensNode.position.y = -len * 0.5 - 0.004;
      tilt.add(lensNode);
      lens = lensNode;
      lensR = lr;
      lensDisc(lr * 0.96, -len * 0.5 - 0.002, tilt, lensMat, emissive);
      if (wash) {
        // fresnel-like concentric lens rings
        for (let i = 1; i < 4; i++) add(geo(`mh-wring-${i}`, () => ring(lr * (1 - i * 0.22), 0.004, -len * 0.5 + 0.003)), M.glass, tilt);
      }
      selC = [0, -0.29, 0];
      selS = [armX * 2 + 0.14, 0.62, 0.34];
      break;
    }
    case "scanner": {
      add(new RoundedBoxGeometry(0.3, 0.14, 0.2, 3, 0.02), M.dark, root, 0, -0.07, 0);
      clamp(ctx, 0.01);
      tilt = new THREE.Group();
      tilt.position.y = -0.16;
      root.add(tilt);
      const mir = add(new THREE.BoxGeometry(0.16, 0.012, 0.16), M.chrome, tilt, 0, 0, 0);
      mir.rotation.x = Math.PI / 4;
      const lensNode = new THREE.Object3D();
      lensNode.position.y = -0.0;
      tilt.add(lensNode);
      lens = lensNode;
      lensR = 0.06;
      lensDisc(0.05, -0.145, root, lensMat, emissive);
      selC = [0, -0.1, 0]; selS = [0.32, 0.3, 0.22];
      break;
    }
    case "par":
    case "ledpar":
    case "fresnel": {
      const led = kind === "ledpar";
      const fres = kind === "fresnel";
      const R = fres ? 0.12 : 0.1;
      const L = led ? 0.12 : fres ? 0.2 : 0.22;
      // U-bracket
      add(geo(`par-bracket-${R}`, () => merge([
        new THREE.BoxGeometry(R * 2 + 0.07, 0.012, 0.03).translate(0, -0.06, 0),
        new THREE.BoxGeometry(0.012, R * 1.3, 0.03).translate(R + 0.032, -0.06 - R * 0.6, 0),
        new THREE.BoxGeometry(0.012, R * 1.3, 0.03).translate(-R - 0.032, -0.06 - R * 0.6, 0),
      ])), M.black);
      clamp(ctx, -0.054);
      tilt = new THREE.Group();
      tilt.position.y = -0.06 - R * 0.9;
      root.add(tilt);
      const bodyG = geo(`par-body-${kind}`, () => {
        const pts: [number, number][] = led
          ? [[0, L / 2], [R * 0.95, L / 2], [R, L / 2 - 0.01], [R, -L / 2 + 0.02], [R * 1.04, -L / 2 + 0.012], [R * 1.04, -L / 2], [R * 0.96, -L / 2]]
          : [[0, L / 2], [R * 0.6, L / 2], [R * 0.75, L / 2 - 0.02], [R, L * 0.15], [R, -L / 2 + 0.03], [R * 1.05, -L / 2 + 0.02], [R * 1.05, -L / 2], [R * 0.93, -L / 2]];
        const parts = [lathe(pts, 40)];
        if (!led) for (let i = 0; i < 4; i++) parts.push(ring(R * 0.62 + i * 0.002, 0.008, L / 2 - 0.035 - i * 0.025, 28));
        parts.push((() => { const c = new THREE.CylinderGeometry(0.013, 0.013, R * 2 + 0.08, 12); c.rotateZ(Math.PI / 2); return c; })());
        return merge(parts);
      });
      add(bodyG, M.dark, tilt);
      const lensNode = new THREE.Object3D();
      lensNode.position.y = -L / 2 - 0.004;
      tilt.add(lensNode);
      lens = lensNode;
      lensR = R * 0.9;
      lensDisc(R * 0.9, -L / 2 + 0.002, tilt, lensMat, emissive, led);
      if (fres) {
        // stepped fresnel rings + barn doors
        for (let i = 1; i < 5; i++) add(geo(`fres-${i}`, () => ring(R * 0.9 * (1 - i * 0.18), 0.003, -L / 2 + 0.004)), M.glass, tilt);
        for (let s = 0; s < 4; s++) {
          const d = add(geo("barn", () => new THREE.BoxGeometry(R * 1.7, 0.004, R * 0.9)), M.black, tilt, 0, -L / 2 - 0.05, 0);
          d.position.set(Math.sin((s * Math.PI) / 2) * (R + 0.02), -L / 2 - 0.045, Math.cos((s * Math.PI) / 2) * (R + 0.02));
          d.rotation.y = (s * Math.PI) / 2;
          d.rotation.x = -0.45;
        }
      }
      selC = [0, -0.2, 0]; selS = [R * 2 + 0.1, 0.34, R * 2 + 0.06];
      break;
    }
    case "profile": {
      const R = 0.085, L = 0.5;
      add(geo("prof-bracket", () => merge([
        new THREE.BoxGeometry(0.2, 0.012, 0.03).translate(0, -0.06, 0),
        new THREE.BoxGeometry(0.012, 0.1, 0.03).translate(0.1, -0.11, 0),
        new THREE.BoxGeometry(0.012, 0.1, 0.03).translate(-0.1, -0.11, 0),
      ])), M.black);
      clamp(ctx, -0.054);
      tilt = new THREE.Group();
      tilt.position.y = -0.16;
      root.add(tilt);
      add(geo("prof-body", () => merge([
        lathe([[0, L / 2], [R * 0.9, L / 2], [R, L / 2 - 0.04], [R, 0.03], [R * 0.78, 0.0], [R * 0.78, -L * 0.3], [R * 0.9, -L * 0.34], [R * 0.9, -L / 2 + 0.01], [R * 0.74, -L / 2]], 36),
        (() => { const c = new THREE.CylinderGeometry(0.013, 0.013, 0.22, 10); c.rotateZ(Math.PI / 2); return c; })(),
        ...[0, 1, 2, 3, 4].map((i) => ring(R + 0.006, 0.01, L / 2 - 0.08 - i * 0.03, 36)),
      ])), M.dark, tilt);
      const lensNode = new THREE.Object3D();
      lensNode.position.y = -L / 2 - 0.004;
      tilt.add(lensNode);
      lens = lensNode;
      lensR = R * 0.72;
      lensDisc(R * 0.72, -L / 2 + 0.002, tilt, lensMat, emissive);
      selC = [0, -0.2, 0]; selS = [0.24, 0.62, 0.2];
      break;
    }
    case "bar": {
      const W = 1.0;
      add(geo("bar-body", () => merge([
        new RoundedBoxGeometry(W, 0.075, 0.1, 3, 0.012).translate(0, -0.06, 0),
        new THREE.BoxGeometry(0.03, 0.09, 0.11).translate(W / 2, -0.06, 0),
        new THREE.BoxGeometry(0.03, 0.09, 0.11).translate(-W / 2, -0.06, 0),
      ])), M.dark);
      add(geo("bar-bracket", () => merge([
        new THREE.BoxGeometry(0.03, 0.05, 0.06).translate(W * 0.3, -0.005, 0),
        new THREE.BoxGeometry(0.03, 0.05, 0.06).translate(-W * 0.3, -0.005, 0),
      ])), M.alu);
      const strip = new THREE.Mesh(geo("bar-strip", () => { const p = new THREE.PlaneGeometry(W - 0.02, 0.07); p.rotateX(Math.PI / 2); p.rotateY(Math.PI); const u = p.attributes.uv as THREE.BufferAttribute; for (let i = 0; i < u.count; i++) u.setXY(i, u.getX(i) * 12, u.getY(i)); return p; }), lensMat);
      strip.position.y = -0.0985;
      root.add(strip);
      emissive.push(strip);
      const ln = new THREE.Object3D();
      ln.position.y = -0.1;
      root.add(ln);
      lens = ln;
      lensR = 0.1;
      selC = [0, -0.06, 0]; selS = [W + 0.1, 0.14, 0.14];
      break;
    }
    case "strobe":
    case "blinder": {
      const W = kind === "blinder" ? 0.34 : 0.5, H = kind === "blinder" ? 0.17 : 0.13;
      add(geo(`panel-${kind}`, () => new RoundedBoxGeometry(W, 0.1, H, 3, 0.012).translate(0, -0.06, 0)), M.dark);
      clamp(ctx, 0.0);
      const face = new THREE.Mesh(geo(`panelface-${kind}`, () => { const p = new THREE.PlaneGeometry(W - 0.03, H - 0.03); p.rotateX(Math.PI / 2); p.rotateY(Math.PI); const u = p.attributes.uv as THREE.BufferAttribute; for (let i = 0; i < u.count; i++) u.setXY(i, u.getX(i) * (kind === "blinder" ? 2 : 4), u.getY(i)); return p; }), lensMat);
      face.position.y = -0.1103;
      root.add(face);
      emissive.push(face);
      const ln = new THREE.Object3D();
      ln.position.y = -0.112;
      root.add(ln);
      lens = ln;
      lensR = H * 0.5;
      selC = [0, -0.06, 0]; selS = [W + 0.06, 0.16, H + 0.06];
      break;
    }
    default: {
      add(geo("generic-box", () => new RoundedBoxGeometry(0.28, 0.16, 0.2, 3, 0.02).translate(0, -0.09, 0)), M.dark);
      clamp(ctx, 0.0);
      const ln = new THREE.Object3D();
      ln.position.y = -0.172;
      root.add(ln);
      lens = ln;
      lensR = 0.05;
      lensDisc(0.04, -0.171, root, lensMat, emissive);
      selC = [0, -0.09, 0]; selS = [0.34, 0.22, 0.26];
    }
  }

  return {
    kind, root, pan, tilt, lens, lensRadius: lensR, emissive,
    selCenter: selC, selSize: selS,
    dispose() { own.forEach((m) => m.dispose()); },
  };
}

/** Emissive material accessor for a built model (the shared per-rig basic material). */
export function lensMaterialOf(m: FixtureModel): THREE.MeshBasicMaterial {
  return m.emissive[0].material as THREE.MeshBasicMaterial;
}
