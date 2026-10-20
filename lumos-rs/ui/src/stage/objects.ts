// Stage object builders (truss, platform, wall, column, curtain, tree, box, screen).
import * as THREE from "three";
import { mergeGeometries } from "three/examples/jsm/utils/BufferGeometryUtils.js";
import type { StageObject } from "../types";
import type { MediaCache, MediaEntry } from "./media";

export interface StageObj {
  id: number;
  group: THREE.Group;
  /** geometry signature — rebuild when it changes */
  sig: string;
  /** per-frame hook (screens) */
  tick?: (now: number, media: MediaCache) => void;
  dispose(): void;
}

export const objSig = (o: StageObject) =>
  JSON.stringify([o.kind, o.size, o.color, o.media, o.screen_type, o.brightness]);

const matCache = new Map<string, THREE.MeshStandardMaterial>();
function std(color: string, rough = 0.7, metal = 0.1, side: THREE.Side = THREE.FrontSide): THREE.MeshStandardMaterial {
  const key = `${color}|${rough}|${metal}|${side}`;
  let m = matCache.get(key);
  if (!m) { m = new THREE.MeshStandardMaterial({ color, roughness: rough, metalness: metal, side }); matCache.set(key, m); }
  return m;
}

function cyl(a: THREE.Vector3, b: THREE.Vector3, r: number): THREE.BufferGeometry {
  const len = a.distanceTo(b);
  const g = new THREE.CylinderGeometry(r, r, len, 6, 1);
  const m = new THREE.Matrix4();
  const q = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 1, 0), b.clone().sub(a).normalize());
  m.compose(a.clone().add(b).multiplyScalar(0.5), q, new THREE.Vector3(1, 1, 1));
  g.applyMatrix4(m);
  return g;
}

export function trussGeometry(length: number, side: number): THREE.BufferGeometry {
  const hs = side / 2;
  const corners = [[hs, hs], [-hs, hs], [-hs, -hs], [hs, -hs]]; // (y,z)
  const parts: THREE.BufferGeometry[] = [];
  const x0 = -length / 2, x1 = length / 2;
  const P = (x: number, c: number[]) => new THREE.Vector3(x, c[0], c[1]);
  for (const c of corners) parts.push(cyl(P(x0, c), P(x1, c), side * 0.085));
  const n = Math.max(1, Math.round(length / side));
  const step = length / n;
  for (let i = 0; i < n; i++) {
    const xa = x0 + i * step, xb = xa + step;
    for (let f = 0; f < 4; f++) {
      const ca = corners[f], cb = corners[(f + 1) % 4];
      parts.push(i % 2 === 0 ? cyl(P(xa, ca), P(xb, cb), side * 0.04) : cyl(P(xa, cb), P(xb, ca), side * 0.04));
    }
  }
  for (let i = 0; i <= n; i++) {
    const x = x0 + i * step;
    for (let f = 0; f < 4; f++) parts.push(cyl(P(x, corners[f]), P(x, corners[(f + 1) % 4]), side * 0.03));
  }
  const g = mergeGeometries(parts)!;
  parts.forEach((p) => p.dispose());
  return g;
}

export function curtainGeometry(w: number, h: number): THREE.BufferGeometry {
  const g = new THREE.PlaneGeometry(w, h, Math.max(24, Math.round(w * 14)), 1);
  const p = g.attributes.position as THREE.BufferAttribute;
  const folds = Math.max(4, Math.round(w * 2.2));
  for (let i = 0; i < p.count; i++) {
    const u = (p.getX(i) / w + 0.5) * folds * Math.PI * 2;
    const y = p.getY(i) / h + 0.5;
    p.setZ(i, (Math.sin(u) * 0.11 + Math.sin(u * 2.7 + 1.3) * 0.025) * (0.6 + 0.4 * y));
  }
  g.computeVertexNormals();
  return g;
}

const SCREEN_VERT = /* glsl */ `varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position,1.0); }`;
const SCREEN_FRAG = /* glsl */ `
uniform sampler2D map; uniform float hasMap; uniform vec3 tint; uniform float brightness;
uniform vec2 cells; uniform float led; uniform float soft;
varying vec2 vUv;
void main(){
  vec3 c = hasMap > 0.5 ? texture2D(map, vUv).rgb : tint;
  float a = 1.0;
  if (led > 0.5) {
    vec2 g = fract(vUv * cells); vec2 d = abs(g - 0.5) * 2.0;
    float m = smoothstep(1.0, 0.55, max(d.x, d.y));
    c *= mix(0.12, 1.0, m);
  }
  if (soft > 0.5) { vec2 e = min(vUv, 1.0 - vUv); a = smoothstep(0.0, 0.07, min(e.x, e.y)); }
  gl_FragColor = vec4(c * brightness, a);
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
}`;

let radialTex: THREE.CanvasTexture | null = null;
export function radialTexture(): THREE.CanvasTexture {
  if (radialTex) return radialTex;
  const c = document.createElement("canvas");
  c.width = c.height = 128;
  const g = c.getContext("2d")!;
  const gr = g.createRadialGradient(64, 64, 0, 64, 64, 64);
  gr.addColorStop(0, "rgba(255,255,255,1)");
  gr.addColorStop(0.35, "rgba(255,255,255,0.55)");
  gr.addColorStop(0.7, "rgba(255,255,255,0.12)");
  gr.addColorStop(1, "rgba(255,255,255,0)");
  g.fillStyle = gr; g.fillRect(0, 0, 128, 128);
  radialTex = new THREE.CanvasTexture(c);
  return radialTex;
}

function buildScreen(o: StageObject, g: THREE.Group): StageObj["tick"] {
  const [w, h, d] = o.size;
  const led = o.screen_type !== "projection";
  const frame = new THREE.Mesh(new THREE.BoxGeometry(w + 0.08, h + 0.08, Math.max(0.04, d)), std("#0c0d10", 0.6, 0.4));
  g.add(frame);
  const z = Math.max(0.04, d) / 2 + 0.002;

  const mkMat = (isLed: boolean) => new THREE.ShaderMaterial({
    uniforms: {
      map: { value: null }, hasMap: { value: 0 }, tint: { value: new THREE.Color(1, 1, 1) }, brightness: { value: o.brightness * (isLed ? 1.8 : 1) },
      cells: { value: new THREE.Vector2(Math.max(8, Math.round(w * 24)), Math.max(8, Math.round(h * 24))) },
      led: { value: isLed ? 1 : 0 }, soft: { value: isLed ? 0 : 1 },
    },
    vertexShader: SCREEN_VERT, fragmentShader: SCREEN_FRAG,
    transparent: !isLed, depthWrite: isLed,
    blending: isLed ? THREE.NormalBlending : THREE.AdditiveBlending,
    toneMapped: !isLed,
  });

  let halo: THREE.Mesh | null = null;
  if (!led) {
    const surf = new THREE.Mesh(new THREE.PlaneGeometry(w, h), std(o.color || "#e8e8e8", 0.9, 0));
    surf.position.z = z;
    g.add(surf);
  }
  const mat = mkMat(led);
  const face = new THREE.Mesh(new THREE.PlaneGeometry(w, h), mat);
  face.position.z = z + (led ? 0 : 0.003);
  face.renderOrder = led ? 0 : 2;
  g.add(face);
  if (led) {
    halo = new THREE.Mesh(
      new THREE.PlaneGeometry(w * 1.5, h * 1.7),
      new THREE.MeshBasicMaterial({ map: radialTexture(), transparent: true, blending: THREE.AdditiveBlending, depthWrite: false, opacity: 0.35 * o.brightness, toneMapped: false }),
    );
    halo.position.z = z + 0.01;
    halo.raycast = () => undefined;
    g.add(halo);
  }
  const u = mat.uniforms;
  return (now, media) => {
    const e: MediaEntry = media.get(o.media);
    u.map.value = e.tex;
    u.hasMap.value = e.tex ? 1 : 0;
    (u.tint.value as THREE.Color).copy(e.color);
    if (halo) {
      (halo.material as THREE.MeshBasicMaterial).color.copy(e.avg);
    }
    void now;
  };
}

export function buildStageObject(o: StageObject): StageObj {
  const g = new THREE.Group();
  g.name = o.name;
  const [w, h, d] = o.size;
  const col = o.color || "#888888";
  let tick: StageObj["tick"];
  switch (o.kind) {
    case "truss": {
      const side = Math.max(0.15, h || 0.3);
      const m = new THREE.Mesh(trussGeometry(Math.max(0.3, w), side), std(col, 0.3, 0.92));
      g.add(m);
      // end plates
      const plate = new THREE.BoxGeometry(0.012, side * 1.04, side * 1.04);
      for (const sx of [-1, 1]) { const p = new THREE.Mesh(plate, std("#7b8088", 0.35, 0.9)); p.position.x = (sx * Math.max(0.3, w)) / 2; g.add(p); }
      break;
    }
    case "platform": {
      g.add(new THREE.Mesh(new THREE.BoxGeometry(w, h, d), std(col, 0.8, 0.05)));
      const top = new THREE.Mesh(new THREE.BoxGeometry(w + 0.02, 0.025, d + 0.02), std("#15161a", 0.5, 0.15));
      top.position.y = h / 2 + 0.004;
      g.add(top);
      const trim = new THREE.Mesh(new THREE.BoxGeometry(w + 0.03, 0.05, d + 0.03), std("#07080a", 0.6, 0.3));
      trim.position.y = -h / 2 + 0.05;
      g.add(trim);
      break;
    }
    case "wall": {
      g.add(new THREE.Mesh(new THREE.BoxGeometry(w, h, d), std(col, 0.92, 0.0)));
      break;
    }
    case "box": {
      g.add(new THREE.Mesh(new THREE.BoxGeometry(w, h, d), std(col, 0.55, 0.1)));
      break;
    }
    case "column": {
      g.add(new THREE.Mesh(new THREE.CylinderGeometry(w / 2, w / 2, h, 32), std(col, 0.55, 0.12)));
      const baseM = std("#25272d", 0.5, 0.2);
      const b1 = new THREE.Mesh(new THREE.CylinderGeometry(w * 0.62, w * 0.66, 0.12, 32), baseM); b1.position.y = -h / 2 + 0.06; g.add(b1);
      const b2 = new THREE.Mesh(new THREE.CylinderGeometry(w * 0.66, w * 0.62, 0.12, 32), baseM); b2.position.y = h / 2 - 0.06; g.add(b2);
      break;
    }
    case "curtain": {
      g.add(new THREE.Mesh(curtainGeometry(w, h), std(col, 0.95, 0, THREE.DoubleSide)));
      break;
    }
    case "tree": {
      const trunk = new THREE.Mesh(new THREE.CylinderGeometry(w * 0.06, w * 0.09, h * 0.35, 8), std("#4a3322", 0.9, 0));
      trunk.position.y = -h / 2 + h * 0.175;
      g.add(trunk);
      for (let i = 0; i < 3; i++) {
        const cone = new THREE.Mesh(new THREE.ConeGeometry(w * (0.5 - i * 0.12), h * 0.4, 10), std(col, 0.9, 0));
        cone.position.y = -h / 2 + h * (0.3 + i * 0.22) + h * 0.2;
        g.add(cone);
      }
      break;
    }
    case "screen":
      tick = buildScreen(o, g);
      break;
  }
  g.traverse((c) => {
    c.userData.objectId = o.id;
    const m = c as THREE.Mesh;
    if (m.isMesh && o.kind !== "screen") { m.castShadow = o.kind !== "curtain"; m.receiveShadow = true; }
    else if (m.isMesh && o.kind === "screen") { m.receiveShadow = false; }
  });
  return {
    id: o.id, group: g, sig: objSig(o), tick,
    dispose() {
      g.traverse((c) => {
        const m = c as THREE.Mesh;
        if (m.geometry) m.geometry.dispose();
        const mat = m.material as THREE.Material | undefined;
        if (mat && !(mat instanceof THREE.MeshStandardMaterial)) mat.dispose();
      });
    },
  };
}
