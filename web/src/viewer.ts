import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";

export class SceneViewer {
  renderer: THREE.WebGLRenderer;
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  controls: OrbitControls;
  root: THREE.Group;
  private anim = 0;

  constructor(container: HTMLElement) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(container.clientWidth, container.clientHeight);
    this.renderer.domElement.id = "viewer-canvas";
    container.appendChild(this.renderer.domElement);

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x0d1210);

    this.camera = new THREE.PerspectiveCamera(
      50,
      container.clientWidth / Math.max(container.clientHeight, 1),
      0.05,
      200,
    );
    this.camera.position.set(6, 5, 8);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.target.set(2.5, 1.2, 2);
    this.controls.update();

    const hemi = new THREE.HemisphereLight(0xe8efe6, 0x2a332c, 1.1);
    this.scene.add(hemi);
    const dir = new THREE.DirectionalLight(0xfff2d6, 1.2);
    dir.position.set(5, 10, 3);
    this.scene.add(dir);

    this.root = new THREE.Group();
    this.scene.add(this.root);

    const grid = new THREE.GridHelper(20, 20, 0x3a4a3c, 0x243028);
    grid.position.y = -0.01;
    this.scene.add(grid);

    const onResize = () => {
      const w = container.clientWidth;
      const h = container.clientHeight;
      this.camera.aspect = w / Math.max(h, 1);
      this.camera.updateProjectionMatrix();
      this.renderer.setSize(w, h);
    };
    window.addEventListener("resize", onResize);

    const tick = () => {
      this.anim = requestAnimationFrame(tick);
      this.controls.update();
      this.renderer.render(this.scene, this.camera);
    };
    tick();
  }

  async loadGlb(url: string) {
    while (this.root.children.length) {
      this.root.remove(this.root.children[0]);
    }
    const loader = new GLTFLoader();
    const gltf = await loader.loadAsync(url);
    this.root.add(gltf.scene);

    const box = new THREE.Box3().setFromObject(gltf.scene);
    const center = box.getCenter(new THREE.Vector3());
    const size = box.getSize(new THREE.Vector3());
    this.controls.target.copy(center);
    const dist = Math.max(size.x, size.y, size.z) * 1.4;
    this.camera.position.set(center.x + dist, center.y + dist * 0.7, center.z + dist);
    this.controls.update();
  }

  focusRoom(name: string) {
    const target = this.root.getObjectByName(name);
    if (!target) {
      // try floor- prefix
      const floor = this.root.getObjectByName(`floor-${name}`);
      if (!floor) return;
      const box = new THREE.Box3().setFromObject(floor);
      const c = box.getCenter(new THREE.Vector3());
      this.controls.target.copy(c);
      this.camera.position.set(c.x + 3, c.y + 4, c.z + 3);
      this.controls.update();
      return;
    }
    const box = new THREE.Box3().setFromObject(target);
    const c = box.getCenter(new THREE.Vector3());
    this.controls.target.copy(c);
    this.camera.position.set(c.x + 3, c.y + 4, c.z + 3);
    this.controls.update();
  }

  dispose() {
    cancelAnimationFrame(this.anim);
    this.renderer.dispose();
  }
}
