// Build a fixture visual from the GDTF geometry tree (Model3d). Y-up metres; beams travel along local -Y.
// Pan rotates a node about Y, tilt about X. Model files: .glb/.gltf (GLTFLoader) and .3ds (TDSLoader, Z-up → rotated).
import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { TDSLoader } from "three/examples/jsm/loaders/TDSLoader.js";
import { RoundedBoxGeometry } from "three/examples/jsm/geometries/RoundedBoxGeometry.js";
import type { BeamInfo, GeoNode, Model3d } from "../types";
import { M, type FixtureModel } from "./fixture-models";

const modelCache = new Map<string, Promise<THREE.Object3D | null>>();

function fixMaterial(m: THREE.Material | THREE.Material[]): THREE.Material | THREE.Material[] {
  const one = (x: THREE.Material) => {
    if ((x as THREE.MeshStandardMaterial).isMeshStandardMaterial) {
      const st = x as THREE.MeshStandardMaterial;
      st.side = THREE.DoubleSide;
      return st;
    }
    const old = x as THREE.MeshPhongMaterial;
    const col = old.color ? old.color.clone() : new THREE.Color("#2a2d33");
    // 3DS vendors ship near-black plastics; keep the hue but make sure the form stays readable.
    const hsl = { h: 0, s: 0, l: 0 };
    col.getHSL(hsl);
    col.setHSL(hsl.h, hsl.s, Math.max(0.12, Math.min(0.75, hsl.l)));
    return new THREE.MeshStandardMaterial({ color: col, roughness: 0.48, metalness: 0.45, side: THREE.DoubleSide });
  };
  return Array.isArray(m) ? m.map(one) : one(m);
}

function loadModel(url: string): Promise<THREE.Object3D | null> {
  let p = modelCache.get(url);
  if (p) return p;
  const lower = url.toLowerCase();
  p = new Promise<THREE.Object3D | null>((resolve) => {
    const done = (obj: THREE.Object3D, zUp: boolean) => {
      obj.traverse((o) => {
        const mesh = o as THREE.Mesh;
        if (mesh.isMesh) {
          mesh.material = fixMaterial(mesh.material);
          if (mesh.geometry && !mesh.geometry.attributes.normal) mesh.geometry.computeVertexNormals();
        }
      });
      const wrap = new THREE.Group();
      if (zUp) obj.rotation.x = -Math.PI / 2;
      wrap.add(obj);
      // mm-scale files: normalise to metres
      const box = new THREE.Box3().setFromObject(wrap);
      const size = box.getSize(new THREE.Vector3());
      if (Math.max(size.x, size.y, size.z) > 20) wrap.scale.setScalar(0.001);
      resolve(wrap);
    };
    const fail = () => resolve(null);
    try {
      if (lower.endsWith(".3ds")) new TDSLoader().load(url, (g) => done(g, true), undefined, fail);
      else new GLTFLoader().load(url, (g) => done(g.scene, false), undefined, fail);
    } catch { fail(); }
  });
  modelCache.set(url, p);
  return p;
}

function primitiveMesh(name: string, size: [number, number, number], axis: string | null): THREE.Mesh {
  const [w, h, d] = [Math.max(0.01, size[0]), Math.max(0.01, size[1]), Math.max(0.01, size[2])];
  const n = name.toLowerCase();
  let g: THREE.BufferGeometry;
  if (n === "cylinder" || n === "head" || n === "conventional") g = new THREE.CylinderGeometry(Math.min(w, d) / 2, Math.min(w, d) / 2, h, 32);
  else if (n === "sphere") g = new THREE.SphereGeometry(Math.min(w, h, d) / 2, 24, 16);
  else if (n === "yoke") g = new RoundedBoxGeometry(w, h, Math.max(0.04, d * 0.4), 3, 0.02);
  else g = new RoundedBoxGeometry(w, h, d, 3, Math.min(0.02, Math.min(w, h, d) / 4));
  void axis;
  return new THREE.Mesh(g, n === "head" || n === "conventional" ? M.dark : M.black);
}

export interface GdtfBuild extends FixtureModel {
  beam: BeamInfo | null;
}

export function buildGdtfModel(m3: Model3d): GdtfBuild {
  const root = new THREE.Group();
  root.rotation.order = "YXZ";
  const emissive: THREE.Mesh[] = [];
  const lensMat = new THREE.MeshBasicMaterial({ color: "#ffffff", toneMapped: false });
  const lensGeo = new THREE.CircleGeometry(1, 32);
  lensGeo.rotateX(Math.PI / 2);
  let pan: THREE.Object3D | null = null;
  let tilt: THREE.Object3D | null = null;
  let lens: THREE.Object3D | null = null;
  let mainBeam: BeamInfo | null = null;
  let lensR = 0.08;
  const assets = m3.assets.endsWith("/") ? m3.assets : m3.assets + "/";

  const visit = (n: GeoNode, parent: THREE.Object3D) => {
    const g = new THREE.Group();
    g.name = n.name;
    g.matrixAutoUpdate = false;
    g.matrix.fromArray(n.matrix);
    g.matrixWorldNeedsUpdate = true;
    parent.add(g);
    // rotation sub-node for axes
    let host: THREE.Object3D = g;
    if (n.axis === "pan" || n.axis === "tilt") {
      const r = new THREE.Group();
      r.name = n.axis;
      g.add(r);
      host = r;
      if (n.axis === "pan") pan = r; else tilt = r;
    }
    if (n.model) {
      const holder = new THREE.Group();
      host.add(holder);
      loadModel(assets + n.model).then((obj) => {
        if (obj) holder.add(obj.clone(true));
        else holder.add(primitiveMesh(n.primitive ?? "Cube", n.size, n.axis));
      });
    } else if (n.primitive && !n.beam) {
      host.add(primitiveMesh(n.primitive, n.size, n.axis));
    }
    if (n.beam && n.beam.beam_type !== "none") {
      if (!lens) {
        lens = new THREE.Object3D();
        host.add(lens);
        mainBeam = n.beam;
        lensR = Math.max(0.01, n.beam.radius || 0.05);
        const d = new THREE.Mesh(lensGeo, lensMat);
        d.scale.setScalar(lensR * 0.96);
        d.position.y = -0.002;
        lens.add(d);
        emissive.push(d);
      } else {
        // additional emitters (pixels): glow discs only
        const d = new THREE.Mesh(lensGeo, lensMat);
        d.scale.setScalar(Math.max(0.005, n.beam.radius || 0.01));
        host.add(d);
        emissive.push(d);
      }
    }
    for (const c of n.children) visit(c, host);
  };
  visit(m3.root, root);

  if (!lens) {
    lens = new THREE.Object3D();
    lens.position.y = -0.2;
    root.add(lens);
  }
  return {
    kind: "generic", root, pan, tilt, lens, lensRadius: lensR, emissive,
    selCenter: [0, -0.25, 0], selSize: [0.45, 0.6, 0.45],
    beam: mainBeam,
    dispose() { lensMat.dispose(); lensGeo.dispose(); },
  };
}
