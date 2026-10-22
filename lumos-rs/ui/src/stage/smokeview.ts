// GPU side of the smoke simulation: density 3D texture, shared GLSL (used by the beam ray-march and the plume pass),
// and the "plume" volume pass that makes smoke visible where no beam crosses it.
import * as THREE from "three";
import { GRIDS, type SmokeSim } from "./smoke";

export const NOISE_GLSL = /* glsl */ `
float hash(vec3 p){ p = fract(p * 0.3183099 + 0.1); p *= 17.0; return fract(p.x * p.y * p.z * (p.x + p.y + p.z)); }
float vnoise(vec3 x){
  vec3 i = floor(x), f = fract(x); f = f*f*(3.0-2.0*f);
  return mix(mix(mix(hash(i+vec3(0,0,0)), hash(i+vec3(1,0,0)), f.x), mix(hash(i+vec3(0,1,0)), hash(i+vec3(1,1,0)), f.x), f.y),
             mix(mix(hash(i+vec3(0,0,1)), hash(i+vec3(1,0,1)), f.x), mix(hash(i+vec3(0,1,1)), hash(i+vec3(1,1,1)), f.x), f.y), f.z);
}`;

/** Smoke density lookup with sub-voxel wisps: animated noise warps the lookup and modulates dense regions. */
export const SMOKE_GLSL = /* glsl */ `
uniform sampler3D uSmoke; uniform vec3 uSmokeMin; uniform vec3 uSmokeInv; uniform float uSmokeOn;
float smokeAt(vec3 p){
  if (uSmokeOn < 0.5) return 0.0;
  vec3 flow = vec3(0.0, -uTime * 0.05, uTime * 0.03);
  vec3 q = p;
  if (uNoise > 0.5) q += 0.5 * (vec3(vnoise(p * 0.75 + flow + 3.1), vnoise(p * 0.75 + flow + 17.7), vnoise(p * 0.75 + flow + 9.3)) - 0.5);
  vec3 uvw = (q - uSmokeMin) * uSmokeInv;
  if (uvw.x < 0.0 || uvw.y < 0.0 || uvw.z < 0.0 || uvw.x > 1.0 || uvw.y > 1.0 || uvw.z > 1.0) return 0.0;
  float s = texture(uSmoke, uvw).r;
  float d = s * s * 1.5;
  if (d < 0.0008) return 0.0;
  if (uNoise > 0.5) {
    float det = vnoise(q * 2.3 + flow * 2.0) * 0.62 + vnoise(q * 5.1 - flow * 3.0 + 4.0) * 0.38;
    d *= mix(0.9, 0.25 + 1.55 * det, clamp(d * 4.0, 0.0, 1.0));
  }
  return d;
}`;

/** Henyey-Greenstein phase (g ≈ 0.65), scaled so the back-scatter level stays near the old uniform look. */
export const PHASE_GLSL = /* glsl */ `
float phaseHG(float c){ const float g = 0.65; return 0.3 + 0.07 * (1.0 - g * g) / pow(1.0 + g * g - 2.0 * g * c, 1.5); }`;

// ── texture ──────────────────────────────────────────────
export class SmokeTexture {
  tex: THREE.Data3DTexture | null = null;
  private data = new Uint8Array(0);
  private ver = -1;
  private dims = "";
  /** Upload the density when it changed. Returns whether smoke is currently worth drawing. */
  sync(sim: SmokeSim, u: SmokeUniforms, active: boolean): boolean {
    const dims = `${sim.nx}x${sim.ny}x${sim.nz}`;
    if (dims !== this.dims) {
      this.tex?.dispose();
      this.data = new Uint8Array(sim.nx * sim.ny * sim.nz);
      this.tex = new THREE.Data3DTexture(this.data, sim.nx, sim.ny, sim.nz);
      this.tex.format = THREE.RedFormat;
      this.tex.type = THREE.UnsignedByteType;
      this.tex.minFilter = THREE.LinearFilter;
      this.tex.magFilter = THREE.LinearFilter;
      this.tex.wrapS = this.tex.wrapT = this.tex.wrapR = THREE.ClampToEdgeWrapping;
      this.tex.unpackAlignment = 1;
      this.dims = dims;
      this.ver = -1;
    }
    if (this.ver !== sim.version && this.tex) {
      sim.fillTexture(this.data);
      this.tex.needsUpdate = true;
      this.ver = sim.version;
    }
    u.uSmoke.value = this.tex;
    u.uSmokeMin.value.set(sim.min[0], sim.min[1], sim.min[2]);
    u.uSmokeInv.value.set(1 / sim.size[0], 1 / sim.size[1], 1 / sim.size[2]);
    u.uSmokeSize.value.set(sim.size[0], sim.size[1], sim.size[2]);
    u.uSmokeOn.value = active ? 1 : 0;
    return active;
  }
  dispose() { this.tex?.dispose(); }
}

export interface SmokeUniforms {
  uSmoke: { value: THREE.Data3DTexture | null };
  uSmokeMin: { value: THREE.Vector3 };
  uSmokeInv: { value: THREE.Vector3 };
  uSmokeSize: { value: THREE.Vector3 };
  uSmokeOn: { value: number };
  uSmokeGain: { value: number };
}

/** One shared uniform set — merged into the beam uniforms and the plume material. */
export const smokeU: SmokeUniforms = {
  uSmoke: { value: null },
  uSmokeMin: { value: new THREE.Vector3() },
  uSmokeInv: { value: new THREE.Vector3(1, 1, 1) },
  uSmokeSize: { value: new THREE.Vector3(1, 1, 1) },
  uSmokeOn: { value: 0 },
  uSmokeGain: { value: 2.2 },
};

// ── plume pass ───────────────────────────────────────────
export interface PlumeBeam { apex: THREE.Vector3; axis: THREE.Vector3; tanOuter: number; len: number; color: THREE.Color }
const NB = 8;

const PLUME_VERT = /* glsl */ `varying vec3 vWorld; void main(){ vec4 w = modelMatrix * vec4(position, 1.0); vWorld = w.xyz; gl_Position = projectionMatrix * viewMatrix * w; }`;

const PLUME_FRAG = /* glsl */ `
precision highp float;
uniform sampler2D uDepth; uniform vec2 uRes; uniform mat4 uInvProj; uniform float uTime; uniform float uNoise;
uniform vec3 uSmokeSize; uniform float uSmokeGain;
uniform float uPlumeSteps; uniform vec3 uAmbient; uniform float uDebug; uniform float uSliceY;
uniform int uNB; uniform vec4 uBA[${NB}]; uniform vec4 uBX[${NB}]; uniform vec3 uBC[${NB}];
varying vec3 vWorld;
${NOISE_GLSL}
${SMOKE_GLSL}
${PHASE_GLSL}
vec3 heat(float v){ v = clamp(v, 0.0, 1.0); return vec3(smoothstep(0.35, 0.8, v), smoothstep(0.0, 0.6, v) * (1.0 - 0.5 * smoothstep(0.7, 1.0, v)), 0.25 + 0.75 * smoothstep(0.0, 0.35, v) * (1.0 - smoothstep(0.5, 0.9, v))); }
void main(){
  vec3 o = cameraPosition;
  vec3 d = normalize(vWorld - o);
  vec3 bmin = uSmokeMin, bmax = uSmokeMin + uSmokeSize;
  vec3 ta = (bmin - o) / d, tb = (bmax - o) / d;
  vec3 tmn = min(ta, tb), tmx = max(ta, tb);
  float tin = max(max(tmn.x, tmn.y), max(tmn.z, 0.0));
  float tout = min(min(tmx.x, tmx.y), tmx.z);
  vec2 suv = gl_FragCoord.xy / uRes;
  float dz = texture2D(uDepth, suv).x;
  float sceneT = 1e6;
  if (dz < 0.99999) { vec4 c = vec4(suv * 2.0 - 1.0, dz * 2.0 - 1.0, 1.0); vec4 v = uInvProj * c; v /= v.w; sceneT = length(v.xyz); }
  tout = min(tout, sceneT);
  if (tout <= tin) discard;
  if (uDebug > 1.5) { // volume: heat-map of the densest sample along the ray
    float mx = 0.0; float dt0 = (tout - tin) / 40.0;
    for (int i = 0; i < 40; i++) mx = max(mx, smokeAt(o + d * (tin + (float(i) + 0.5) * dt0)));
    gl_FragColor = vec4(heat(mx / 1.2) * 0.9, 0.0); return;
  }
  if (uDebug > 0.5) { // horizontal slice
    float t = (uSliceY - o.y) / d.y;
    if (t < tin || t > tout) discard;
    float v = smokeAt(o + d * t);
    gl_FragColor = vec4(heat(v / 1.2) * 0.85, 0.0); return;
  }
  tout = min(tout, tin + 45.0);
  float dt = (tout - tin) / uPlumeSteps;
  float jit = fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
  vec3 L = vec3(0.0); float tau = 0.0;
  for (int i = 0; i < 40; i++) {
    if (float(i) >= uPlumeSteps) break;
    vec3 p = o + d * (tin + (float(i) + jit) * dt);
    float dn = smokeAt(p);
    if (dn < 0.004) continue;
    vec3 lit = uAmbient;
    for (int j = 0; j < ${NB}; j++) {
      if (j >= uNB) break;
      vec3 q = p - uBA[j].xyz;
      float h = dot(q, uBX[j].xyz);
      if (h <= 0.05 || h > uBX[j].w) continue;
      float s = length(q - h * uBX[j].xyz) / (h * uBA[j].w);
      // only the halo outside the cone — the cone itself is integrated by the beam pass
      if (s > 1.0) lit += uBC[j] * (0.2 * exp(-(s - 1.0) * 2.6) / (1.0 + h * h * 0.02)) * phaseHG(dot(uBX[j].xyz, -d));
    }
    float T = exp(-tau);
    L += lit * (dn * dt * T);
    tau += dn * dt * 0.9;
  }
  gl_FragColor = vec4(L * uSmokeGain, min(0.82, 1.0 - exp(-tau)) * 0.6);
}`;

export class SmokePlume {
  readonly mesh: THREE.Mesh;
  readonly mat: THREE.ShaderMaterial;
  constructor(shared: Record<string, { value: unknown }>) {
    const NBV = Array.from({ length: NB }, () => new THREE.Vector4());
    this.mat = new THREE.ShaderMaterial({
      vertexShader: PLUME_VERT,
      fragmentShader: PLUME_FRAG,
      transparent: true,
      depthTest: false,
      depthWrite: false,
      side: THREE.BackSide,
      blending: THREE.CustomBlending,
      blendEquation: THREE.AddEquation,
      blendSrc: THREE.OneFactor,
      blendDst: THREE.OneFactor,
      uniforms: {
        uDepth: shared.uDepth, uRes: shared.uRes, uInvProj: shared.uInvProj, uTime: shared.uTime, uNoise: shared.uNoise,
        ...smokeU,
        uPlumeSteps: { value: 24 }, uAmbient: { value: new THREE.Vector3(0.01, 0.01, 0.012) }, uDebug: { value: 0 }, uSliceY: { value: 1.2 },
        uNB: { value: 0 }, uBA: { value: NBV.map((v) => v.clone()) }, uBX: { value: NBV.map((v) => v.clone()) },
        uBC: { value: Array.from({ length: NB }, () => new THREE.Vector3()) },
      },
    });
    const g = new THREE.BoxGeometry(1, 1, 1);
    this.mesh = new THREE.Mesh(g, this.mat);
    this.mesh.frustumCulled = false;
    this.mesh.visible = false;
    this.mesh.renderOrder = -1;
  }

  update(sim: SmokeSim, on: boolean, steps: number, beams: PlumeBeam[], ambient: THREE.Vector3, debug: number, sliceY: number) {
    this.mesh.visible = on;
    if (!on) return;
    this.mesh.position.set(sim.min[0] + sim.size[0] / 2, sim.min[1] + sim.size[1] / 2, sim.min[2] + sim.size[2] / 2);
    this.mesh.scale.set(sim.size[0], sim.size[1], sim.size[2]);
    this.mesh.updateMatrixWorld(true);
    const u = this.mat.uniforms;
    u.uPlumeSteps.value = steps;
    (u.uAmbient.value as THREE.Vector3).copy(ambient);
    u.uDebug.value = debug;
    u.uSliceY.value = sliceY;
    const n = Math.min(NB, beams.length);
    u.uNB.value = n;
    for (let i = 0; i < n; i++) {
      const b = beams[i];
      (u.uBA.value as THREE.Vector4[])[i].set(b.apex.x, b.apex.y, b.apex.z, b.tanOuter);
      (u.uBX.value as THREE.Vector4[])[i].set(b.axis.x, b.axis.y, b.axis.z, b.len);
      (u.uBC.value as THREE.Vector3[])[i].set(b.color.r, b.color.g, b.color.b);
    }
  }
  dispose() { this.mat.dispose(); (this.mesh.geometry as THREE.BufferGeometry).dispose(); }
}

export const GRID_FOR = { low: GRIDS.low, med: GRIDS.med, high: GRIDS.high };
