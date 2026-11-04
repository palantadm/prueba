// Fixture rigs: detailed bodies (procedural or GDTF), pan/tilt from DMX, emissive lens, volumetric beam, gobo floor pools.
import * as THREE from "three";
import type { BeamInfo, Fixture, FixtureProfile } from "../types";
import { beamAngle, decode, makePlan, newState, type FxState, type Plan } from "../lib/stage-dmx";
import { radialTexture } from "./objects";
import { buildProcedural, classify, type FixtureKind, type FixtureModel } from "./fixture-models";
import { buildGdtfModel } from "./gdtf-model";
import { BeamVisual } from "./beams";
import { goboTexture, whiteTexture } from "./gobos";
import { getSmoke, machineKind } from "./smoke";

const D2R = Math.PI / 180;
const NOOP = () => undefined;

const GEO = {
  pool: (() => { const g = new THREE.PlaneGeometry(1, 1); g.rotateX(-Math.PI / 2); return g; })(),
  sel: new THREE.EdgesGeometry(new THREE.BoxGeometry(1, 1, 1)),
};

let glowTex: THREE.CanvasTexture | null = null;
function glowTexture(): THREE.CanvasTexture {
  if (glowTex) return glowTex;
  const c = document.createElement("canvas");
  c.width = c.height = 128;
  const g = c.getContext("2d")!;
  const gr = g.createRadialGradient(64, 64, 0, 64, 64, 64);
  gr.addColorStop(0, "rgba(255,255,255,1)");
  gr.addColorStop(0.12, "rgba(255,255,255,0.7)");
  gr.addColorStop(0.4, "rgba(255,255,255,0.14)");
  gr.addColorStop(1, "rgba(255,255,255,0)");
  g.fillStyle = gr; g.fillRect(0, 0, 128, 128);
  glowTex = new THREE.CanvasTexture(c);
  return glowTex;
}

export interface Rig {
  fx: Fixture;
  plan: Plan | null;
  prof: FixtureProfile | undefined;
  sig: string;
  kind: FixtureKind;
  /** smoke / haze machine: emits through the smoke simulation, never as a light beam */
  machine: boolean;
  model: FixtureModel;
  info: BeamInfo | null;
  assets: string;
  root: THREE.Group;
  panG: THREE.Object3D | null;
  tiltG: THREE.Object3D | null;
  beam: BeamVisual;
  /** extra beams produced by a prism (created on demand) */
  extra: BeamVisual[];
  lensMat: THREE.MeshBasicMaterial;
  glow: THREE.Sprite;
  pool: THREE.Group;
  poolMesh: THREE.Mesh;
  poolMat: THREE.MeshBasicMaterial;
  label: THREE.Sprite;
  sel: THREE.LineSegments;
  state: FxState;
  goboAngle: number;
  // derived each frame
  origin: THREE.Vector3;
  dir: THREE.Vector3;
  dist: number;
  halfAngle: number;
  score: number;
  mapped: boolean;
}

export const fxSig = (fx: Fixture, prof: FixtureProfile | undefined) =>
  `${fx.profile}|${fx.mode}|${prof ? prof.modes.length : -1}|${prof?.category}|${prof?.model3d ? "g" : "p"}`;

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
  let model: FixtureModel;
  let info: BeamInfo | null = null;
  let assets = "";
  let kind: FixtureKind;
  if (prof?.model3d) {
    const g = buildGdtfModel(prof.model3d);
    model = g;
    info = g.beam;
    assets = prof.model3d.assets;
    kind = plan?.moving ? "moving-spot" : "generic";
  } else {
    kind = classify(prof, plan);
    model = buildProcedural(kind, prof);
  }
  const root = model.root;
  root.userData.fixtureId = fx.id;
  root.traverse((o) => { o.userData.fixtureId = fx.id; });

  const lensMat = model.emissive.length ? (model.emissive[0].material as THREE.MeshBasicMaterial) : new THREE.MeshBasicMaterial();

  const glow = new THREE.Sprite(new THREE.SpriteMaterial({
    map: glowTexture(), color: "#ffffff", blending: THREE.AdditiveBlending, depthWrite: false, transparent: true, toneMapped: false,
  }));
  glow.raycast = NOOP;
  glow.visible = false;
  model.lens.add(glow);

  const beam = new BeamVisual(whiteTexture());

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
  label.position.set(0, 0.3, 0);
  label.visible = false;
  root.add(label);

  const sel = new THREE.LineSegments(GEO.sel, new THREE.LineBasicMaterial({ color: "#3cc4e8", depthTest: false, toneMapped: false }));
  sel.scale.set(...model.selSize);
  sel.position.set(...model.selCenter);
  sel.renderOrder = 998;
  sel.visible = false;
  sel.raycast = NOOP;
  root.add(sel);

  const rig: Rig = {
    fx, plan, prof, sig: fxSig(fx, prof), kind, machine: !!machineKind(prof), model, info, assets, root, panG: model.pan, tiltG: model.tilt,
    beam, extra: [], lensMat, glow, pool, poolMesh, poolMat, label, sel,
    state: newState(), goboAngle: 0, origin: new THREE.Vector3(), dir: new THREE.Vector3(0, -1, 0), dist: 10, halfAngle: 0.2, score: 0, mapped: false,
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
const _q2 = new THREE.Quaternion();
const _ax = new THREE.Vector3();
const _d2 = new THREE.Vector3();

export interface Env {
  crowd: number;
  haze: number;
  beams: boolean;
  pools: boolean;
  labels: boolean;
  time: number;
  dt: number;
  camPos: THREE.Vector3;
}

/** Wash fixtures have softer, wider falloff than spots. */
function softness(rig: Rig, st: FxState): number {
  let s = rig.kind === "moving-wash" || rig.kind === "ledpar" || rig.kind === "fresnel" || rig.kind === "par" || rig.kind === "blinder" || rig.kind === "strobe" ? 0.55 : 0.12;
  if (rig.info) s = rig.info.beam_type === "wash" || rig.info.beam_type === "fresnel" || rig.info.beam_type === "pc" ? 0.5 : 0.12;
  s += st.frost * 0.5 + (1 - st.focus) * 0.2 * (rig.kind === "moving-spot" || rig.kind === "moving-beam" ? 1 : 0);
  return Math.min(1, s);
}

/** Decode DMX, pose the head, shape the beam and the floor pool. */
export function updateRig(rig: Rig, frame: Uint8Array | undefined, env: Env) {
  const st = rig.state;
  const plan = rig.plan;
  if (plan) decode(plan, rig.fx, frame, env.time, st);
  else { st.bright = 0; }
  if (rig.machine) {
    // no light beam: a status LED shows the machine output, the smoke itself comes from the simulation
    const o = getSmoke().outputOf(rig.fx.id);
    st.bright = 0;
    rig.lensMat.color.setRGB(0.05 + o * 0.3, 0.12 + o * 3.2, 0.06 + o * 0.5);
    rig.glow.visible = false; rig.beam.mesh.visible = false; rig.pool.visible = false; rig.score = 0;
    rig.extra.forEach((x) => (x.mesh.visible = false));
    rig.model.lens.getWorldPosition(rig.origin);
    rig.label.visible = env.labels;
    return;
  }
  if (plan?.moving) {
    if (rig.panG) rig.panG.rotation.y = (st.pan - 0.5) * plan.panRange * D2R;
    if (rig.tiltG) rig.tiltG.rotation.x = (st.tilt - 0.5) * plan.tiltRange * D2R;
  }
  rig.root.updateMatrixWorld(true);

  let angle: number, field: number;
  if (rig.info) {
    const i = rig.info;
    angle = (i.beam_angle + (i.beam_angle_max - i.beam_angle) * st.zoom) * (1 - 0.8 * st.iris);
    field = (i.field_angle + (i.field_angle_max - i.field_angle) * st.zoom) * (1 - 0.8 * st.iris);
  } else {
    angle = plan ? beamAngle(plan, st) : 25;
    field = angle * (rig.kind === "moving-wash" || rig.kind === "ledpar" || rig.kind === "par" || rig.kind === "fresnel" ? 1.35 : 1.18);
  }
  field = Math.max(field, angle * 1.04);
  rig.halfAngle = Math.max(0.01, (field * D2R) / 2);
  const on = st.bright > 0.004;

  // emissive lens + glow sprite
  const lk = 0.06 + st.bright * 3.2;
  rig.lensMat.color.setRGB(st.r * lk, st.g * lk, st.b * lk);
  rig.label.visible = env.labels;

  // beam origin / direction in world space
  rig.model.lens.getWorldPosition(rig.origin);
  rig.model.lens.getWorldQuaternion(_q);
  rig.dir.set(0, -1, 0).applyQuaternion(_q);

  rig.score = on ? st.bright : 0;
  rig.glow.visible = on;
  if (!on) { rig.beam.mesh.visible = false; rig.pool.visible = false; rig.extra.forEach((x) => (x.mesh.visible = false)); return; }

  const gs = Math.max(0.1, rig.model.lensRadius) * 5.5 * (0.5 + st.bright);
  rig.glow.scale.set(gs, gs, 1);
  (rig.glow.material as THREE.SpriteMaterial).color.setRGB(st.r * st.bright, st.g * st.bright, st.b * st.bright);

  // gobo state
  const gtex = goboTexture(st.goboCap, rig.assets);
  if (st.goboRotSpeed) rig.goboAngle += (st.goboRot - 0.5) * 2 * 2.2 * env.dt;
  else rig.goboAngle = st.goboRot * Math.PI * 2;

  const dy = rig.dir.y;
  const L = 60;
  // floor distance (for the pool decal)
  rig.dist = dy < -0.02 ? Math.min(60, rig.origin.y / -dy) : 18;
  if (rig.origin.y <= 0.02) rig.dist = 12;

  rig.beam.mesh.visible = env.beams;
  if (env.beams) {
    const gain = Math.min(2.4, Math.sqrt(32 / Math.max(3, angle))) * env.crowd * (gtex ? 1.25 : 1);
    const k = st.bright * gain * 0.5;
    tmpC.setRGB(st.r * k, st.g * k, st.b * k);
    rig.beam.update({
      apex: rig.origin, axis: rig.dir, length: L, beamAngle: angle, fieldAngle: field,
      color: tmpC, soft: softness(rig, st), gobo: gtex, goboRot: rig.goboAngle,
    }, env.camPos);
  }

  // prism: two additional beams fanned around the lens X axis
  const wantExtra = env.beams && st.prism;
  if (wantExtra && rig.extra.length === 0) rig.extra = [new BeamVisual(whiteTexture()), new BeamVisual(whiteTexture())];
  rig.extra.forEach((x, i) => {
    x.mesh.visible = wantExtra;
    if (!wantExtra) return;
    const sep = Math.max(3.5, field * 0.55) * D2R * (i === 0 ? 1 : -1);
    _q2.setFromAxisAngle(_ax.set(1, 0, 0).applyQuaternion(_q), sep);
    _d2.copy(rig.dir).applyQuaternion(_q2);
    const k2 = st.bright * Math.min(2.4, Math.sqrt(32 / Math.max(3, angle))) * env.crowd * 0.32;
    tmpC.setRGB(st.r * k2, st.g * k2, st.b * k2);
    x.update({ apex: rig.origin, axis: _d2, length: L, beamAngle: angle, fieldAngle: field, color: tmpC, soft: softness(rig, st), gobo: gtex, goboRot: rig.goboAngle }, env.camPos);
  });

  const hitsFloor = dy < -0.05 && rig.origin.y > 0.02;
  rig.pool.visible = env.pools && hitsFloor && !rig.mapped;
  if (rig.pool.visible) {
    const R = rig.dist * Math.tan(rig.halfAngle);
    _v.copy(rig.origin).addScaledVector(rig.dir, rig.dist);
    rig.pool.position.set(_v.x, 0.012, _v.z);
    rig.pool.rotation.y = Math.atan2(rig.dir.x, rig.dir.z);
    const diam = Math.max(0.15, 2 * R) * 1.2;
    const stretch = Math.min(6, 1 / Math.max(0.17, -dy));
    rig.poolMesh.scale.set(diam, 1, diam * stretch);
    rig.poolMat.map = gtex ?? radialTexture();
    rig.poolMesh.rotation.y = gtex ? -rig.goboAngle : 0;
    const k = Math.min(1, 0.36 * st.bright * Math.min(1.8, 25 / Math.max(2, angle))) * Math.sqrt(env.crowd);
    tmpC.setRGB(st.r * k, st.g * k, st.b * k);
    rig.poolMat.color.copy(tmpC);
  }
}

export function disposeRig(rig: Rig) {
  rig.beam.mat.dispose();
  rig.extra.forEach((x) => x.mat.dispose());
  rig.model.dispose();
  rig.poolMat.dispose();
  (rig.glow.material as THREE.SpriteMaterial).dispose();
  (rig.label.material as THREE.SpriteMaterial).map?.dispose();
  (rig.label.material as THREE.SpriteMaterial).dispose();
}
