import type { FloorplanIR, Vec2, WallSegment } from "./api";

function uuid(): string {
  return crypto.randomUUID();
}

export type EditorTool = "select" | "pan" | "add-wall" | "add-door" | "add-window";

function vx(p: Vec2): number {
  return p[0];
}
function vy(p: Vec2): number {
  return p[1];
}

export class FloorplanEditor {
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  ir: FloorplanIR;
  image: HTMLImageElement | null = null;
  imageNatural = { w: 1, h: 1 };
  tool: EditorTool = "select";
  selectedWallId: string | null = null;
  drag: { wallId: string; end: "a" | "b" } | null = null;
  panDrag: { x: number; y: number; panX: number; panY: number } | null = null;
  pendingStart: Vec2 | null = null;
  onChange: ((ir: FloorplanIR) => void) | null = null;

  /** Pixels per image pixel at zoom=1 */
  baseScale = 1;
  zoom = 1;
  panX = 0;
  panY = 0;
  displayW = 800;
  displayH = 480;
  private dpr = 1;
  private resizeObserver: ResizeObserver | null = null;

  constructor(canvas: HTMLCanvasElement, ir: FloorplanIR) {
    this.canvas = canvas;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2d context unavailable");
    this.ctx = ctx;
    this.ir = structuredClone(ir);
    this.bind();
    const wrap = canvas.parentElement;
    if (wrap) {
      this.resizeObserver = new ResizeObserver(() => this.resize());
      this.resizeObserver.observe(wrap);
    }
  }

  destroy() {
    this.resizeObserver?.disconnect();
  }

  setIr(ir: FloorplanIR) {
    this.ir = structuredClone(ir);
    this.draw();
  }

  async loadImage(url: string) {
    const img = new Image();
    img.crossOrigin = "anonymous";
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error("floorplan image failed"));
      img.src = url;
    });
    this.image = img;
    this.imageNatural = { w: img.naturalWidth, h: img.naturalHeight };
    this.resize();
    this.fitToView();
    this.draw();
  }

  resize() {
    const wrap = this.canvas.parentElement;
    if (!wrap) return;
    this.dpr = window.devicePixelRatio || 1;
    this.displayW = wrap.clientWidth || 800;
    this.displayH = Math.max(420, wrap.clientHeight || 420);
    this.canvas.width = Math.floor(this.displayW * this.dpr);
    this.canvas.height = Math.floor(this.displayH * this.dpr);
    this.canvas.style.width = `${this.displayW}px`;
    this.canvas.style.height = `${this.displayH}px`;
    this.draw();
  }

  fitToView() {
    const iw = this.imageNatural.w;
    const ih = this.imageNatural.h;
    if (iw < 1 || ih < 1) return;
    this.baseScale = Math.min(this.displayW / iw, this.displayH / ih) * 0.92;
    this.zoom = 1;
    this.panX = (this.displayW - iw * this.baseScale) * 0.5;
    this.panY = (this.displayH - ih * this.baseScale) * 0.5;
    this.draw();
  }

  zoomIn() {
    this.zoomAt(this.displayW * 0.5, this.displayH * 0.5, 1.25);
  }

  zoomOut() {
    this.zoomAt(this.displayW * 0.5, this.displayH * 0.5, 1 / 1.25);
  }

  private zoomAt(sx: number, sy: number, factor: number) {
    const before = this.screenToPx(sx, sy);
    this.zoom = Math.min(12, Math.max(0.15, this.zoom * factor));
    const after = this.screenToPx(sx, sy);
    this.panX += (after.x - before.x) * this.baseScale * this.zoom;
    this.panY += (after.y - before.y) * this.baseScale * this.zoom;
    this.draw();
  }

  private scaleFactor() {
    return this.baseScale * this.zoom;
  }

  private pxToScreen(px: number, py: number) {
    const s = this.scaleFactor();
    return { x: px * s + this.panX, y: py * s + this.panY };
  }

  private screenToPx(sx: number, sy: number) {
    const s = this.scaleFactor();
    return { x: (sx - this.panX) / s, y: (sy - this.panY) / s };
  }

  private toWorld(e: PointerEvent): Vec2 {
    const rect = this.canvas.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    const px = this.screenToPx(sx, sy);
    return [px.x * this.ir.scale_m_per_px, px.y * this.ir.scale_m_per_px];
  }

  private toScreen(p: Vec2) {
    const px = vx(p) / this.ir.scale_m_per_px;
    const py = vy(p) / this.ir.scale_m_per_px;
    return this.pxToScreen(px, py);
  }

  private bind() {
    this.canvas.addEventListener("pointerdown", (e) => this.onDown(e));
    this.canvas.addEventListener("pointermove", (e) => this.onMove(e));
    this.canvas.addEventListener("pointerup", (e) => this.onUp(e));
    this.canvas.addEventListener("pointerleave", (e) => this.onUp(e));
    this.canvas.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        const rect = this.canvas.getBoundingClientRect();
        const sx = e.clientX - rect.left;
        const sy = e.clientY - rect.top;
        const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
        this.zoomAt(sx, sy, factor);
      },
      { passive: false },
    );
  }

  private onDown(e: PointerEvent) {
    const rect = this.canvas.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;

    if (this.tool === "pan" || e.button === 1 || (e.button === 0 && e.shiftKey)) {
      this.panDrag = { x: sx, y: sy, panX: this.panX, panY: this.panY };
      this.canvas.setPointerCapture(e.pointerId);
      return;
    }

    const p = this.toWorld(e);
    if (this.tool === "add-wall") {
      if (!this.pendingStart) {
        this.pendingStart = p;
      } else {
        const wall: WallSegment = {
          id: uuid(),
          a: this.pendingStart,
          b: p,
          thickness_m: this.ir.default_wall_thickness_m,
          height_m: this.ir.default_wall_height_m,
        };
        this.ir.walls.push(wall);
        this.pendingStart = null;
        this.rebuildRooms();
        this.emit();
        this.draw();
      }
      return;
    }

    if (this.tool === "add-door" || this.tool === "add-window") {
      const wall = this.hitWall(p);
      if (!wall) return;
      this.ir.openings.push({
        id: uuid(),
        wall_id: wall.id,
        kind: this.tool === "add-door" ? "door" : "window",
        t0: 0.35,
        t1: 0.55,
        sill_m: this.tool === "add-door" ? 0 : 0.9,
        height_m: this.tool === "add-door" ? 2.1 : 1.2,
      });
      this.emit();
      this.draw();
      return;
    }

    const hitTol = 10 / this.scaleFactor();
    for (const w of this.ir.walls) {
      const sa = this.toScreen(w.a);
      const sb = this.toScreen(w.b);
      if (Math.hypot(sa.x - sx, sa.y - sy) < hitTol) {
        this.selectedWallId = w.id;
        this.drag = { wallId: w.id, end: "a" };
        this.canvas.setPointerCapture(e.pointerId);
        this.draw();
        return;
      }
      if (Math.hypot(sb.x - sx, sb.y - sy) < hitTol) {
        this.selectedWallId = w.id;
        this.drag = { wallId: w.id, end: "b" };
        this.canvas.setPointerCapture(e.pointerId);
        this.draw();
        return;
      }
    }
    const wall = this.hitWall(p);
    this.selectedWallId = wall?.id ?? null;
    this.draw();
  }

  private onMove(e: PointerEvent) {
    const rect = this.canvas.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;

    if (this.panDrag) {
      this.panX = this.panDrag.panX + (sx - this.panDrag.x);
      this.panY = this.panDrag.panY + (sy - this.panDrag.y);
      this.draw();
      return;
    }

    if (!this.drag) return;
    const p = this.toWorld(e);
    const wall = this.ir.walls.find((w) => w.id === this.drag!.wallId);
    if (!wall) return;
    if (this.drag.end === "a") wall.a = p;
    else wall.b = p;
    this.draw();
  }

  private onUp(e: PointerEvent) {
    if (this.canvas.hasPointerCapture(e.pointerId)) {
      this.canvas.releasePointerCapture(e.pointerId);
    }
    if (this.panDrag) {
      this.panDrag = null;
      return;
    }
    if (this.drag) {
      this.drag = null;
      this.rebuildRooms();
      this.emit();
      this.draw();
    }
  }

  deleteSelected() {
    if (!this.selectedWallId) return;
    this.ir.walls = this.ir.walls.filter((w) => w.id !== this.selectedWallId);
    this.ir.openings = this.ir.openings.filter((o) => o.wall_id !== this.selectedWallId);
    this.selectedWallId = null;
    this.rebuildRooms();
    this.emit();
    this.draw();
  }

  private hitWall(p: Vec2): WallSegment | null {
    let best: WallSegment | null = null;
    let bestD = Math.max(0.15, 12 / this.scaleFactor()) * this.ir.scale_m_per_px;
    for (const w of this.ir.walls) {
      const d = distToSegment(p, w.a, w.b);
      if (d < bestD) {
        bestD = d;
        best = w;
      }
    }
    return best;
  }

  rebuildRooms() {
    if (this.ir.walls.length === 0) {
      this.ir.rooms = [];
      return;
    }
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity;
    for (const w of this.ir.walls) {
      minX = Math.min(minX, vx(w.a), vx(w.b));
      minY = Math.min(minY, vy(w.a), vy(w.b));
      maxX = Math.max(maxX, vx(w.a), vx(w.b));
      maxY = Math.max(maxY, vy(w.a), vy(w.b));
    }
    const existing = this.ir.rooms[0];
    this.ir.rooms = [
      {
        id: existing?.id ?? uuid(),
        name: existing?.name ?? "Room 1",
        polygon: [
          [minX, minY],
          [maxX, minY],
          [maxX, maxY],
          [minX, maxY],
        ],
        wall_ids: this.ir.walls.map((w) => w.id),
      },
    ];
  }

  private emit() {
    this.onChange?.(structuredClone(this.ir));
  }

  draw() {
    const { ctx } = this;
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.clearRect(0, 0, this.displayW, this.displayH);
    ctx.fillStyle = "#0d1210";
    ctx.fillRect(0, 0, this.displayW, this.displayH);

    if (this.image) {
      const iw = this.imageNatural.w;
      const ih = this.imageNatural.h;
      const s = this.scaleFactor();
      ctx.drawImage(this.image, this.panX, this.panY, iw * s, ih * s);
      ctx.fillStyle = "rgba(10,14,12,0.32)";
      ctx.fillRect(this.panX, this.panY, iw * s, ih * s);
    }

    for (const room of this.ir.rooms) {
      if (room.polygon.length < 3) continue;
      ctx.beginPath();
      room.polygon.forEach((p, i) => {
        const sc = this.toScreen(p);
        if (i === 0) ctx.moveTo(sc.x, sc.y);
        else ctx.lineTo(sc.x, sc.y);
      });
      ctx.closePath();
      ctx.fillStyle = "rgba(111, 158, 138, 0.18)";
      ctx.fill();
    }

    const lw = Math.max(1.5, 2 / this.scaleFactor());
    const handleR = Math.max(4, 5 / this.scaleFactor());

    for (const w of this.ir.walls) {
      const a = this.toScreen(w.a);
      const b = this.toScreen(w.b);
      ctx.strokeStyle = w.id === this.selectedWallId ? "#c4a35a" : "#e8efe6";
      ctx.lineWidth = w.id === this.selectedWallId ? lw + 1 : lw;
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
      ctx.fillStyle = "#c4a35a";
      ctx.beginPath();
      ctx.arc(a.x, a.y, handleR, 0, Math.PI * 2);
      ctx.arc(b.x, b.y, handleR, 0, Math.PI * 2);
      ctx.fill();
    }

    for (const o of this.ir.openings) {
      const wall = this.ir.walls.find((w) => w.id === o.wall_id);
      if (!wall) continue;
      const mx = vx(wall.a) + (vx(wall.b) - vx(wall.a)) * ((o.t0 + o.t1) / 2);
      const my = vy(wall.a) + (vy(wall.b) - vy(wall.a)) * ((o.t0 + o.t1) / 2);
      const sc = this.toScreen([mx, my]);
      const sz = Math.max(6, 8 / this.scaleFactor());
      ctx.fillStyle = o.kind === "door" ? "#8b5a2b" : "#6fa8dc";
      ctx.fillRect(sc.x - sz / 2, sc.y - sz / 2, sz, sz);
    }

    if (this.pendingStart) {
      const sc = this.toScreen(this.pendingStart);
      ctx.fillStyle = "#c4a35a";
      ctx.beginPath();
      ctx.arc(sc.x, sc.y, handleR + 1, 0, Math.PI * 2);
      ctx.fill();
    }

    ctx.fillStyle = "rgba(154, 171, 158, 0.9)";
    ctx.font = '12px "Noto Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif';
    ctx.fillText(
      `缩放 ${(this.zoom * 100).toFixed(0)}% · 滚轮缩放 · Shift 拖拽平移`,
      10,
      this.displayH - 10,
    );
  }
}

function distToSegment(p: Vec2, a: Vec2, b: Vec2): number {
  const dx = vx(b) - vx(a);
  const dy = vy(b) - vy(a);
  const len2 = dx * dx + dy * dy || 1e-6;
  let t = ((vx(p) - vx(a)) * dx + (vy(p) - vy(a)) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  const qx = vx(a) + t * dx;
  const qy = vy(a) + t * dy;
  return Math.hypot(vx(p) - qx, vy(p) - qy);
}
