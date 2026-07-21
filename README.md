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
# 前端
cd web && npm install && npm run build && cd ..

# 后端（默认 :8080，托管 web/dist）
cargo run -p openspace-api --release
```

打开 http://127.0.0.1:8080

开发时也可：

```bash
# 终端 1
cargo run -p openspace-api

# 终端 2
cd web && npm run dev   # Vite :5173，代理 /api → :8080
```

环境变量：

- `OPENSPACE_DATA` — 数据目录（默认 `data/`）
- `OPENSPACE_WEB` — 静态前端目录（默认 `web/dist`）
- `PORT` — 端口（默认 `8080`）

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
