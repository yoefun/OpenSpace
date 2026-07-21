export type Vec2 = [number, number];

export type WallSegment = {
  id: string;
  a: Vec2;
  b: Vec2;
  thickness_m: number;
  height_m: number;
};

export type Opening = {
  id: string;
  wall_id: string;
  kind: "door" | "window";
  t0: number;
  t1: number;
  sill_m: number | null;
  height_m: number;
};

export type Room = {
  id: string;
  name: string;
  polygon: Vec2[];
  wall_ids: string[];
};

export type FloorplanIR = {
  walls: WallSegment[];
  openings: Opening[];
  rooms: Room[];
  scale_m_per_px: number;
  default_wall_height_m: number;
  default_wall_thickness_m: number;
};

export type PhotoRef = {
  id: string;
  path: string;
  room_id: string | null;
  room_name: string | null;
  notes: string | null;
};

export type Project = {
  id: string;
  name: string;
  floorplan_image: string;
  photos: PhotoRef[];
  ir: FloorplanIR;
  status: string;
  error: string | null;
  created_at: string;
  updated_at: string;
  model_path: string | null;
  progress: number;
  progress_message: string;
};

export type ProgressEvent = {
  project_id: string;
  status: string;
  progress: number;
  message: string;
};

async function parseJson<T>(res: Response): Promise<T> {
  if (!res.ok) {
    let msg = res.statusText;
    try {
      const body = await res.json();
      msg = body.error || msg;
    } catch {
      /* ignore */
    }
    throw new Error(msg);
  }
  return res.json() as Promise<T>;
}

export async function createProject(name: string, file: File): Promise<Project> {
  const fd = new FormData();
  fd.append("name", name);
  fd.append("floorplan", file);
  const res = await fetch("/api/projects", { method: "POST", body: fd });
  return parseJson(res);
}

export async function getProject(id: string): Promise<Project> {
  const res = await fetch(`/api/projects/${id}`);
  return parseJson(res);
}

export async function uploadPhotos(
  id: string,
  files: FileList,
  roomName?: string,
): Promise<Project> {
  const fd = new FormData();
  if (roomName) fd.append("room_name", roomName);
  for (const f of Array.from(files)) {
    fd.append("photo", f);
  }
  const res = await fetch(`/api/projects/${id}/photos`, { method: "POST", body: fd });
  return parseJson(res);
}

export async function detect(id: string): Promise<Project> {
  const res = await fetch(`/api/projects/${id}/detect`, { method: "POST" });
  return parseJson(res);
}

export async function putIr(id: string, ir: FloorplanIR): Promise<Project> {
  const res = await fetch(`/api/projects/${id}/ir`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(ir),
  });
  return parseJson(res);
}

export async function build(id: string): Promise<Project> {
  const res = await fetch(`/api/projects/${id}/build`, { method: "POST" });
  return parseJson(res);
}

export async function simplifyIr(id: string): Promise<Project> {
  const res = await fetch(`/api/projects/${id}/ir/simplify`, { method: "POST" });
  return parseJson(res);
}

/** Poll until ready/failed (SSE fallback for Windows / flaky EventSource). */
export async function waitForBuild(
  id: string,
  onProgress?: (p: Project) => void,
  timeoutMs = 120_000,
): Promise<Project> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const p = await getProject(id);
    onProgress?.(p);
    if (p.status === "ready") return p;
    if (p.status === "failed") {
      throw new Error(p.error || p.progress_message || "重建失败");
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error("重建超时，请检查后端日志或刷新页面");
}

export function subscribeEvents(
  id: string,
  onEvent: (ev: ProgressEvent) => void,
): () => void {
  const es = new EventSource(`/api/projects/${id}/events`);
  es.addEventListener("progress", (e) => {
    try {
      onEvent(JSON.parse((e as MessageEvent).data));
    } catch {
      /* ignore */
    }
  });
  return () => es.close();
}
