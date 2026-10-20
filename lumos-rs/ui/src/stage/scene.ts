// three.js scene manager for the 3D previsualiser / stage editor.
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { TransformControls } from "three/examples/jsm/controls/TransformControls.js";
import { EffectComposer } from "three/examples/jsm/postprocessing/EffectComposer.js";
import { RenderPass } from "three/examples/jsm/postprocessing/RenderPass.js";
import { UnrealBloomPass } from "three/examples/jsm/postprocessing/UnrealBloomPass.js";
import { OutputPass } from "three/examples/jsm/postprocessing/OutputPass.js";
import { ShaderPass } from "three/examples/jsm/postprocessing/ShaderPass.js";
import { FXAAShader } from "three/examples/jsm/shaders/FXAAShader.js";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { dmx } from "../store";
import type { Fixture, FixtureProfile, Stage } from "../types";
import { aimPanTilt } from "../lib/stage-dmx";
import { MediaCache } from "./media";
import { buildStageObject, objSig, type StageObj } from "./objects";
import { applyTransform, buildRig, disposeRig, fxSig, updateRig, type Rig } from "./fixtures";
import { BEAM_Q, BeamPass, shared as beamShared } from "./beams";
import { whiteTexture, goboTexture } from "./gobos";
import { Venue } from "./venue";
import {
  bloomOn, camPreset, gizmoMode, haze, quality, showAudience, showBeams, showGrid, showLabels, showMannequin, showPools, snapOn, stats,
  venueMode, workLight, type CamPreset,
} from "./state";

const D2R = Math.PI / 180;
const N_LIGHTS = 12;

type Q = "low" | "med" | "high";
const QCFG: Record<Q, { pr: number; bloom: number; reflect: boolean; shadows: number; shadowRes: number; fxaa: boolean; audience: boolean }> = {
  low: { pr: 1, bloom: 0, reflect: false, shadows: 0, shadowRes: 512, fxaa: false, audience: false },
  med: { pr: 1.5, bloom: 0.5, reflect: false, shadows: 3, shadowRes: 512, fxaa: true, audience: true },
  high: { pr: 2, bloom: 0.7, reflect: true, shadows: 6, shadowRes: 1024, fxaa: true, audience: true },
};

export type Target = { type: "fixture" | "object"; id: number };
export interface Callbacks {
  onPick(hit: { fixture?: number; object?: number } | null, additive: boolean): void;
  onTransform(t: Target, pos: [number, number, number], rotDeg: [number, number, number], scale: [number, number, number], final: boolean): void;
  onAim(results: { id: number; pan: number; tilt: number }[]): void;
}

export class StageScene {
  readonly renderer: THREE.WebGLRenderer;
  readonly scene = new THREE.Scene();
  readonly camera = new THREE.PerspectiveCamera(45, 1, 0.1, 400);
  readonly controls: OrbitControls;
  private tc: TransformControls;
  private host: HTMLElement;
  private cb: Callbacks;
  private ro: ResizeObserver;
  private media = new MediaCache();

  private ambient = new THREE.AmbientLight(0x9aa4c0, 0.08);
  private hemi = new THREE.HemisphereLight(0x4a5878, 0x101014, 0.22);
  private pickRoot = new THREE.Group();
  private poolRoot = new THREE.Group();
  private venue = new Venue();
  private composer!: EffectComposer;
  private beamPass!: BeamPass;
  private bloom!: UnrealBloomPass;
  private fxaa!: ShaderPass;
  private q: Q = "med";
  private qActive: Q | null = null;
  private software = false;
  private qTimer = { low: 0, high: 0 };
  private lastNow = 0;
  private grid = new THREE.LineSegments();
  private stageEdge = new THREE.LineSegments();
  private rigs = new Map<number, Rig>();
  private objs = new Map<number, StageObj>();
  private lights: THREE.SpotLight[] = [];
  private objHelper = new THREE.BoxHelper(new THREE.Object3D(), 0x3cc4e8);

  private stageSize: [number, number, number] = [0, 0, 0];
  private selFixtures: number[] = [];
  private selObject: number | null = null;
  private target: Target | null = null;
  private dragging = false;
  private draggingTarget: Target | null = null;
  private ray = new THREE.Raycaster();
  private down: { x: number; y: number; onGizmo: boolean; shift: boolean } | null = null;
  private frameN = 0;
  private fpsAcc = { t: performance.now(), n: 0 };
  private anim: { t0: number; dur: number; p0: THREE.Vector3; p1: THREE.Vector3; t0v: THREE.Vector3; t1v: THREE.Vector3 } | null = null;
  private lastPreset: CamPreset | null = null;
  private aiming = false;
  private interacting = false;
  private activeBeams = 0;
  private camMoved = false;
  private disposed = false;

  constructor(host: HTMLElement, cb: Callbacks) {
    this.host = host;
    this.cb = cb;
    this.renderer = new THREE.WebGLRenderer({ antialias: false, powerPreference: "high-performance" });
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.0;
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.renderer.info.autoReset = false;
    try {
      const gl = this.renderer.getContext();
      const ext = gl.getExtension("WEBGL_debug_renderer_info");
      const name = ext ? String(gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : "";
      this.software = /swiftshader|llvmpipe|software|softpipe/i.test(name);
    } catch { /* ignore */ }
    host.appendChild(this.renderer.domElement);
    this.renderer.domElement.style.touchAction = "none";
    this.renderer.domElement.style.display = "block";

    this.scene.background = new THREE.Color("#020204");
    this.scene.fog = new THREE.FogExp2(0x05060a, 0.0);
    this.scene.add(this.ambient, this.hemi);
    {
      const pm = new THREE.PMREMGenerator(this.renderer);
      const envTex = pm.fromScene(new RoomEnvironment(), 0.04).texture;
      this.scene.environment = envTex;
      this.scene.environmentIntensity = 0.14;
      pm.dispose();
    }

    this.scene.add(this.venue.group, this.pickRoot, this.poolRoot);
    this.buildFloor([12, 8, 6]);
    this.grid.raycast = () => undefined;
    this.stageEdge.raycast = () => undefined;
    this.scene.add(this.grid, this.stageEdge);

    for (let i = 0; i < N_LIGHTS; i++) {
      const l = new THREE.SpotLight(0xffffff, 0, 0, 0.3, 0.5, 2);
      if (i < 6) {
        l.castShadow = true;
        l.map = whiteTexture();
        l.shadow.bias = -0.0004;
        l.shadow.normalBias = 0.02;
        l.shadow.radius = 4;
        l.shadow.camera.near = 0.25;
        l.shadow.camera.far = 60;
      }
      this.scene.add(l, l.target);
      this.lights.push(l);
    }

    // HDR pipeline: scene → volumetric beams → bloom → tone-map/sRGB → FXAA
    const rt = new THREE.WebGLRenderTarget(4, 4, { type: THREE.HalfFloatType, depthTexture: new THREE.DepthTexture(4, 4) });
    this.composer = new EffectComposer(this.renderer, rt);
    this.composer.addPass(new RenderPass(this.scene, this.camera));
    this.beamPass = new BeamPass(this.camera);
    this.composer.addPass(this.beamPass);
    this.bloom = new UnrealBloomPass(new THREE.Vector2(4, 4), 0.6, 0.55, 0.9);
    this.composer.addPass(this.bloom);
    this.composer.addPass(new OutputPass());
    this.fxaa = new ShaderPass(FXAAShader);
    this.composer.addPass(this.fxaa);

    this.objHelper.visible = false;
    this.objHelper.raycast = () => undefined;
    this.scene.add(this.objHelper);

    // camera + controls
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.12;
    this.controls.maxPolarAngle = Math.PI * 0.499;
    this.controls.minDistance = 0.8;
    this.controls.maxDistance = 90;
    this.controls.screenSpacePanning = true;
    this.controls.addEventListener("start", () => { this.interacting = true; this.camMoved = false; });
    this.controls.addEventListener("change", () => { if (this.interacting) this.camMoved = true; });
    this.controls.addEventListener("end", () => { this.interacting = false; if (this.camMoved) this.saveCam(true); });

    this.tc = new TransformControls(this.camera, this.renderer.domElement);
    this.tc.size = 0.8;
    this.scene.add(this.tc.getHelper());
    this.tc.addEventListener("dragging-changed", (e) => {
      const v = (e as unknown as { value: boolean }).value;
      this.controls.enabled = !v;
      this.dragging = v;
      this.draggingTarget = v ? this.target : this.draggingTarget;
      if (!v) { this.emitTransform(true); this.draggingTarget = null; }
    });
    this.tc.addEventListener("objectChange", () => this.emitTransform(false));

    const el = this.renderer.domElement;
    el.addEventListener("pointerdown", this.onDown);
    el.addEventListener("pointerup", this.onUp);

    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(host);
    this.resize();
    this.restoreCam();
    this.renderer.setAnimationLoop(this.frame);
  }

  // ── floor / grid ──────────────────────────────────────
  private buildFloor(size: [number, number, number]) {
    const [w, d] = size;
    this.stageSize = size;
    const pts: number[] = [];
    for (let x = Math.ceil(-w / 2); x <= w / 2; x++) pts.push(x, 0.004, -d / 2, x, 0.004, d / 2);
    for (let z = Math.ceil(-d / 2); z <= d / 2; z++) pts.push(-w / 2, 0.004, z, w / 2, 0.004, z);
    this.grid.geometry.dispose();
    this.grid.geometry = new THREE.BufferGeometry().setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    this.grid.material = new THREE.LineBasicMaterial({ color: 0x2b303a, transparent: true, opacity: 0.55 });
    const e = [-w / 2, 0.006, -d / 2, w / 2, 0.006, -d / 2, w / 2, 0.006, -d / 2, w / 2, 0.006, d / 2, w / 2, 0.006, d / 2, -w / 2, 0.006, d / 2, -w / 2, 0.006, d / 2, -w / 2, 0.006, -d / 2];
    this.stageEdge.geometry.dispose();
    this.stageEdge.geometry = new THREE.BufferGeometry().setAttribute("position", new THREE.Float32BufferAttribute(e, 3));
    this.stageEdge.material = new THREE.LineBasicMaterial({ color: 0x3cc4e8, transparent: true, opacity: 0.6 });
  }

  // ── data sync ─────────────────────────────────────────
  setStage(stage: Stage, hidden: number[]) {
    const s = stage.size;
    if (s[0] !== this.stageSize[0] || s[1] !== this.stageSize[1] || s[2] !== this.stageSize[2]) this.buildFloor([s[0], s[1], s[2]]);
    const hid = new Set(hidden);
    const seen = new Set<number>();
    for (const o of stage.objects) {
      seen.add(o.id);
      let so = this.objs.get(o.id);
      const sig = objSig(o);
      if (so && so.sig !== sig) {
        this.pickRoot.remove(so.group);
        so.dispose();
        this.objs.delete(o.id);
        so = undefined;
      }
      if (!so) {
        so = buildStageObject(o);
        this.objs.set(o.id, so);
        this.pickRoot.add(so.group);
        if (this.selObject === o.id) this.refreshGizmo();
      }
      const draggingThis = this.dragging && this.draggingTarget?.type === "object" && this.draggingTarget.id === o.id;
      if (!draggingThis) {
        so.group.position.set(o.pos[0], o.pos[1], o.pos[2]);
        so.group.rotation.order = "YXZ";
        so.group.rotation.set(o.rot[0] * D2R, o.rot[1] * D2R, o.rot[2] * D2R);
        so.group.scale.set(1, 1, 1);
      }
      so.group.visible = !hid.has(o.id);
    }
    for (const [id, so] of this.objs) {
      if (!seen.has(id)) {
        this.pickRoot.remove(so.group);
        so.dispose();
        this.objs.delete(id);
        if (this.selObject === id) { this.selObject = null; this.refreshGizmo(); }
      }
    }
    this.updateHelper();
  }

  setFixtures(fixtures: Fixture[], library: FixtureProfile[]) {
    const seen = new Set<number>();
    const profs = new Map(library.map((p) => [p.id, p]));
    for (const fx of fixtures) {
      seen.add(fx.id);
      const prof = profs.get(fx.profile);
      let rig = this.rigs.get(fx.id);
      if (rig && (rig.sig !== fxSig(fx, prof) || (rig.plan && rig.plan.prof !== prof))) {
        this.removeRig(fx.id);
        rig = undefined;
      }
      if (!rig) {
        rig = buildRig(fx, prof);
        this.rigs.set(fx.id, rig);
        this.pickRoot.add(rig.root);
        this.poolRoot.add(rig.pool);
        this.beamPass.scene.add(rig.beam.mesh);
        rig.sel.visible = this.selFixtures.includes(fx.id);
        if (this.selFixtures.length === 1 && this.selFixtures[0] === fx.id) this.refreshGizmo();
      } else {
        const draggingThis = this.dragging && this.draggingTarget?.type === "fixture" && this.draggingTarget.id === fx.id;
        if (draggingThis) rig.fx = fx; else applyTransform(rig, fx);
      }
    }
    for (const id of [...this.rigs.keys()]) if (!seen.has(id)) this.removeRig(id);
  }

  private removeRig(id: number) {
    const rig = this.rigs.get(id);
    if (!rig) return;
    this.pickRoot.remove(rig.root);
    this.poolRoot.remove(rig.pool);
    this.beamPass.scene.remove(rig.beam.mesh);
    disposeRig(rig);
    this.rigs.delete(id);
    if (this.target?.type === "fixture" && this.target.id === id) this.refreshGizmo();
  }

  setSelection(fixtures: number[], object: number | null) {
    this.selFixtures = fixtures;
    this.selObject = object;
    for (const [id, r] of this.rigs) r.sel.visible = fixtures.includes(id);
    this.refreshGizmo();
    this.updateHelper();
  }

  /** Enable gizmo only when editing. */
  setEditing(on: boolean) {
    this.editing = on;
    this.refreshGizmo();
  }
  private editing = true;

  setAim(on: boolean) { this.aiming = on; this.renderer.domElement.style.cursor = on ? "crosshair" : ""; this.refreshGizmo(); }

  private refreshGizmo() {
    let t: Target | null = null;
    if (this.selObject !== null && this.objs.has(this.selObject)) t = { type: "object", id: this.selObject };
    else if (this.selFixtures.length === 1 && this.rigs.has(this.selFixtures[0])) t = { type: "fixture", id: this.selFixtures[0] };
    this.target = t;
    if (!t || !this.editing || this.aiming) { this.tc.detach(); return; }
    const obj = t.type === "object" ? this.objs.get(t.id)!.group : this.rigs.get(t.id)!.root;
    this.tc.attach(obj);
    this.applyGizmoMode();
  }

  applyGizmoMode() {
    const m = gizmoMode.value;
    this.tc.setMode(this.target?.type === "fixture" && m === "scale" ? "translate" : m);
    const snap = snapOn.value;
    this.tc.setTranslationSnap(snap ? 0.1 : null);
    this.tc.setRotationSnap(snap ? 15 * D2R : null);
    this.tc.setScaleSnap(snap ? 0.1 : null);
  }

  private updateHelper() {
    const so = this.selObject !== null ? this.objs.get(this.selObject) : undefined;
    if (so && this.editing) { this.objHelper.setFromObject(so.group); this.objHelper.visible = true; }
    else this.objHelper.visible = false;
  }

  private emitTransform(final: boolean) {
    const t = this.draggingTarget ?? this.target;
    if (!t) return;
    const obj = t.type === "object" ? this.objs.get(t.id)?.group : this.rigs.get(t.id)?.root;
    if (!obj) return;
    const r = (n: number) => Math.round(n * 1000) / 1000;
    const pos: [number, number, number] = [r(obj.position.x), r(obj.position.y), r(obj.position.z)];
    const rot: [number, number, number] = [r(obj.rotation.x / D2R), r(obj.rotation.y / D2R), r(obj.rotation.z / D2R)];
    const sc: [number, number, number] = [obj.scale.x, obj.scale.y, obj.scale.z];
    this.cb.onTransform(t, pos, rot, sc, final);
    this.updateHelper();
  }

  // ── camera ────────────────────────────────────────────
  private presetPose(p: CamPreset): { pos: THREE.Vector3; target: THREE.Vector3 } {
    const [w, d, h] = this.stageSize;
    const span = Math.max(w, d);
    switch (p) {
      case "audience": return { pos: new THREE.Vector3(0, 1.8, d / 2 + Math.max(7, span * 0.75)), target: new THREE.Vector3(0, h * 0.3, 0) };
      case "top": return { pos: new THREE.Vector3(0, span * 1.3 + h, 0.01), target: new THREE.Vector3(0, 0, 0) };
      case "side": return { pos: new THREE.Vector3(w / 2 + Math.max(8, span * 0.8), h * 0.45, 0), target: new THREE.Vector3(0, h * 0.3, 0) };
      default: return { pos: new THREE.Vector3(w * 0.75, h * 1.0, d / 2 + span * 0.55 + 3), target: new THREE.Vector3(0, h * 0.25, 0) };
    }
  }

  goPreset(p: CamPreset, animate = true) {
    if (p === "free") return;
    const { pos, target } = this.presetPose(p);
    if (!animate) { this.camera.position.copy(pos); this.controls.target.copy(target); this.controls.update(); return; }
    this.anim = { t0: performance.now(), dur: 650, p0: this.camera.position.clone(), p1: pos, t0v: this.controls.target.clone(), t1v: target };
  }

  private saveCam(manual: boolean) {
    if (manual && !this.anim) camPreset.value = "free";
    try {
      localStorage.setItem("stage.cam", JSON.stringify({ p: this.camera.position.toArray(), t: this.controls.target.toArray() }));
    } catch { /* ignore */ }
  }

  private restoreCam() {
    try {
      const raw = localStorage.getItem("stage.cam");
      if (raw && camPreset.value === "free") {
        const j = JSON.parse(raw) as { p: number[]; t: number[] };
        this.camera.position.fromArray(j.p);
        this.controls.target.fromArray(j.t);
        this.controls.update();
        this.lastPreset = "free";
        return;
      }
    } catch { /* ignore */ }
    this.goPreset(camPreset.value === "free" ? "iso" : camPreset.value, false);
    this.lastPreset = camPreset.value;
  }

  private resize() {
    const w = Math.max(1, this.host.clientWidth), h = Math.max(1, this.host.clientHeight);
    this.size = [w, h];
    this.applySize();
  }

  private size: [number, number] = [1, 1];
  private applySize() {
    const [w, h] = this.size;
    const pr = QCFG[this.qActive ?? "med"].pr;
    const ratio = Math.min(window.devicePixelRatio || 1, pr);
    this.renderer.setPixelRatio(ratio);
    this.renderer.setSize(w, h, false);
    this.composer.setPixelRatio(ratio);
    this.composer.setSize(w, h);
    this.renderer.domElement.style.width = "100%";
    this.renderer.domElement.style.height = "100%";
    this.fxaa.material.uniforms["resolution"].value.set(1 / (w * ratio), 1 / (h * ratio));
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  // ── picking ───────────────────────────────────────────
  private ndc(e: PointerEvent) {
    const r = this.renderer.domElement.getBoundingClientRect();
    return new THREE.Vector2(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1);
  }

  private onDown = (e: PointerEvent) => {
    this.down = { x: e.clientX, y: e.clientY, onGizmo: this.tc.axis !== null && this.tc.object !== undefined, shift: e.shiftKey || e.ctrlKey || e.metaKey };
  };

  private onUp = (e: PointerEvent) => {
    const d = this.down;
    this.down = null;
    if (!d || d.onGizmo || this.dragging) return;
    if (Math.hypot(e.clientX - d.x, e.clientY - d.y) > 5) return;
    this.ray.setFromCamera(this.ndc(e), this.camera);
    const hits = this.ray.intersectObjects(this.pickRoot.children, true).filter((h) => {
      let o: THREE.Object3D | null = h.object;
      while (o) { if (!o.visible) return false; o = o.parent; }
      return true;
    });
    if (this.aiming) {
      const hit = hits.find((h) => { for (let n: THREE.Object3D | null = h.object; n; n = n.parent) if (n.userData.fixtureId !== undefined) return false; return true; });
      let pt: THREE.Vector3 | null = hit ? hit.point.clone() : null;
      if (!pt) {
        const plane = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
        pt = this.ray.ray.intersectPlane(plane, new THREE.Vector3());
      }
      if (pt) this.aimAt(pt);
      return;
    }
    for (const h of hits) {
      let f: number | undefined, o: number | undefined;
      for (let n: THREE.Object3D | null = h.object; n && f === undefined && o === undefined; n = n.parent) {
        f = n.userData.fixtureId as number | undefined;
        o = n.userData.objectId as number | undefined;
      }
      if (f !== undefined) return this.cb.onPick({ fixture: f }, d.shift);
      if (o !== undefined) return this.cb.onPick({ object: o }, d.shift);
    }
    this.cb.onPick(null, d.shift);
  };

  private aimAt(p: THREE.Vector3) {
    const out: { id: number; pan: number; tilt: number }[] = [];
    const piv = new THREE.Vector3(), inv = new THREE.Quaternion(), v = new THREE.Vector3();
    for (const id of this.selFixtures) {
      const rig = this.rigs.get(id);
      if (!rig?.plan?.moving || !rig.panG) continue;
      rig.root.updateMatrixWorld(true);
      rig.panG.getWorldPosition(piv);
      rig.root.getWorldQuaternion(inv).invert();
      v.copy(p).sub(piv).applyQuaternion(inv).normalize();
      const { pan, tilt } = aimPanTilt(rig.plan, v.x, v.y, v.z);
      out.push({ id, pan: Math.round(pan * 1e4) / 1e4, tilt: Math.round(tilt * 1e4) / 1e4 });
    }
    this.cb.onAim(out);
  }

  /** Spread world position of a fixture (for UI helpers). */
  fixtureBeamInfo(id: number) {
    const r = this.rigs.get(id);
    return r ? { origin: r.origin.clone(), dir: r.dir.clone(), bright: r.state.bright } : null;
  }

  /** Screenshot helper for tests. */
  snapshot(): string { this.composer.render(); return this.renderer.domElement.toDataURL("image/png"); }

  // ── quality ───────────────────────────────────────────
  private effectiveQuality(): Q {
    const pref = quality.value;
    if (pref !== "auto") return pref;
    return this.q;
  }

  private applyQuality(q: Q) {
    if (this.qActive === q) return;
    const c = QCFG[q];
    this.qActive = q;
    this.q = q;
    this.beamPass.setQuality(BEAM_Q[q]);
    this.bloom.enabled = bloomOn.value && c.bloom > 0;
    this.bloom.strength = c.bloom;
    this.fxaa.enabled = c.fxaa;
    this.lights.forEach((l, i) => {
      if (i < 6) {
        const on = i < c.shadows;
        l.castShadow = on;
        if (l.shadow.mapSize.x !== c.shadowRes) { l.shadow.mapSize.set(c.shadowRes, c.shadowRes); l.shadow.map?.dispose(); l.shadow.map = null; }
      }
    });
    this.applySize();
  }

  // ── frame loop ────────────────────────────────────────
  private frame = (now: number) => {
    if (this.disposed) return;
    this.frameN++;
    const tSec = now / 1000;
    const dt = Math.min(0.1, Math.max(0.001, (now - (this.lastNow || now - 16)) / 1000));
    this.lastNow = now;
    this.renderer.info.reset();

    if (this.anim) {
      const k = Math.min(1, (now - this.anim.t0) / this.anim.dur);
      const e = k < 0.5 ? 2 * k * k : 1 - Math.pow(-2 * k + 2, 2) / 2;
      this.camera.position.lerpVectors(this.anim.p0, this.anim.p1, e);
      this.controls.target.lerpVectors(this.anim.t0v, this.anim.t1v, e);
      if (k >= 1) { this.anim = null; this.saveCam(false); }
    }
    const preset = camPreset.value;
    if (preset !== this.lastPreset) { this.lastPreset = preset; if (preset !== "free") this.goPreset(preset); }
    this.controls.update();
    this.camera.updateMatrixWorld();

    // quality + venue
    const pref = quality.value;
    if (pref === "auto" && this.qActive === null) this.q = this.software ? "med" : "high";
    const q = pref === "auto" ? this.q : pref;
    this.applyQuality(q);
    this.bloom.enabled = bloomOn.value && QCFG[q].bloom > 0;
    const qc = QCFG[q];
    this.venue.build({
      size: this.stageSize, mode: venueMode.value, audience: showAudience.value && qc.audience, mannequin: showMannequin.value,
      reflect: qc.reflect, reflectRes: 512,
    });

    const hz = haze.value;
    (this.scene.fog as THREE.FogExp2).density = hz * 0.006;
    beamShared.uHaze.value = 0.1 + hz * 0.95;
    beamShared.uTime.value = tSec;
    this.grid.visible = showGrid.value;
    this.ambient.intensity = workLight.value ? 2.4 : 0.08;
    this.hemi.intensity = workLight.value ? 1.2 : 0.22;
    this.applyGizmoModeIfChanged();

    const env = {
      crowd: Math.max(0.35, Math.min(1, Math.sqrt(14 / Math.max(1, this.activeBeams)))), haze: hz, beams: showBeams.value,
      pools: showPools.value, labels: showLabels.value, time: tSec, dt, camPos: this.camera.position,
    };
    let active = 0;
    for (const rig of this.rigs.values()) { updateRig(rig, dmx.get(rig.fx.universe), env); if (rig.score > 0.02) active++; }
    this.activeBeams = active;
    this.beamPass.enabled = showBeams.value;

    if (this.frameN % 2 === 0) this.assignLights(qc.shadows);
    this.media.tick(now);
    for (const so of this.objs.values()) so.tick?.(now, this.media);
    if (this.objHelper.visible) this.updateHelper();

    this.composer.render(dt);

    this.fpsAcc.n++;
    if (now - this.fpsAcc.t > 500) {
      const fps = Math.round((this.fpsAcc.n * 1000) / (now - this.fpsAcc.t));
      const lit = this.lights.filter((l) => l.intensity > 0).length;
      stats.value = { fps, calls: this.renderer.info.render.calls, lights: lit, q };
      this.fpsAcc = { t: now, n: 0 };
      // adaptive quality (not on software renderers — their fps says nothing about real hardware)
      if (pref === "auto" && !this.software && document.visibilityState === "visible") {
        if (fps < 28) { this.qTimer.low++; this.qTimer.high = 0; }
        else if (fps > 56) { this.qTimer.high++; this.qTimer.low = 0; }
        else { this.qTimer.low = 0; this.qTimer.high = 0; }
        if (this.qTimer.low >= 4) { this.q = q === "high" ? "med" : "low"; this.qTimer.low = 0; }
        if (this.qTimer.high >= 24) { this.q = q === "low" ? "med" : "high"; this.qTimer.high = 0; }
      }
    }
  };

  private lastGizmo = "";
  private applyGizmoModeIfChanged() {
    const k = gizmoMode.value + snapOn.value + String(this.target?.type);
    if (k !== this.lastGizmo) { this.lastGizmo = k; this.applyGizmoMode(); }
  }

  private assignLights(shadowCount: number) {
    const lit = [...this.rigs.values()].filter((r) => r.score > 0.02).sort((a, b) => b.score - a.score);
    // moving heads with gobos are the best candidates for the shadow/gobo-mapped lights
    const mappedPool = lit.filter((r) => r.state.goboCap || r.plan?.moving).concat(lit.filter((r) => !(r.state.goboCap || r.plan?.moving)));
    const order: typeof lit = [];
    const taken = new Set<number>();
    for (const r of mappedPool.slice(0, shadowCount)) { order.push(r); taken.add(r.fx.id); }
    for (const r of lit) { if (order.length >= N_LIGHTS) break; if (!taken.has(r.fx.id)) order.push(r); }
    for (const r of this.rigs.values()) r.mapped = false;
    for (let i = 0; i < N_LIGHTS; i++) {
      const l = this.lights[i];
      const r = order[i];
      if (!r) { l.intensity = 0; continue; }
      const mapped = i < shadowCount;
      l.position.copy(r.origin);
      l.target.position.copy(r.origin).addScaledVector(r.dir, Math.min(r.dist, 40));
      l.target.updateMatrixWorld();
      l.angle = Math.min(1.3, r.halfAngle * 1.02);
      l.penumbra = r.state.goboCap ? 0.12 : 0.5;
      l.color.setRGB(r.state.r, r.state.g, r.state.b);
      const spread = Math.max(6, (r.halfAngle / D2R) * 2);
      l.intensity = r.state.bright * 230 * Math.min(2.5, 30 / spread) * (mapped ? 1 : 0.7);
      if (i < 6) {
        const tex = goboTexture(r.state.goboCap, r.assets);
        l.map = tex ?? whiteTexture();
        r.mapped = mapped;
      }
    }
  }

  dispose() {
    this.disposed = true;
    this.renderer.setAnimationLoop(null);
    this.ro.disconnect();
    const el = this.renderer.domElement;
    el.removeEventListener("pointerdown", this.onDown);
    el.removeEventListener("pointerup", this.onUp);
    this.tc.dispose();
    this.controls.dispose();
    for (const r of this.rigs.values()) disposeRig(r);
    for (const o of this.objs.values()) o.dispose();
    this.media.dispose();
    this.venue.dispose();
    this.beamPass.dispose();
    this.composer.dispose();
    this.renderer.dispose();
    el.remove();
  }
}
