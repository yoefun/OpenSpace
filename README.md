# OpenSpace

纯 Rust 户型图重建 3D MVP：上传栅格户型图 + 室内照片 → 自动墙线检测 → 半自动校正 → 几何拉伸与贴纹理 → 浏览器浏览 glTF。

**不引入 Python。** AI 室内设计与 ONNX 推理仅预留接口。

## 架构

| 组件 | 说明 |
|------|------|
| `openspace-core` | `FloorplanIR`、项目状态机 |
| `openspace-vision` | 栅格检测（阈值/边缘/正交墙线） |
| `openspace-mesh` | 墙体拉伸、地板/天花、glTF/GLB 导出 |
| `openspace-texture` | 按房间照片采样材质 |
| `openspace-plugin` | `ReconstructionBackend`；`GeometricBackend` + `OnnxBackend` stub |
| `openspace-api` | Axum + SQLite + 作业队列 + SSE |
| `web/` | Vite + TypeScript + Three.js |

## 要求

- Rust stable（推荐 1.85+，仓库含 `rust-toolchain.toml`）
- Node.js 20+（构建前端）
- 无需 GPU（几何 MVP）

## 快速开始

```bash
# 前端（web/.npmrc 已固定使用 registry.npmjs.org）
cd web && npm install && npm run build && cd ..

# 后端（默认 :8080，托管 web/dist）
cargo run -p openspace-api --release

# 端口被占用时换端口（注意 `--` 后面才是程序参数）
cargo run -p openspace-api --release -- --port 8081
```

打开 http://127.0.0.1:8080（或你指定的端口）

PowerShell 示例：

```powershell
cargo run -p openspace-api --release -- --port 8081
# 或
$env:PORT=8081; cargo run -p openspace-api --release
```

查看全部参数：`cargo run -p openspace-api --release -- --help`

### Windows / 私有 npm 源

若本机默认 registry 是公司源（例如 `npm.shopee.io`）导致 `E404`，本仓库 `web/.npmrc` 会强制走官方源。仍失败时可：

```powershell
cd web
Remove-Item -Recurse -Force node_modules -ErrorAction SilentlyContinue
npm cache clean --force
npm install --registry=https://registry.npmjs.org/
npm run build
```

若出现 `EPERM` 删不掉 `node_modules`，先关掉占用该目录的 IDE/杀毒实时扫描，或重启终端后再删。

开发时也可：

```bash
# 终端 1
cargo run -p openspace-api -- --port 8080

# 终端 2
cd web && npm run dev   # Vite :5173，代理 /api → :8080
```

CLI / 环境变量：

| 参数 | 环境变量 | 默认 | 说明 |
|------|----------|------|------|
| `--port` / `-p` | `PORT` | `8080` | HTTP 端口 |
| `--host` | `OPENSPACE_HOST` | `0.0.0.0` | 监听地址 |
| `--data` | `OPENSPACE_DATA` | `data/` | SQLite 与上传文件目录 |
| `--web` | `OPENSPACE_WEB` | `web/dist` | 前端静态资源目录 |
## 流程

1. **上传**：户型图 JPG/PNG；可选照片并填房间名  
2. **检测**：`POST /api/projects/:id/detect`  
3. **校正**：拖墙端点、加墙/门/窗、设比例尺与层高  
4. **重建**：确认后入队 meshing → texturing → `model.glb`  
5. **查看**：Three.js 加载 glb  

`POST /api/projects/:id/design` 返回 **501**（AI 设计占位）。

## 测试

```bash
cargo test --workspace
```

样例栅格图：`fixtures/floorplans/synthetic_ortho.png`  
样例照片：`fixtures/photos/living_red.png`

## 后续路线

- `OnnxBackend`：布局/分割 ONNX 推理（仍无 Python 运行时）
- AI 设计：风格提示 → 效果图 / 材质替换
- 更完善的房间拓扑与门窗 CSG

## 参考（不直接依赖）

Plan2Scene、SpatialLM、Lyra 2.0 — 仅作能力对照。

## 作者

yoefun（xinglinsky@outlook.com）
