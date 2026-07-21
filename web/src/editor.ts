import type { FloorplanIR, Vec2, WallSegment } from "./api";

function uuid(): string {
  return crypto.randomUUID();
}

export type EditorTool = "select" | "add-wall" | "add-door" | "add-window";

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
  pendingStart: Vec2 | null = null;
  onChange: ((ir: FloorplanIR) => void) | null = null;
  scalePx = 1;

  constructor(canvas: HTMLCanvasElement, ir: FloorplanIR) {
    this.canvas = canvas;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2d context unavailable");
    this.ctx = ctx;
    this.ir = structuredClone(ir);
    this.bind();
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
    this.draw();
  }

  resize() {
    const maxW = this.canvas.parentElement?.clientWidth || 800;
    const scale = Math.min(1, maxW / this.imageNatural.w);
    this.scalePx = scale;
    this.canvas.width = Math.floor(this.imageNatural.w * scale);
    this.canvas.height = Math.floor(this.imageNatural.h * scale);
  }

  private bind() {
    this.canvas.addEventListener("pointerdown", (e) => this.onDown(e));
    this.canvas.addEventListener("pointermove", (e) => this.onMove(e));
    this.canvas.addEventListener("pointerup", () => this.onUp());
    this.canvas.addEventListener("pointerleave", () => this.onUp());
  }

  private toWorld(e: PointerEvent): Vec2 {
    const rect = this.canvas.getBoundingClientRect();
    const x = ((e.clientX - rect.left) / this.scalePx) * this.ir.scale_m_per_px;
    const y = ((e.clientY - rect.top) / this.scalePx) * this.ir.scale_m_per_px;
    return [x, y];
  }

  private toScreen(p: Vec2): { x: number; y: number } {
    return {
      x: (vx(p) / this.ir.scale_m_per_px) * this.scalePx,
      y: (vy(p) / this.ir.scale_m_per_px) * this.scalePx,
    };
  }

  private onDown(e: PointerEvent) {
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

    for (const w of this.ir.walls) {
      const sa = this.toScreen(w.a);
      const sb = this.toScreen(w.b);
      const sx = e.clientX - this.canvas.getBoundingClientRect().left;
      const sy = e.clientY - this.canvas.getBoundingClientRect().top;
      if (Math.hypot(sa.x - sx, sa.y - sy) < 10) {
        this.selectedWallId = w.id;
        this.drag = { wallId: w.id, end: "a" };
        this.draw();
        return;
      }
      if (Math.hypot(sb.x - sx, sb.y - sy) < 10) {
        this.selectedWallId = w.id;
        this.drag = { wallId: w.id, end: "b" };
        this.draw();
        return;
      }
    }
    const wall = this.hitWall(p);
    this.selectedWallId = wall?.id ?? null;
    this.draw();
  }

  private onMove(e: PointerEvent) {
    if (!this.drag) return;
    const p = this.toWorld(e);
    const wall = this.ir.walls.find((w) => w.id === this.drag!.wallId);
    if (!wall) return;
    if (this.drag.end === "a") wall.a = p;
    else wall.b = p;
    this.draw();
  }

  private onUp() {
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
    let bestD = 0.25;
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
    const { ctx, canvas } = this;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (this.image) {
      ctx.drawImage(this.image, 0, 0, canvas.width, canvas.height);
      ctx.fillStyle = "rgba(10,14,12,0.35)";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
    }

    for (const room of this.ir.rooms) {
      if (room.polygon.length < 3) continue;
      ctx.beginPath();
      room.polygon.forEach((p, i) => {
        const s = this.toScreen(p);
        if (i === 0) ctx.moveTo(s.x, s.y);
        else ctx.lineTo(s.x, s.y);
      });
      ctx.closePath();
      ctx.fillStyle = "rgba(111, 158, 138, 0.18)";
      ctx.fill();
    }

    for (const w of this.ir.walls) {
      const a = this.toScreen(w.a);
      const b = this.toScreen(w.b);
      ctx.strokeStyle = w.id === this.selectedWallId ? "#c4a35a" : "#e8efe6";
      ctx.lineWidth = w.id === this.selectedWallId ? 3 : 2;
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
      ctx.fillStyle = "#c4a35a";
      ctx.beginPath();
      ctx.arc(a.x, a.y, 5, 0, Math.PI * 2);
      ctx.arc(b.x, b.y, 5, 0, Math.PI * 2);
      ctx.fill();
    }

    for (const o of this.ir.openings) {
      const wall = this.ir.walls.find((w) => w.id === o.wall_id);
      if (!wall) continue;
      const mx = vx(wall.a) + (vx(wall.b) - vx(wall.a)) * ((o.t0 + o.t1) / 2);
      const my = vy(wall.a) + (vy(wall.b) - vy(wall.a)) * ((o.t0 + o.t1) / 2);
      const s = this.toScreen([mx, my]);
      ctx.fillStyle = o.kind === "door" ? "#8b5a2b" : "#6fa8dc";
      ctx.fillRect(s.x - 6, s.y - 6, 12, 12);
    }

    if (this.pendingStart) {
      const s = this.toScreen(this.pendingStart);
      ctx.fillStyle = "#c4a35a";
      ctx.beginPath();
      ctx.arc(s.x, s.y, 6, 0, Math.PI * 2);
      ctx.fill();
    }
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
