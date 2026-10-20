// Volumetric light beams: analytic cone ray-marching, scene-depth aware, gobo-modulated, haze-noise scattering.
// Rendered in a separate (reduced-resolution) target and added onto the HDR scene before bloom.
import * as THREE from "three";
import { FullScreenQuad, Pass } from "three/examples/jsm/postprocessing/Pass.js";

export interface BeamQuality { steps: number; scale: number; noise: number }
export const BEAM_Q: Record<"low" | "med" | "high", BeamQuality> = {
  low: { steps: 8, scale: 0.4, noise: 0 },
  med: { steps: 14, scale: 0.55, noise: 1 },
  high: { steps: 22, scale: 0.75, noise: 2 },
};

/** Uniforms shared by every beam material (same object refs → updated once per frame). */
export const shared = {
  uDepth: { value: null as THREE.Texture | null },
  uRes: { value: new THREE.Vector2(1, 1) },
  uInvProj: { value: new THREE.Matrix4() },
  uTime: { value: 0 },
  uSteps: { value: 14 },
  uNoise: { value: 1 },
  uHaze: { value: 0.35 },
};

const VERT = /* glsl */ `
varying vec3 vWorld;
void main(){
  vec4 w = modelMatrix * vec4(position, 1.0);
  vWorld = w.xyz;
  gl_Position = projectionMatrix * viewMatrix * w;
}`;

const FRAG = /* glsl */ `
precision highp float;
uniform sampler2D uDepth; uniform vec2 uRes; uniform mat4 uInvProj; uniform float uTime;
uniform float uSteps; uniform float uNoise; uniform float uHaze;
uniform vec3 uApex; uniform vec3 uAxis; uniform vec3 uRight; uniform vec3 uUp;
uniform float uCos2; uniform float uTanOuter; uniform float uTanInner; uniform float uLen; uniform float uSoft; uniform float uAtten;
uniform vec3 uColor; uniform float uInside;
uniform sampler2D uGobo; uniform float uGoboOn; uniform float uGoboRot;
varying vec3 vWorld;

float hash(vec3 p){ p = fract(p * 0.3183099 + 0.1); p *= 17.0; return fract(p.x * p.y * p.z * (p.x + p.y + p.z)); }
float vnoise(vec3 x){
  vec3 i = floor(x), f = fract(x); f = f*f*(3.0-2.0*f);
  return mix(mix(mix(hash(i+vec3(0,0,0)), hash(i+vec3(1,0,0)), f.x), mix(hash(i+vec3(0,1,0)), hash(i+vec3(1,1,0)), f.x), f.y),
             mix(mix(hash(i+vec3(0,0,1)), hash(i+vec3(1,0,1)), f.x), mix(hash(i+vec3(0,1,1)), hash(i+vec3(1,1,1)), f.x), f.y), f.z);
}

vec2 coneInterval(vec3 o, vec3 d){
  vec3 co = o - uApex;
  float dA = dot(d, uAxis), cA = dot(co, uAxis);
  float qa = dA*dA - uCos2;
  float qb = 2.0*(dA*cA - uCos2*dot(d, co));
  float qc = cA*cA - uCos2*dot(co, co);
  float tin = 0.0, tout = 1e6;
  if (abs(dA) > 1e-6) { float ta = -cA/dA, tb = (uLen - cA)/dA; tin = max(tin, min(ta, tb)); tout = min(tout, max(ta, tb)); }
  else if (cA < 0.0 || cA > uLen) return vec2(1.0, 0.0);
  float disc = qb*qb - 4.0*qa*qc;
  if (abs(qa) < 1e-7) {
    if (abs(qb) < 1e-9) { if (qc < 0.0) return vec2(1.0, 0.0); }
    else { float r = -qc/qb; if (qb > 0.0) tin = max(tin, r); else tout = min(tout, r); }
  } else if (qa < 0.0) {
    if (disc < 0.0) return vec2(1.0, 0.0);
    float s = sqrt(disc); float r1 = (-qb - s)/(2.0*qa), r2 = (-qb + s)/(2.0*qa);
    tin = max(tin, min(r1, r2)); tout = min(tout, max(r1, r2));
  } else if (disc >= 0.0) {
    float s = sqrt(disc); float r1 = (-qb - s)/(2.0*qa), r2 = (-qb + s)/(2.0*qa);
    if (dA > 0.0) tin = max(tin, r2); else tout = min(tout, r1);
  }
  return vec2(tin, tout);
}

void main(){
  vec3 o = cameraPosition;
  vec3 d = normalize(vWorld - o);
  vec2 iv = coneInterval(o, d);
  // scene depth along this ray
  vec2 suv = gl_FragCoord.xy / uRes;
  float dz = texture2D(uDepth, suv).x;
  float sceneT = 1e6;
  if (dz < 0.99999) { vec4 c = vec4(suv * 2.0 - 1.0, dz * 2.0 - 1.0, 1.0); vec4 v = uInvProj * c; v /= v.w; sceneT = length(v.xyz); }
  float tin = max(iv.x, 0.0), tout = min(iv.y, sceneT);
  if (tout <= tin) discard;
  tout = min(tout, tin + 60.0);
  float dt = (tout - tin) / uSteps;
  float jit = fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
  float acc = 0.0;
  float k = uTanInner / uTanOuter;
  float edge0 = k * mix(0.97, 0.12, uSoft);
  float ca = cos(uGoboRot), sa = sin(uGoboRot);
  for (int i = 0; i < 40; i++) {
    if (float(i) >= uSteps) break;
    float t = tin + (float(i) + jit) * dt;
    vec3 p = o + d * t;
    vec3 q = p - uApex;
    float h = dot(q, uAxis);
    if (h <= 0.0) continue;
    vec3 rv = q - h * uAxis;
    float R = h * uTanOuter;
    float s = length(rv) / R;
    if (s >= 1.0) continue;
    float prof = 1.0 - smoothstep(edge0, 1.0, s);
    prof = prof * prof * (1.5 - 0.5 * prof);
    float atten = smoothstep(0.0, 0.35, h) / (1.0 + h * h * uAtten);
    float g = 1.0;
    if (uGoboOn > 0.5) {
      vec2 uv = vec2(dot(rv, uRight), dot(rv, uUp)) / R;
      uv = vec2(ca * uv.x - sa * uv.y, sa * uv.x + ca * uv.y) * 0.5 + 0.5;
      g = mix(0.12, 1.0, texture2D(uGobo, uv).r);
    }
    float hz = 1.0;
    if (uNoise > 0.5) {
      vec3 np = p * 0.55 + vec3(0.0, -uTime * 0.07, uTime * 0.03);
      float n = vnoise(np) * 0.65 + (uNoise > 1.5 ? vnoise(np * 2.3 + 7.0) * 0.35 : 0.35 * 0.5);
      hz = 0.45 + 1.1 * n;
    }
    acc += prof * atten * g * hz;
  }
  acc *= dt * uHaze;
  vec3 col = uColor * acc;
  gl_FragColor = vec4(col, 1.0);
}`;

// Unit cone: apex at origin, base (radius 1) at y = -1, closed.
let coneGeo: THREE.BufferGeometry | null = null;
function proxyGeometry(): THREE.BufferGeometry {
  if (coneGeo) return coneGeo;
  const g = new THREE.CylinderGeometry(0.0005, 1, 1, 28, 1, false);
  g.translate(0, -0.5, 0);
  coneGeo = g;
  return g;
}

export interface BeamParams {
  apex: THREE.Vector3;
  axis: THREE.Vector3;       // unit, direction of travel
  length: number;
  /** full angles in degrees */
  beamAngle: number;
  fieldAngle: number;
  color: THREE.Color;        // linear colour × intensity (may exceed 1)
  soft: number;              // 0 sharp … 1 very soft
  gobo: THREE.Texture | null;
  goboRot: number;           // radians
}

const tmpQ = new THREE.Quaternion();
const DOWN = new THREE.Vector3(0, -1, 0);
const tmpA = new THREE.Vector3();
const tmpB = new THREE.Vector3();

export class BeamVisual {
  readonly mesh: THREE.Mesh;
  readonly mat: THREE.ShaderMaterial;
  private white: THREE.Texture;
  constructor(white: THREE.Texture) {
    this.white = white;
    this.mat = new THREE.ShaderMaterial({
      vertexShader: VERT,
      fragmentShader: FRAG,
      transparent: true,
      depthTest: false,
      depthWrite: false,
      blending: THREE.CustomBlending,
      blendEquation: THREE.AddEquation,
      blendSrc: THREE.OneFactor,
      blendDst: THREE.OneFactor,
      uniforms: {
        ...shared,
        uApex: { value: new THREE.Vector3() }, uAxis: { value: new THREE.Vector3(0, -1, 0) },
        uRight: { value: new THREE.Vector3(1, 0, 0) }, uUp: { value: new THREE.Vector3(0, 0, 1) },
        uCos2: { value: 0.9 }, uTanOuter: { value: 0.2 }, uTanInner: { value: 0.15 }, uLen: { value: 10 },
        uSoft: { value: 0.3 }, uAtten: { value: 0.012 }, uColor: { value: new THREE.Color(1, 1, 1) }, uInside: { value: 0 },
        uGobo: { value: white }, uGoboOn: { value: 0 }, uGoboRot: { value: 0 },
      },
    });
    this.mesh = new THREE.Mesh(proxyGeometry(), this.mat);
    this.mesh.frustumCulled = false;
    this.mesh.visible = false;
    this.mesh.matrixAutoUpdate = false;
  }

  update(p: BeamParams, camPos: THREE.Vector3) {
    const u = this.mat.uniforms;
    const half = THREE.MathUtils.degToRad(Math.max(0.5, p.fieldAngle) / 2);
    const halfProxy = Math.min(1.45, half * 1.12 + 0.01);
    const tp = Math.tan(halfProxy);
    u.uApex.value.copy(p.apex);
    u.uAxis.value.copy(p.axis);
    // perpendicular basis for gobo projection (stable)
    tmpA.set(0, 0, 1);
    if (Math.abs(p.axis.dot(tmpA)) > 0.95) tmpA.set(1, 0, 0);
    tmpB.crossVectors(p.axis, tmpA).normalize();   // right
    tmpA.crossVectors(tmpB, p.axis).normalize();   // up
    u.uRight.value.copy(tmpB);
    u.uUp.value.copy(tmpA);
    u.uCos2.value = Math.cos(halfProxy) ** 2;
    u.uTanOuter.value = Math.tan(half);
    u.uTanInner.value = Math.tan(THREE.MathUtils.degToRad(Math.max(0.3, Math.min(p.beamAngle, p.fieldAngle)) / 2));
    u.uLen.value = p.length;
    u.uSoft.value = p.soft;
    u.uAtten.value = 0.5 / (p.length * p.length + 25) * 6;
    (u.uColor.value as THREE.Color).copy(p.color);
    if (p.gobo) { u.uGobo.value = p.gobo; u.uGoboOn.value = 1; } else { u.uGobo.value = this.white; u.uGoboOn.value = 0; }
    u.uGoboRot.value = p.goboRot;
    // proxy transform
    const rBase = p.length * tp;
    tmpQ.setFromUnitVectors(DOWN, p.axis);
    this.mesh.matrix.compose(p.apex, tmpQ, tmpB.set(rBase, p.length, rBase));
    this.mesh.matrixWorld.copy(this.mesh.matrix);
    // camera inside the proxy cone → render back faces
    tmpA.copy(camPos).sub(p.apex);
    const h = tmpA.dot(p.axis);
    const inside = h > 0 && h < p.length && tmpA.lengthSq() - h * h < (h * tp) ** 2;
    this.mat.side = inside ? THREE.BackSide : THREE.FrontSide;
    u.uInside.value = inside ? 1 : 0;
  }
}

/** Beam scene + render-to-target + additive composite into the HDR scene buffer. */
export class BeamPass extends Pass {
  readonly scene = new THREE.Scene();
  private rt: THREE.WebGLRenderTarget;
  private quad: FullScreenQuad;
  private mat: THREE.ShaderMaterial;
  private camera: THREE.PerspectiveCamera;
  private scale = 0.55;
  enabled = true;

  constructor(camera: THREE.PerspectiveCamera) {
    super();
    this.camera = camera;
    this.needsSwap = false;
    this.rt = new THREE.WebGLRenderTarget(4, 4, { type: THREE.HalfFloatType, depthBuffer: false, minFilter: THREE.LinearFilter, magFilter: THREE.LinearFilter });
    this.mat = new THREE.ShaderMaterial({
      uniforms: { tBeam: { value: this.rt.texture } },
      vertexShader: "varying vec2 vUv; void main(){ vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }",
      fragmentShader: "uniform sampler2D tBeam; varying vec2 vUv; void main(){ gl_FragColor = vec4(texture2D(tBeam, vUv).rgb, 1.0); }",
      transparent: true, depthTest: false, depthWrite: false,
      blending: THREE.CustomBlending, blendEquation: THREE.AddEquation, blendSrc: THREE.OneFactor, blendDst: THREE.OneFactor,
    });
    this.quad = new FullScreenQuad(this.mat);
  }

  setQuality(q: BeamQuality) {
    this.scale = q.scale;
    shared.uSteps.value = q.steps;
    shared.uNoise.value = q.noise;
  }

  override setSize(w: number, h: number) {
    this.rt.setSize(Math.max(2, Math.round(w * this.scale)), Math.max(2, Math.round(h * this.scale)));
    shared.uRes.value.set(this.rt.width, this.rt.height);
  }

  override render(renderer: THREE.WebGLRenderer, _write: THREE.WebGLRenderTarget, read: THREE.WebGLRenderTarget) {
    if (!this.enabled || this.scene.children.every((c) => !c.visible)) return;
    // keep RT size in sync with the quality scale
    const full = read;
    const tw = Math.max(2, Math.round(full.width * this.scale)), th = Math.max(2, Math.round(full.height * this.scale));
    if (this.rt.width !== tw || this.rt.height !== th) { this.rt.setSize(tw, th); shared.uRes.value.set(tw, th); }
    shared.uDepth.value = read.depthTexture;
    shared.uInvProj.value.copy(this.camera.projectionMatrixInverse);
    const prev = renderer.getRenderTarget();
    const ac = renderer.autoClear;
    renderer.autoClear = true;
    renderer.setRenderTarget(this.rt);
    renderer.setClearColor(0x000000, 0);
    renderer.clear(true, false, false);
    renderer.render(this.scene, this.camera);
    renderer.autoClear = false;
    renderer.setRenderTarget(read);
    this.quad.render(renderer);
    renderer.autoClear = ac;
    renderer.setRenderTarget(prev);
  }

  dispose() { this.rt.dispose(); this.mat.dispose(); this.quad.dispose(); }
}
