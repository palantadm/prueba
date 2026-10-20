// Venue shell: wood deck, black-box walls / theatre legs & borders / cyclorama, house floor, audience, mannequin.
import * as THREE from "three";
import { Reflector } from "three/examples/jsm/objects/Reflector.js";
import { mergeGeometries } from "three/examples/jsm/utils/BufferGeometryUtils.js";
import { RoundedBoxGeometry } from "three/examples/jsm/geometries/RoundedBoxGeometry.js";
import { curtainGeometry } from "./objects";

export type VenueMode = "blackbox" | "theatre" | "cyc" | "open";

export interface VenueOpts {
  size: [number, number, number];
  mode: VenueMode;
  audience: boolean;
  mannequin: boolean;
  reflect: boolean;
  reflectRes: number;
}

let woodTex: THREE.CanvasTexture | null = null;
function woodTexture(): THREE.CanvasTexture {
  if (woodTex) return woodTex;
  const c = document.createElement("canvas");
  c.width = 1024; c.height = 1024;
  const g = c.getContext("2d")!;
  g.fillStyle = "#6a5440"; g.fillRect(0, 0, 1024, 1024);
  const boards = 16;
  let seed = 7;
  const rnd = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296; };
  for (let i = 0; i < boards; i++) {
    const x = (i * 1024) / boards;
    const l = 0.82 + rnd() * 0.3;
    g.fillStyle = `rgb(${Math.round(106 * l)},${Math.round(84 * l)},${Math.round(64 * l)})`;
    g.fillRect(x, 0, 1024 / boards, 1024);
    // grain
    for (let k = 0; k < 40; k++) {
      g.strokeStyle = `rgba(30,20,10,${0.03 + rnd() * 0.06})`;
      g.lineWidth = 1 + rnd() * 1.5;
      const gx = x + rnd() * (1024 / boards);
      g.beginPath(); g.moveTo(gx, 0); g.bezierCurveTo(gx + rnd() * 6 - 3, 340, gx + rnd() * 6 - 3, 680, gx + rnd() * 6 - 3, 1024); g.stroke();
    }
    g.fillStyle = "rgba(0,0,0,0.55)"; g.fillRect(x, 0, 1.6, 1024);
    // butt joints
    const j = rnd() * 1024;
    g.fillRect(x, j, 1024 / boards, 1.4);
  }
  woodTex = new THREE.CanvasTexture(c);
  woodTex.wrapS = woodTex.wrapT = THREE.RepeatWrapping;
  woodTex.colorSpace = THREE.SRGBColorSpace;
  woodTex.anisotropy = 8;
  return woodTex;
}

const REFL_SHADER = {
  uniforms: { color: { value: null }, tDiffuse: { value: null }, textureMatrix: { value: null }, uStrength: { value: 0.3 } },
  vertexShader: /* glsl */ `
    uniform mat4 textureMatrix; varying vec4 vUv;
    void main(){ vUv = textureMatrix * vec4(position, 1.0); gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
  fragmentShader: /* glsl */ `
    uniform sampler2D tDiffuse; uniform float uStrength; varying vec4 vUv;
    void main(){ vec4 b = texture2DProj(tDiffuse, vUv); gl_FragColor = vec4(b.rgb * uStrength, 1.0); }`,
};

/** Cove-profile cyclorama: floor → radius → wall, extruded along x. */
function cycGeometry(width: number, depth: number, height: number): THREE.BufferGeometry {
  const R = Math.min(1.6, depth * 0.25);
  const arc = 14;
  const profile: [number, number][] = [[0, 1.2]]; // [y, z]
  for (let i = 0; i <= arc; i++) { const a = (i / arc) * (Math.PI / 2); profile.push([R - Math.cos(a) * R, -Math.sin(a) * R]); }
  profile.push([height, -R]);
  const P: number[] = [], I: number[] = [];
  const seg = 24;
  for (let r = 0; r < profile.length; r++) for (let s = 0; s <= seg; s++) P.push((s / seg - 0.5) * width, profile[r][0], profile[r][1]);
  for (let r = 0; r < profile.length - 1; r++) for (let s = 0; s < seg; s++) {
    const a = r * (seg + 1) + s, b = a + 1, c = a + seg + 1, d = c + 1;
    I.push(a, c, b, b, c, d);
  }
  const g = new THREE.BufferGeometry();
  g.setAttribute("position", new THREE.Float32BufferAttribute(P, 3));
  g.setIndex(I);
  g.computeVertexNormals();
  return g;
}

export class Venue {
  readonly group = new THREE.Group();
  /** floor meshes that should receive pools/shadows */
  readonly floor = new THREE.Group();
  private disposables: { dispose(): void }[] = [];
  private reflector: Reflector | null = null;
  private key = "";

  /** World bounds of the room (for orbit limits / camera presets). */
  bounds = new THREE.Box3();

  constructor() {
    this.group.name = "venue";
  }

  private own<T extends { dispose(): void }>(x: T): T { this.disposables.push(x); return x; }

  build(o: VenueOpts) {
    const key = JSON.stringify([o.size, o.mode, o.audience, o.mannequin, o.reflect, o.reflectRes]);
    if (key === this.key) return;
    this.key = key;
    this.clear();
    const [W, D, H] = o.size;
    const root = this.group;

    // ── deck ──
    const wood = this.own(woodTexture().clone());
    wood.needsUpdate = true;
    wood.repeat.set(Math.max(1, W / 2.4), Math.max(1, D / 2.4));
    const deckMat = this.own(new THREE.MeshStandardMaterial({ map: wood, color: "#a89478", roughness: 0.46, metalness: 0.0, envMapIntensity: 0.12 }));
    const deck = new THREE.Mesh(new THREE.PlaneGeometry(W, D).rotateX(-Math.PI / 2), deckMat);
    deck.receiveShadow = true;
    deck.raycast = () => undefined;
    this.own(deck.geometry);
    root.add(deck);

    // deck edge (apron lip)
    const lip = new THREE.Mesh(this.own(new THREE.BoxGeometry(W, 0.12, 0.06)), this.own(new THREE.MeshStandardMaterial({ color: "#101114", roughness: 0.6, metalness: 0.2 })));
    lip.position.set(0, -0.06, D / 2 + 0.03);
    lip.raycast = () => undefined;
    root.add(lip);

    // house floor
    const houseMat = this.own(new THREE.MeshStandardMaterial({ color: "#0d0e11", roughness: 0.8, metalness: 0.05, envMapIntensity: 0.1 }));
    const house = new THREE.Mesh(this.own(new THREE.PlaneGeometry(W + 24, 40).rotateX(-Math.PI / 2)), houseMat);
    house.position.set(0, -0.12, D / 2 + 20);
    house.receiveShadow = true;
    house.raycast = () => undefined;
    root.add(house);
    const wings = new THREE.Mesh(this.own(new THREE.PlaneGeometry(W + 24, D + 6).rotateX(-Math.PI / 2)), houseMat);
    wings.position.set(0, -0.02, -1);
    wings.visible = o.mode !== "open";
    wings.raycast = () => undefined;
    // (the deck covers the centre; wings only matter beyond the deck width)
    wings.scale.set(1, 1, 1);
    root.add(wings);

    // ── planar reflection (High) ──
    if (o.reflect) {
      const res = o.reflectRes;
      const refl = new Reflector(new THREE.PlaneGeometry(W, D), {
        textureWidth: res, textureHeight: Math.round(res * 0.6), clipBias: 0.003, shader: REFL_SHADER, color: 0xffffff,
      });
      refl.rotation.x = -Math.PI / 2;
      refl.position.y = 0.004;
      const m = refl.material as THREE.ShaderMaterial;
      m.transparent = true;
      m.blending = THREE.AdditiveBlending;
      m.depthWrite = false;
      m.uniforms.uStrength.value = 0.15;
      refl.raycast = () => undefined;
      this.reflector = refl;
      root.add(refl);
    }

    // ── walls / drapes ──
    const black = this.own(new THREE.MeshStandardMaterial({ color: "#09090b", roughness: 1, metalness: 0, side: THREE.DoubleSide, envMapIntensity: 0 }));
    const wallH = H + 1.5;
    const ex = W / 2 + 2.5;                      // room half width
    const backZ = -D / 2 - 0.6;
    if (o.mode === "blackbox" || o.mode === "theatre") {
      const back = new THREE.Mesh(this.own(new THREE.PlaneGeometry(ex * 2, wallH)), black);
      back.position.set(0, wallH / 2, backZ);
      back.receiveShadow = true; back.raycast = () => undefined;
      root.add(back);
      for (const sx of [-1, 1]) {
        const side = new THREE.Mesh(this.own(new THREE.PlaneGeometry(D + 14, wallH)), black);
        side.rotation.y = -sx * Math.PI / 2;
        side.position.set(sx * ex, wallH / 2, backZ + (D + 14) / 2);
        side.receiveShadow = true; side.raycast = () => undefined;
        root.add(side);
      }
    }
    if (o.mode === "theatre") {
      const folds = this.own(curtainGeometry(1.8, H + 0.6));
      const vel = this.own(new THREE.MeshStandardMaterial({ color: "#07070a", roughness: 0.96, metalness: 0, side: THREE.DoubleSide, envMapIntensity: 0 }));
      for (const z of [-D / 2 + 0.5, -D / 6, D / 6 + 0.2]) {
        for (const sx of [-1, 1]) {
          for (let k = 0; k < 2; k++) {
            const leg = new THREE.Mesh(folds, vel);
            leg.position.set(sx * (W / 2 - 0.9 + (z > 0 ? 0.3 : 0) - k * 1.7 * 0), (H + 0.6) / 2, z);
            leg.position.x += sx * k * 1.7;
            leg.castShadow = true; leg.raycast = () => undefined;
            root.add(leg);
          }
        }
        const borderGeo = this.own(curtainGeometry(W + 3, 1.2));
        const border = new THREE.Mesh(borderGeo, vel);
        border.position.set(0, H - 0.1, z);
        border.castShadow = true; border.raycast = () => undefined;
        root.add(border);
      }
    }
    if (o.mode === "cyc") {
      const cyc = new THREE.Mesh(this.own(cycGeometry(W + 1, D, H)), this.own(new THREE.MeshStandardMaterial({ color: "#d8d6d0", roughness: 0.92, metalness: 0, side: THREE.DoubleSide, envMapIntensity: 0.05 })));
      cyc.position.set(0, 0, -D / 2 - 0.5);
      cyc.receiveShadow = true; cyc.raycast = () => undefined;
      root.add(cyc);
      const side = this.own(new THREE.PlaneGeometry(D + 14, wallH));
      for (const sx of [-1, 1]) {
        const s = new THREE.Mesh(side, black);
        s.rotation.y = -sx * Math.PI / 2;
        s.position.set(sx * ex, wallH / 2, backZ + (D + 14) / 2);
        s.raycast = () => undefined;
        root.add(s);
      }
    }
    if (o.mode === "open") {
      const sky = new THREE.Mesh(this.own(new THREE.PlaneGeometry(260, 260).rotateX(-Math.PI / 2)), this.own(new THREE.MeshStandardMaterial({ color: "#0a0b0e", roughness: 1 })));
      sky.position.y = -0.2; sky.raycast = () => undefined;
      root.add(sky);
    }

    // ── audience ──
    if (o.audience) {
      const rows = 7, cols = Math.max(6, Math.round((W + 4) / 0.62));
      const seatG = this.own(mergeGeometries([
        new RoundedBoxGeometry(0.5, 0.1, 0.5, 2, 0.03).translate(0, 0.45, 0),
        new RoundedBoxGeometry(0.5, 0.55, 0.09, 2, 0.03).translate(0, 0.72, -0.23),
      ])!);
      const seatM = this.own(new THREE.MeshStandardMaterial({ color: "#2a0e14", roughness: 0.9, metalness: 0 }));
      const inst = new THREE.InstancedMesh(seatG, seatM, rows * cols);
      const mtx = new THREE.Matrix4();
      let n = 0;
      for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) {
        const x = (c - (cols - 1) / 2) * 0.62;
        if (Math.abs(x) < 0.5) continue;           // centre aisle
        mtx.makeTranslation(x, r * 0.16 - 0.1, D / 2 + 5.6 + r * 1.0);
        inst.setMatrixAt(n++, mtx);
      }
      inst.count = n;
      inst.castShadow = false; inst.receiveShadow = true; inst.raycast = () => undefined;
      root.add(inst);
    }

    // ── scale figure ──
    if (o.mannequin) {
      const g = new THREE.Group();
      const m = this.own(new THREE.MeshStandardMaterial({ color: "#8a8f98", roughness: 0.7, metalness: 0 }));
      const legs = new THREE.Mesh(this.own(new THREE.CapsuleGeometry(0.1, 0.65, 4, 10)), m); legs.position.y = 0.45;
      const torso = new THREE.Mesh(this.own(new THREE.CapsuleGeometry(0.17, 0.4, 4, 12)), m); torso.position.y = 1.15;
      const head = new THREE.Mesh(this.own(new THREE.SphereGeometry(0.11, 16, 12)), m); head.position.y = 1.64;
      for (const mm of [legs, torso, head]) { mm.castShadow = true; mm.receiveShadow = true; mm.raycast = () => undefined; g.add(mm); }
      g.position.set(0, 0, 0.8);
      root.add(g);
    }

    this.bounds.set(new THREE.Vector3(-ex, 0, backZ), new THREE.Vector3(ex, H + 1.5, D / 2 + 24));
  }

  private clear() {
    while (this.group.children.length) {
      const c = this.group.children.pop()!;
      c.removeFromParent();
      if (c === this.reflector) { this.reflector.dispose(); this.reflector = null; }
    }
    this.disposables.forEach((d) => d.dispose());
    this.disposables = [];
  }

  dispose() { this.clear(); }
}
