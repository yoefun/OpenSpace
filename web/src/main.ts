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
import { FloorplanEditor, type EditorTool } from "./editor";
import { SceneViewer } from "./viewer";

const app = document.querySelector<HTMLDivElement>("#app")!;

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
  else if (page === "editor") renderEditor(pageEl);
  else renderViewer(pageEl);
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
      <h3 style="margin-top:0;font-family:var(--font-display)">室内照片（可选）</h3>
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

  el.innerHTML = `
    <h2>半自动校正</h2>
    <p class="lede">滚轮缩放 · Shift+拖拽平移 · 拖墙端点修正。墙线过多时可先「简化墙线」再重建。</p>
    <div class="toolbar" id="tools">
      <button data-tool="select" class="active">选择/拖拽</button>
      <button data-tool="pan">平移</button>
      <button data-tool="add-wall">加墙</button>
      <button data-tool="add-door">门</button>
      <button data-tool="add-window">窗</button>
      <button id="del">删除选中墙</button>
      <span class="toolbar-sep"></span>
      <button id="zoom-out" title="缩小">−</button>
      <button id="zoom-fit" title="适应窗口">适应</button>
      <button id="zoom-in" title="放大">+</button>
      <button class="btn secondary" id="simplify">简化墙线</button>
    </div>
    <div class="editor-layout">
      <div class="canvas-wrap"><canvas id="fp"></canvas></div>
      <div class="panel">
        <label>比例尺 m/px
          <input type="number" step="0.0001" id="scale" value="${project.ir.scale_m_per_px}" />
        </label>
        <label style="margin-top:0.75rem">层高 m
          <input type="number" step="0.1" id="height" value="${project.ir.default_wall_height_m}" />
        </label>
        <label style="margin-top:0.75rem">墙厚 m
          <input type="number" step="0.01" id="thick" value="${project.ir.default_wall_thickness_m}" />
        </label>
        <div class="row" style="margin-top:1rem">
          <button class="btn secondary" id="save">保存 IR</button>
          <button class="btn" id="confirm">确认重建</button>
        </div>
        <button class="btn ghost-disabled" id="design" disabled title="MVP 占位">AI 设计（即将推出）</button>
        <div class="status" id="estatus">墙 ${project.ir.walls.length} · 房间 ${project.ir.rooms.length}</div>
        <div class="progress-bar"><span id="pbar"></span></div>
      </div>
    </div>
  `;

  const canvas = el.querySelector<HTMLCanvasElement>("#fp")!;
  editor?.destroy();
  editor = new FloorplanEditor(canvas, project.ir);
  await editor.loadImage(`/api/projects/${project.id}/floorplan`);
  editor.onChange = (ir) => {
    project!.ir = ir;
    el.querySelector("#estatus")!.textContent = `墙 ${ir.walls.length} · 房间 ${ir.rooms.length}`;
  };

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

  el.querySelector("#simplify")!.addEventListener("click", async () => {
    const status = el.querySelector("#estatus")!;
    try {
      applyMeta();
      project = await putIr(project!.id, editor!.ir);
      project = await simplifyIr(project.id);
      editor!.setIr(project.ir);
      status.textContent = `已简化：墙 ${project.ir.walls.length} · 房间 ${project.ir.rooms.length}`;
      status.className = "status ok";
    } catch (e) {
      status.textContent = String(e);
      status.className = "status error";
    }
  });

  const applyMeta = () => {
    if (!editor || !project) return;
    editor.ir.scale_m_per_px = Number((el.querySelector("#scale") as HTMLInputElement).value);
    editor.ir.default_wall_height_m = Number((el.querySelector("#height") as HTMLInputElement).value);
    editor.ir.default_wall_thickness_m = Number((el.querySelector("#thick") as HTMLInputElement).value);
    for (const w of editor.ir.walls) {
      w.height_m = editor.ir.default_wall_height_m;
      w.thickness_m = editor.ir.default_wall_thickness_m;
    }
    editor.draw();
  };

  el.querySelector("#save")!.addEventListener("click", async () => {
    applyMeta();
    const status = el.querySelector("#estatus")!;
    try {
      project = await putIr(project!.id, editor!.ir);
      status.textContent = "已保存";
      status.className = "status ok";
    } catch (e) {
      status.textContent = String(e);
      status.className = "status error";
    }
  });

  el.querySelector("#confirm")!.addEventListener("click", async () => {
    applyMeta();
    const status = el.querySelector("#estatus")!;
    const pbar = el.querySelector<HTMLElement>("#pbar")!;
    const confirmBtn = el.querySelector<HTMLButtonElement>("#confirm")!;
    confirmBtn.disabled = true;
    try {
      project = await putIr(project!.id, editor!.ir);
      unsub?.();
      const onProgress = (msg: string, progress: number, ok?: boolean) => {
        pbar.style.width = `${Math.round(progress * 100)}%`;
        status.textContent = msg;
        status.className = ok === false ? "status error" : ok ? "status ok" : "status";
      };
      unsub = subscribeEvents(project.id, (ev) => {
        onProgress(`${ev.status}: ${ev.message}`, ev.progress, ev.status === "ready" ? true : ev.status === "failed" ? false : undefined);
      });
      project = await build(project.id);
      onProgress("重建中…", project.progress);
      project = await waitForBuild(project.id, (p) => {
        onProgress(`${p.status}: ${p.progress_message}`, p.progress);
      });
      onProgress(`完成：${project.progress_message}`, 1, true);
      setTimeout(() => render("viewer"), 400);
    } catch (e) {
      status.textContent = String(e);
      status.className = "status error";
    } finally {
      confirmBtn.disabled = false;
    }
  });
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
    <p class="lede">浏览重建的室内网格。点击房间聚焦。</p>
    <div class="editor-layout">
      <div id="viewer-host" style="min-height:520px"></div>
      <div class="panel">
        <div class="status" id="vstatus">状态：${project.status}</div>
        <div class="progress-bar"><span id="vpbar" style="width:${Math.round(project.progress * 100)}%"></span></div>
        <button class="btn secondary" id="reload" style="margin-top:0.75rem">刷新模型</button>
        <h3 style="font-family:var(--font-display);margin:1rem 0 0.25rem">房间</h3>
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
