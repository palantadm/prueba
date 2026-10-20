// Fixture rigs: bodies (par / bar / strobe / moving head with real yoke+head), volumetric beams, floor pools.
import * as THREE from "three";
import type { Fixture, FixtureProfile } from "../types";
import { beamAngle, decode, makePlan, newState, type FxState, type Plan } from "../lib/stage-dmx";
import { radialTexture } from "./objects";

const D2R = Math.PI / 180;
const NOOP = () => undefined;

// ── shared resources ────────────────────────────────────
const bodyMat = new THREE.MeshStandardMaterial({ color: "#3a3e48", roughness: 0.5, metalness: 0.5 });
const bodyMat2 = new THREE.MeshStandardMaterial({ color: "#4a4f5a", roughness: 0.45, metalness: 0.5 });
const stroboMat = new THREE.MeshStandardMaterial({ color: "#c8ccd2", roughness: 0.4, metalness: 0.3 });
const GEO = {
  base: new THREE.BoxGeometry(0.3, 0.09, 0.22),
  arm: new THREE.BoxGeometry(0.035, 0.3, 0.07),
  head: new THREE.CylinderGeometry(0.115, 0.095, 0.27, 20),
  headCap: new THREE.CylinderGeometry(0.095, 0.095, 0.03, 20),
  par: new THREE.CylinderGeometry(0.1, 0.085, 0.2, 20),
  bracket: new THREE.BoxGeometry(0.26, 0.015, 0.03),
  bar: new THREE.BoxGeometry(1.0, 0.08, 0.1),
  strobe: new THREE.BoxGeometry(0.5, 0.12, 0.1),
  other: new THREE.BoxGeometry(0.25, 0.14, 0.18),
  lens: (() => { const g = new THREE.CircleGeometry(1, 24); g.rotateX(Math.PI / 2); return g; })(),
  // unit cone: apex at origin, base (radius 1) at y = -1
  beam: (() => { const g = new THREE.CylinderGeometry(0.02, 1, 1, 40, 1, true); g.translate(0, -0.5, 0); return g; })(),
  pool: (() => { const g = new THREE.PlaneGeometry(1, 1); g.rotateX(-Math.PI / 2); return g; })(),
  sel: new THREE.EdgesGeometry(new THREE.BoxGeometry(1, 1, 1)),
};

const BEAM_VERT = /* glsl */ `
varying vec3 vN; varying vec3 vV; varying float vT;
void main(){
  vT = -position.y;
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  vV = -mv.xyz; vN = normalize(normalMatrix * normal);
  gl_Position = projectionMatrix * mv;
}`;
const BEAM_FRAG = /* glsl */ `
uniform vec3 color; uniform float intensity; uniform float haze; uniform float gain;
varying vec3 vN; varying vec3 vV; varying float vT;
void main(){
  float f = abs(dot(normalize(vN), normalize(vV)));
  float edge = pow(f, 1.6);
  float along = pow(1.0 - vT, 1.4) * smoothstep(0.0, 0.025, vT);
  float vis = mix(0.10, 1.0, haze);
  float a = intensity * edge * along * vis * gain;
  gl_FragColor = vec4(color, clamp(a, 0.0, 1.0));
  #include <colorspace_fragment>
}`;

export interface Rig {
  fx: Fixture;
  plan: Plan | null;
  sig: string;
  root: THREE.Group;
  panG: THREE.Group | null;
  tiltG: THREE.Group;
  beam: THREE.Mesh;
  beamMat: THREE.ShaderMaterial;
  lens: THREE.Mesh;
  lensMat: THREE.MeshBasicMaterial;
  pool: THREE.Group;
  poolMesh: THREE.Mesh;
  poolMat: THREE.MeshBasicMaterial;
  label: THREE.Sprite;
  sel: THREE.LineSegments;
  state: FxState;
  // derived each frame
  origin: THREE.Vector3;
  dir: THREE.Vector3;
  dist: number;
  halfAngle: number;
  score: number;
}

export const fxSig = (fx: Fixture, prof: FixtureProfile | undefined) =>
  `${fx.profile}|${fx.mode}|${prof ? prof.modes.length : -1}|${prof?.category}`;

function labelSprite(text: string): THREE.Sprite {
  const c = document.createElement("canvas");
  c.width = 128; c.height = 64;
  const g = c.getContext("2d")!;
  g.fillStyle = "rgba(10,12,14,0.78)"; g.beginPath(); g.roundRect(8, 8, 112, 48, 10); g.fill();
  g.strokeStyle = "#3cc4e8"; g.lineWidth = 2; g.stroke();
  g.fillStyle = "#e6f6fb"; g.font = "bold 30px ui-monospace, monospace"; g.textAlign = "center"; g.textBaseline = "middle";
  g.fillText(text, 64, 33);
  const t = new THREE.CanvasTexture(c);
  const s = new THREE.Sprite(new THREE.SpriteMaterial({ map: t, depthTest: false, transparent: true }));
  s.scale.set(0.5, 0.25, 1);
  s.renderOrder = 999;
  s.raycast = NOOP;
  return s;
}

export function buildRig(fx: Fixture, prof: FixtureProfile | undefined): Rig {
  const plan = makePlan(fx, prof);
  const moving = !!plan?.moving;
  const cat = prof?.category ?? "other";
  const root = new THREE.Group();
  root.rotation.order = "YXZ";
  root.userData.fixtureId = fx.id;
  const tiltG = new THREE.Group();
  let panG: THREE.Group | null = null;
  const add = (geo: THREE.BufferGeometry, mat: THREE.Material, parent: THREE.Object3D, y = 0, x = 0): THREE.Mesh => {
    const m = new THREE.Mesh(geo, mat);
    m.position.set(x, y, 0);
    m.userData.fixtureId = fx.id;
    parent.add(m);
    return m;
  };

  let lensR = 0.08;
  if (moving) {
    add(GEO.base, bodyMat2, root, -0.045);
    panG = new THREE.Group();
    panG.position.y = -0.09;
    root.add(panG);
    add(GEO.arm, bodyMat, panG, -0.15, 0.15);
    add(GEO.arm, bodyMat, panG, -0.15, -0.15);
    tiltG.position.y = -0.22;
    panG.add(tiltG);
    add(GEO.head, bodyMat, tiltG, -0.135);
    add(GEO.headCap, bodyMat2, tiltG, -0.28);
    tiltG.position.y = -0.22;
    lensR = 0.085;
  } else if (cat === "bar") {
    root.add(tiltG);
    add(GEO.bar, bodyMat, tiltG, -0.04);
    lensR = 0.05;
  } else if (cat === "strobe") {
    root.add(tiltG);
    add(GEO.strobe, stroboMat, tiltG, -0.06);
    lensR = 0.12;
  } else if (cat === "par" || cat === "wash" || cat === "spot" || cat === "beam" || cat === "effect") {
    root.add(tiltG);
    add(GEO.bracket, bodyMat2, tiltG, 0.0);
    add(GEO.par, bodyMat, tiltG, -0.11);
    lensR = 0.085;
  } else {
    root.add(tiltG);
    add(GEO.other, bodyMat, tiltG, -0.07);
    lensR = 0.06;
  }
  const lensY = moving ? -0.295 : cat === "bar" ? -0.082 : cat === "strobe" ? -0.122 : cat === "par" || cat === "wash" || cat === "spot" || cat === "beam" || cat === "effect" ? -0.21 : -0.142;

  const lensMat = new THREE.MeshBasicMaterial({ color: "#ffffff", toneMapped: false });
  const lens = new THREE.Mesh(GEO.lens, lensMat);
  lens.scale.setScalar(lensR);
  lens.position.y = lensY;
  lens.raycast = NOOP;
  tiltG.add(lens);

  const beamMat = new THREE.ShaderMaterial({
    uniforms: { color: { value: new THREE.Color(1, 1, 1) }, intensity: { value: 0 }, haze: { value: 0.3 }, gain: { value: 1 } },
    vertexShader: BEAM_VERT, fragmentShader: BEAM_FRAG,
    transparent: true, depthWrite: false, side: THREE.DoubleSide, blending: THREE.AdditiveBlending,
  });
  const beam = new THREE.Mesh(GEO.beam, beamMat);
  beam.position.y = lensY;
  beam.raycast = NOOP;
  beam.frustumCulled = false;
  beam.visible = false;
  tiltG.add(beam);

  const poolMat = new THREE.MeshBasicMaterial({
    map: radialTexture(), color: "#ffffff", transparent: true, blending: THREE.AdditiveBlending,
    depthWrite: false, toneMapped: false, polygonOffset: true, polygonOffsetFactor: -2,
  });
  const poolMesh = new THREE.Mesh(GEO.pool, poolMat);
  poolMesh.raycast = NOOP;
  const pool = new THREE.Group();
  pool.add(poolMesh);
  pool.visible = false;

  const label = labelSprite(String(fx.id));
  label.position.set(0, 0.35, 0);
  label.visible = false;
  root.add(label);

  const sel = new THREE.LineSegments(GEO.sel, new THREE.LineBasicMaterial({ color: "#3cc4e8", depthTest: false }));
  sel.scale.set(moving ? 0.42 : cat === "bar" ? 1.1 : 0.34, moving ? 0.5 : 0.3, moving ? 0.34 : 0.26);
  sel.position.y = moving ? -0.18 : -0.1;
  sel.renderOrder = 998;
  sel.visible = false;
  sel.raycast = NOOP;
  root.add(sel);

  const rig: Rig = {
    fx, plan, sig: fxSig(fx, prof), root, panG, tiltG, beam, beamMat, lens, lensMat, pool, poolMesh, poolMat, label, sel,
    state: newState(), origin: new THREE.Vector3(), dir: new THREE.Vector3(0, -1, 0), dist: 10, halfAngle: 0.2, score: 0,
  };
  applyTransform(rig, fx);
  return rig;
}

export function applyTransform(rig: Rig, fx: Fixture) {
  rig.fx = fx;
  rig.root.position.set(fx.pos[0], fx.pos[1], fx.pos[2]);
  rig.root.rotation.set(fx.rot[0] * D2R, fx.rot[1] * D2R, fx.rot[2] * D2R);
}

const _q = new THREE.Quaternion();
const _v = new THREE.Vector3();
const tmpC = new THREE.Color();

export interface Env { haze: number; beams: boolean; pools: boolean; labels: boolean; time: number }

/** Decode DMX, pose the head, size the beam and the floor pool. */
export function updateRig(rig: Rig, frame: Uint8Array | undefined, env: Env) {
  const st = rig.state;
  const plan = rig.plan;
  if (plan) decode(plan, rig.fx, frame, env.time, st);
  else { st.bright = 0; }
  if (plan?.moving && rig.panG) {
    rig.panG.rotation.y = (st.pan - 0.5) * plan.panRange * D2R;
    rig.tiltG.rotation.x = (st.tilt - 0.5) * plan.tiltRange * D2R;
  }
  rig.root.updateMatrixWorld(true);
  const angle = plan ? beamAngle(plan, st) : 25;
  rig.halfAngle = Math.max(0.01, (angle * D2R) / 2);
  const on = st.bright > 0.004;

  rig.lensMat.color.setRGB(st.r * (0.08 + st.bright), st.g * (0.08 + st.bright), st.b * (0.08 + st.bright));
  rig.label.visible = env.labels;

  // world origin / direction of the beam
  rig.beam.getWorldPosition(rig.origin);
  rig.beam.getWorldQuaternion(_q);
  rig.dir.set(0, -1, 0).applyQuaternion(_q);

  rig.score = on ? st.bright : 0;
  if (!on) { rig.beam.visible = false; rig.pool.visible = false; return; }

  const dy = rig.dir.y;
  rig.dist = dy < -0.02 ? Math.min(40, rig.origin.y / -dy) : 18;
  if (rig.origin.y <= 0.02) rig.dist = 12;
  const L = rig.dist;
  const R = L * Math.tan(rig.halfAngle);
  rig.beam.visible = env.beams;
  if (env.beams) {
    rig.beam.scale.set(R, L, R);
    const u = rig.beamMat.uniforms;
    (u.color.value as THREE.Color).setRGB(st.r, st.g, st.b);
    u.intensity.value = st.bright * 0.9;
    u.haze.value = env.haze;
    // narrow beams read brighter
    u.gain.value = Math.min(2.2, Math.sqrt(30 / Math.max(2, angle)));
  }

  const hitsFloor = dy < -0.05 && rig.origin.y > 0.02;
  rig.pool.visible = env.pools && hitsFloor;
  if (rig.pool.visible) {
    _v.copy(rig.origin).addScaledVector(rig.dir, rig.dist);
    rig.pool.position.set(_v.x, 0.012, _v.z);
    const horiz = Math.hypot(rig.dir.x, rig.dir.z);
    rig.pool.rotation.y = Math.atan2(rig.dir.x, rig.dir.z);
    const diam = Math.max(0.15, 2 * R) * 1.25;
    const stretch = Math.min(6, 1 / Math.max(0.17, -dy)); // 1/cos(incidence)
    void horiz;
    rig.poolMesh.scale.set(diam, 1, diam * stretch);
    const k = Math.min(1, 0.55 * st.bright * Math.min(1.8, 25 / Math.max(2, angle)));
    tmpC.setRGB(st.r * k, st.g * k, st.b * k);
    rig.poolMat.color.copy(tmpC);
  }
}

export function disposeRig(rig: Rig) {
  rig.beamMat.dispose();
  rig.lensMat.dispose();
  rig.poolMat.dispose();
  (rig.label.material as THREE.SpriteMaterial).map?.dispose();
  (rig.label.material as THREE.SpriteMaterial).dispose();
}
