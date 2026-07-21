import "./styles.css";
import {
  build,
  createProject,
  detect,
  getProject,
  putIr,
  simplifyIr,
  subscribeEvents,
  uploadPhotos,
  waitForBuild,
  type FloorplanIR,
  type Project,
} from "./api";
import { mountBackground } from "./bg";
import { FloorplanEditor, type EditorTool } from "./editor";
import { SceneViewer } from "./viewer";

const app = document.querySelector<HTMLDivElement>("#app")!;
const bgRoot = document.querySelector<HTMLElement>("#bg-fx");
if (bgRoot) mountBackground(bgRoot);

let project: Project | null = null;
let editor: FloorplanEditor | null = null;
let viewer: SceneViewer | null = null;
let unsub: (() => void) | null = null;

function loadStoredId(): string | null {
  return localStorage.getItem("openspace_project_id");
}

function storeId(id: string) {
  localStorage.setItem("openspace_project_id", id);
}

type Page = "upload" | "editor" | "viewer";

function render(page: Page) {
  app.innerHTML = `
    <header class="app-bar">
      <h1 class="brand">Open<span>Space</span></h1>
      <nav class="tabs">
        <button data-page="upload" class="${page === "upload" ? "active" : ""}">上传</button>
        <button data-page="editor" class="${page === "editor" ? "active" : ""}">校正</button>
        <button data-page="viewer" class="${page === "viewer" ? "active" : ""}">3D 查看</button>
      </nav>
    </header>
    <main id="page"></main>
  `;

  app.querySelectorAll<HTMLButtonElement>("nav.tabs button").forEach((btn) => {
    btn.addEventListener("click", () => {
      const p = btn.dataset.page as Page;
      render(p);
    });
  });

  const pageEl = app.querySelector<HTMLElement>("#page")!;
  if (page === "upload") renderUpload(pageEl);
  else if (page === "editor") {
    void renderEditor(pageEl).catch((e) => {
      pageEl.innerHTML = `<p class="lede status error">校正页加载失败: ${e}</p>`;
    });
  } else renderViewer(pageEl);
}

function renderUpload(el: HTMLElement) {
  el.innerHTML = `
    <h2>上传户型图与照片</h2>
    <p class="lede">上传栅格户型图（JPG/PNG），可选室内照片并标注房间名。随后自动检测墙线，再进入半自动校正。</p>
    <div class="panel">
      <div class="row">
        <label>项目名称
          <input type="text" id="name" value="我的户型" />
        </label>
        <label>户型图
          <input type="file" id="floorplan" accept="image/png,image/jpeg" />
        </label>
        <button class="btn" id="create">创建并检测</button>
      </div>
      <div class="status" id="status"></div>
    </div>
    <div class="panel">
      <h3 class="panel-heading">室内照片（可选）</h3>
      <div class="row">
        <label>房间名
          <input type="text" id="room_name" placeholder="Room 1" />
        </label>
        <label>照片
          <input type="file" id="photos" accept="image/*" multiple />
        </label>
        <button class="btn secondary" id="upload-photos" ${project ? "" : "disabled"}>上传照片</button>
      </div>
      <p class="status">${project ? `当前项目：${project.name} (${project.id.slice(0, 8)}…) · 状态 ${project.status}` : "请先创建项目"}</p>
    </div>
  `;

  const status = el.querySelector<HTMLElement>("#status")!;

  el.querySelector("#create")!.addEventListener("click", async () => {
    const name = (el.querySelector("#name") as HTMLInputElement).value || "Untitled";
    const fileInput = el.querySelector("#floorplan") as HTMLInputElement;
    const file = fileInput.files?.[0];
    if (!file) {
      status.textContent = "请选择户型图文件";
      status.className = "status error";
      return;
    }
    status.textContent = "上传中…";
    status.className = "status";
    try {
      project = await createProject(name, file);
      storeId(project.id);
      status.textContent = "已创建，正在检测墙线…";
      project = await detect(project.id);
      status.textContent = `检测完成：${project.ir.walls.length} 面墙，${project.ir.rooms.length} 个房间。请进入「校正」。`;
      status.className = "status ok";
      render("editor");
    } catch (e) {
      status.textContent = String(e);
      status.className = "status error";
    }
  });

  el.querySelector("#upload-photos")?.addEventListener("click", async () => {
    if (!project) return;
    const files = (el.querySelector("#photos") as HTMLInputElement).files;
    if (!files?.length) return;
    const roomName = (el.querySelector("#room_name") as HTMLInputElement).value || undefined;
    try {
      project = await uploadPhotos(project.id, files, roomName);
      status.textContent = `已上传 ${files.length} 张照片`;
      status.className = "status ok";
    } catch (e) {
      status.textContent = String(e);
      status.className = "status error";
    }
  });
}

async function renderEditor(el: HTMLElement) {
  if (!project) {
    const id = loadStoredId();
    if (id) {
      try {
        project = await getProject(id);
      } catch {
        /* ignore */
      }
    }
  }

  if (!project) {
    el.innerHTML = `<p class="lede">还没有项目。请先到「上传」页创建。</p>`;
    return;
  }

  const projectId = project.id;
  el.innerHTML = `
    <h2>半自动校正</h2>
    <p class="lede">滚轮缩放 · Shift+拖拽平移 · 拖墙端点修正。检测不准时可「重新检测」或「简化墙线」后再重建。</p>
    <div class="toolbar" id="tools">
      <button type="button" data-tool="select" class="tool-btn active">选择/拖拽</button>
      <button type="button" data-tool="pan" class="tool-btn">平移</button>
      <button type="button" data-tool="add-wall" class="tool-btn">加墙</button>
      <button type="button" data-tool="add-door" class="tool-btn">门</button>
      <button type="button" data-tool="add-window" class="tool-btn">窗</button>
      <button type="button" id="del" class="tool-btn">删除选中墙</button>
      <span class="toolbar-sep"></span>
      <button type="button" id="zoom-out" class="tool-btn icon-btn" title="缩小">−</button>
      <button type="button" id="zoom-fit" class="tool-btn" title="适应窗口">适应</button>
      <button type="button" id="zoom-in" class="tool-btn icon-btn" title="放大">+</button>
      <button type="button" class="btn secondary" id="redetect">重新检测</button>
      <button type="button" class="btn secondary" id="simplify">简化墙线</button>
    </div>
    <div class="editor-layout">
      <div class="canvas-wrap"><canvas id="fp"></canvas></div>
      <div class="panel">
        <label class="field">比例尺 m/px
          <input type="number" step="0.0001" id="scale" value="${project.ir.scale_m_per_px}" />
        </label>
        <label class="field">层高 m
          <input type="number" step="0.1" id="height" value="${project.ir.default_wall_height_m}" />
        </label>
        <label class="field">墙厚 m
          <input type="number" step="0.01" id="thick" value="${project.ir.default_wall_thickness_m}" />
        </label>
        <div class="side-actions">
          <button type="button" class="btn secondary" id="save">保存 IR</button>
          <button type="button" class="btn" id="confirm">确认重建</button>
        </div>
        <button type="button" class="btn ghost-disabled" id="design" disabled title="MVP 占位">AI 设计（即将推出）</button>
        <div class="status" id="estatus">墙 ${project.ir.walls.length} · 房间 ${project.ir.rooms.length}</div>
        <div class="progress-bar"><span id="pbar"></span></div>
      </div>
    </div>
  `;

  const statusEl = () => el.querySelector<HTMLElement>("#estatus")!;
  const pbarEl = () => el.querySelector<HTMLElement>("#pbar")!;

  const applyMeta = () => {
    if (!editor || !project) return;
    const scale = (el.querySelector("#scale") as HTMLInputElement).value;
    const height = (el.querySelector("#height") as HTMLInputElement).value;
    const thick = (el.querySelector("#thick") as HTMLInputElement).value;
    editor.ir.scale_m_per_px = Number(scale) || project.ir.scale_m_per_px;
    editor.ir.default_wall_height_m = Number(height) || project.ir.default_wall_height_m;
    editor.ir.default_wall_thickness_m = Number(thick) || project.ir.default_wall_thickness_m;
    for (const w of editor.ir.walls) {
      w.height_m = editor.ir.default_wall_height_m;
      w.thickness_m = editor.ir.default_wall_thickness_m;
    }
    editor.draw();
  };

  const canvas = el.querySelector<HTMLCanvasElement>("#fp")!;
  editor?.destroy();
  editor = new FloorplanEditor(canvas, project.ir);
  editor.onChange = (ir) => {
    if (project) project.ir = ir;
    statusEl().textContent = `墙 ${ir.walls.length} · 房间 ${ir.rooms.length}`;
  };

  // Bind handlers BEFORE async image load — otherwise a failed loadImage leaves buttons dead.
  el.querySelectorAll<HTMLButtonElement>("#tools button[data-tool]").forEach((btn) => {
    btn.addEventListener("click", () => {
      el.querySelectorAll("#tools button[data-tool]").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      editor!.tool = btn.dataset.tool as EditorTool;
    });
  });

  el.querySelector("#del")!.addEventListener("click", () => editor?.deleteSelected());
  el.querySelector("#zoom-in")!.addEventListener("click", () => editor?.zoomIn());
  el.querySelector("#zoom-out")!.addEventListener("click", () => editor?.zoomOut());
  el.querySelector("#zoom-fit")!.addEventListener("click", () => editor?.fitToView());

  el.querySelector("#redetect")!.addEventListener("click", async () => {
    const status = statusEl();
    try {
      status.textContent = "重新检测中…";
      status.className = "status";
      project = await detect(projectId);
      editor!.setIr(project.ir);
      try {
        await editor!.loadImage(`/api/projects/${projectId}/floorplan`);
      } catch (e) {
        status.textContent = `检测完成但底图加载失败: ${e}`;
        status.className = "status error";
        return;
      }
      status.textContent = `检测完成：墙 ${project.ir.walls.length} · 房间 ${project.ir.rooms.length}`;
      status.className = project.ir.walls.length <= 30 ? "status ok" : "status";
    } catch (e) {
      status.textContent = String(e);
      status.className = "status error";
    }
  });

  el.querySelector("#simplify")!.addEventListener("click", async () => {
    const status = statusEl();
    try {
      applyMeta();
      project = await putIr(projectId, editor!.ir);
      project = await simplifyIr(projectId);
      editor!.setIr(project.ir);
      status.textContent = `已简化：墙 ${project.ir.walls.length} · 房间 ${project.ir.rooms.length}`;
      status.className = "status ok";
    } catch (e) {
      status.textContent = String(e);
      status.className = "status error";
    }
  });

  el.querySelector("#save")!.addEventListener("click", async () => {
    applyMeta();
    const status = statusEl();
    try {
      project = await putIr(projectId, editor!.ir);
      status.textContent = "已保存";
      status.className = "status ok";
    } catch (e) {
      status.textContent = String(e);
      status.className = "status error";
    }
  });

  el.querySelector("#confirm")!.addEventListener("click", async () => {
    const status = statusEl();
    const pbar = pbarEl();
    const confirmBtn = el.querySelector<HTMLButtonElement>("#confirm")!;

    status.textContent = "正在提交重建…";
    status.className = "status";
    pbar.style.width = "8%";
    confirmBtn.disabled = true;

    try {
      if (!editor || !project) {
        throw new Error("编辑器未就绪，请刷新页面");
      }
      applyMeta();
      project = await putIr(projectId, editor.ir);
      status.textContent = "已保存，正在入队重建…";
      pbar.style.width = "20%";

      unsub?.();
      const onProgress = (msg: string, progress: number, ok?: boolean) => {
        pbar.style.width = `${Math.max(8, Math.round(progress * 100))}%`;
        status.textContent = msg;
        status.className = ok === false ? "status error" : ok ? "status ok" : "status";
      };

      unsub = subscribeEvents(projectId, (ev) => {
        onProgress(
          `${ev.status}: ${ev.message}`,
          ev.progress,
          ev.status === "ready" ? true : ev.status === "failed" ? false : undefined,
        );
      });

      project = await build(projectId);
      onProgress(`已入队 · ${project.progress_message}`, project.progress);
      project = await waitForBuild(projectId, (p) => {
        onProgress(`${p.status}: ${p.progress_message}`, p.progress);
      });
      onProgress(`完成：${project.progress_message}`, 1, true);
      setTimeout(() => render("viewer"), 400);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      status.textContent = msg.includes("fetch")
        ? `请求失败：请确认后端已启动 (openspace-api) 且页面地址正确。${msg}`
        : msg;
      status.className = "status error";
      pbar.style.width = "0%";
    } finally {
      confirmBtn.disabled = false;
    }
  });

  try {
    await editor.loadImage(`/api/projects/${projectId}/floorplan`);
  } catch (e) {
    statusEl().textContent = `底图加载失败（仍可编辑墙线）: ${e}`;
    statusEl().className = "status error";
  }
}

async function renderViewer(el: HTMLElement) {
  if (!project) {
    const id = loadStoredId();
    if (id) {
      try {
        project = await getProject(id);
      } catch {
        /* ignore */
      }
    }
  }

  if (!project) {
    el.innerHTML = `<p class="lede">还没有项目。</p>`;
    return;
  }

  el.innerHTML = `
    <h2>3D 场景</h2>
    <p class="lede">浏览重建的室内网格（开口俯视，无顶棚）。点击房间聚焦。</p>
    <div class="editor-layout">
      <div id="viewer-host"></div>
      <div class="panel">
        <div class="status" id="vstatus">状态：${project.status}</div>
        <div class="progress-bar"><span id="vpbar" style="width:${Math.round(project.progress * 100)}%"></span></div>
        <div class="side-actions">
          <button type="button" class="btn secondary" id="reload">刷新模型</button>
        </div>
        <h3 class="panel-heading" style="margin-top:1.1rem">房间</h3>
        <ul class="room-list" id="rooms"></ul>
      </div>
    </div>
  `;

  const host = el.querySelector<HTMLElement>("#viewer-host")!;
  viewer?.dispose();
  viewer = new SceneViewer(host);

  const rooms = el.querySelector("#rooms")!;
  rooms.innerHTML = project.ir.rooms
    .map((r) => `<li data-name="${r.name}">${r.name}</li>`)
    .join("");
  rooms.querySelectorAll("li").forEach((li) => {
    li.addEventListener("click", () => viewer?.focusRoom(li.getAttribute("data-name") || ""));
  });

  const load = async () => {
    project = await getProject(project!.id);
    const status = el.querySelector("#vstatus")!;
    status.textContent = `状态：${project.status} · ${project.progress_message}`;
    if (project.status === "ready") {
      await viewer!.loadGlb(`/api/projects/${project.id}/model.glb?t=${Date.now()}`);
      status.className = "status ok";
    } else {
      status.className = "status";
    }
  };

  el.querySelector("#reload")!.addEventListener("click", () => {
    load().catch((e) => {
      el.querySelector("#vstatus")!.textContent = String(e);
      el.querySelector("#vstatus")!.className = "status error";
    });
  });

  unsub?.();
  unsub = subscribeEvents(project.id, (ev) => {
    const pbar = el.querySelector<HTMLElement>("#vpbar")!;
    pbar.style.width = `${Math.round(ev.progress * 100)}%`;
    const status = el.querySelector("#vstatus")!;
    status.textContent = `${ev.status}: ${ev.message}`;
    if (ev.status === "ready") {
      load().catch(console.error);
    }
  });

  load().catch((e) => {
    el.querySelector("#vstatus")!.textContent = String(e);
    el.querySelector("#vstatus")!.className = "status error";
  });
}

// silence unused import in case of tree shake
void (null as unknown as FloorplanIR);

async function boot() {
  const id = loadStoredId();
  if (id) {
    try {
      project = await getProject(id);
    } catch {
      localStorage.removeItem("openspace_project_id");
    }
  }
  render(project ? "editor" : "upload");
}

boot();
