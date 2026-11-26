# photoSelf — 自托管照片 / 视频画廊技术设计方案

> 文档版本：v0.1（Draft）
> 日期：2026-11-26
> 状态：设计评审稿，尚未进入实现
> 目标：一个面向低资源设备与家庭自托管场景的开源照片/视频浏览与管理服务
> 技术栈：Rust（后端）+ Vue 3（前端）
> 项目名：photoSelf

---

## 1. 摘要

photoSelf 是一个**轻量、自托管、配置驱动**的照片与视频画廊：

- 后端：单个 Rust 二进制，负责索引、元数据、缩略图、搜索、标签、任务队列；
- 前端：Vue 3 SPA，提供时间线、网格、灯箱、视频播放、标签与相册；
- 文件系统是唯一事实来源，数据库只是可重建的索引 + 用户元数据；
- 默认只读，写入能力（上传/删除/移动）显式开启，并带回收站；
- 媒体文件优先交给反向代理 / 静态文件服务，Rust 不转发大文件；
- 可在 x86、ARM64、ARMv7、NAS、Docker、OpenWrt 等环境运行；
- 所有环境差异（路径、端口、认证、缓存、转码、并发）都通过配置处理，核心代码不包含设备特例。

---

## 2. 背景与动机

现有方案在“低资源 + 已有目录 + 视频较多 + 想复用现有 Web 基础设施”这个组合下都不够理想：

| 方案 | 问题 |
|---|---|
| Immich / PhotoPrism | 资源需求高（数 GB 内存、数据库、ML），不适合 512MB～1GB 设备 |
| Piwigo / Lychee | 功能全，但 PHP + 数据库/较重运行时；视频与低内存场景体验一般 |
| FileBrowser | 已归档，不再修复安全问题 |
| Photofield | 浏览体验好，但只读、无认证、官方包为 glibc 动态链接 |
| 静态相册（Sigal 等） | 零运行时开销，但没有在线管理/标签/搜索 |

因此需要一个**聚焦型**产品：把“浏览体验”做好，把“重活”外包给现有基础设施（反向代理、WebDAV、ffmpeg），同时保持开源项目应有的通用性与可部署性。

---

## 3. 目标与非目标

### 3.1 目标

1. **低资源**：512MB 内存设备可运行；常驻 RSS 目标 < 150MB；缩略图峰值可控。
2. **通用**：不绑定某一路由器、NAS 或 Web 服务器；通过配置适配不同部署。
3. **零迁移**：直接索引已有目录，不要求导入/搬库；支持多个库目录。
4. **浏览优先**：时间线、虚拟滚动、懒加载、灯箱、视频播放、EXIF。
5. **安全写入**：默认只读；写操作可关闭；删除进回收站；原子写。
6. **可重建**：删除数据库后可从文件系统重建；用户元数据可导出/导入。
7. **易部署**：单二进制 + 嵌入式前端；Docker / systemd / procd / 反向代理样例齐全。
8. **可维护**：纯 Rust 核心依赖尽量少 C；CI 覆盖多架构；有 ADR 与迁移策略。

### 3.2 非目标（v1 不做）

- 人脸识别、物体识别、语义搜索、AI 相册；
- 多人 SaaS / 租户隔离 / 复杂权限模型；
- 原生 iOS/Android App（先做 PWA）；
- 云盘同步、去重合并、照片编辑；
- 视频剪辑、实时转码；
- 插件市场 / 脚本系统；
- 与 Immich/PhotoPrism 的数据模型兼容。

---

## 4. 用户与场景

| 用户 | 场景 |
|---|---|
| Homelab / NAS 用户 | 已有大量照片目录，想用轻量 UI 浏览与打标签 |
| 低配 ARM 设备用户 | OpenWrt 路由器、树莓派、ARM 小主机，内存 512MB–2GB |
| 已有 Web 基础设施的用户 | 已有 Nginx/Caddy/lighttpd、HTTPS、Basic Auth，想复用 |
| 开发者 / 开源贡献者 | 想要可读、可扩展、可交叉编译的 Rust + Vue 项目 |

典型规模：1 万～10 万媒体项；单家庭/小团队；局域网或 WireGuard 访问。

---

## 5. 设计原则

1. **文件系统是事实来源**：照片原目录不被修改；DB 可随时重建。
2. **默认只读**：浏览、缩略图、搜索不需要写权限；写模式显式开启。
3. **配置优于代码**：设备差异、路径、认证、代理、转码都进配置；核心代码无设备专属分支。
4. **复用系统能力**：TLS、Basic Auth、大文件 Range、静态缓存优先交给反向代理。
5. **失败安全**：任何异常都不能删除/破坏原文件；删除必须可恢复。
6. **无聊的技术**：稳定、主流、可交叉编译的依赖；避免 C/系统库依赖。
7. **单二进制交付**：release 默认内嵌前端；开发时前后端分离。
8. **可观测**：结构化日志、健康检查、任务状态、可选的 Prometheus 指标。

---

## 6. 总体架构

```text
┌──────────────────────────────────────────────────────────────┐
│                         Browser (Vue 3 SPA)                  │
│   Timeline / Grid / Lightbox / Tags / Albums / Upload / Jobs  │
└───────────────▲──────────────────────────────▲───────────────┘
                │ HTTPS / HTTP                 │ 静态资源 / 媒体流
                │                              │
┌───────────────┴───────────────┐   ┌──────────┴───────────────┐
│ Reverse Proxy (可选但推荐)     │   │ 静态文件层（可选）        │
│ Nginx / Caddy / lighttpd      │   │ /media  → 照片原目录      │
│ - TLS                         │   │ /thumbs → 缩略图缓存      │
│ - Basic Auth / OIDC           │   │ 支持 Range / 缓存         │
│ - /api → 127.0.0.1:8787       │   └──────────┬───────────────┘
└───────────────┬───────────────┘              │
                │                              │
┌───────────────▼──────────────────────────────▼───────────────┐
│                    photoSelf Core (Rust)                     │
│  ┌──────────┐  ┌───────────┐  ┌────────────┐  ┌───────────┐  │
│  │ Scanner  │  │ Metadata  │  │ Thumbnail  │  │ Transcode │  │
│  │ (索引)    │  │ Extractor │  │ Worker     │  │ Worker    │  │
│  └────┬─────┘  └─────┬─────┘  └─────┬──────┘  └─────┬─────┘  │
│       │              │              │               │        │
│  ┌────▼──────────────▼──────────────▼───────────────▼─────┐  │
│  │              Store (SQLite) + Job Queue                │  │
│  └──────────────────────────┬─────────────────────────────┘  │
│                             │                                │
│  ┌──────────────────────────▼─────────────────────────────┐  │
│  │ HTTP API (axum) / Embedded SPA (optional)               │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
                │
        ┌───────▼────────┐
        │  Library Roots  │  （只读挂载优先）
        │  /photos/...    │
        └────────────────┘
```

**请求路径：**

- 浏览：SPA → `/api/v1/items` → SQLite → JSON；
- 缩略图：`<img src="/thumbs/...">` → 反向代理/静态层（命中缓存）→ 未命中时由 API 触发生成；
- 原图/视频：`<img>/<video src="/media/...">` → 静态层 Range 响应，不经过 Rust；
- 扫描：API/定时器 → Scanner → 元数据 → DB → 队列 → 缩略图 worker；
- 转码：任务队列 → ffmpeg → 派生文件 → DB。

> 对低资源设备，推荐“API 由 Rust、静态媒体由反向代理”的模式；对单机小规模部署，Rust 也可以内置静态/媒体服务，减少依赖。

---

## 7. 部署模式

通过配置切换，不编译不同分支。

| 模式 | 说明 | 适用 |
|---|---|---|
| **A. Standalone** | Rust 同时提供 SPA、API、缩略图、原图/视频（Range） | 单机、Docker、个人 NAS |
| **B. Reverse Proxy**（推荐） | Rust 只提供 API；TLS/Auth/静态媒体由 Nginx/Caddy/lighttpd 提供 | 已有 Web 基础设施、低资源设备 |
| **C. Metadata-only / 外部管理** | Rust 只读索引；上传/删除由 WebDAV/SMB/外部工具完成 | 想复用现有 WebDAV 的用户 |
| **D. Container** | 官方多架构 Docker 镜像，`/photos:ro`、`/data` 卷 | NAS / 服务器 |

配置项 `server.mode` 与 `media.serve_mode` 决定行为；示例见第 15 章。

---

## 8. 功能设计

### 8.1 版本规划

| 版本 | 范围 |
|---|---|
| **v0.1（MVP）** | 扫描、时间线/网格、缩略图、灯箱、EXIF、视频原片播放/下载、手动扫描、健康检查、配置、单用户认证（可选） |
| **v0.2** | 标签、收藏、搜索、筛选、回收站、上传/删除/重命名（内置写模式或外部 WebDAV 模式）、任务进度 |
| **v0.3** | 视频海报/元数据完善、可选后台转码、虚拟相册、PWA、i18n、暗色模式 |
| **v1.0** | 稳定 API、迁移工具、Docker/多架构发布、安全审计、文档与示例部署 |
| **后续** | 分享链接、多用户、智能相册、HEIC/RAW 增强、Face/AI（可选插件化，不承诺） |

### 8.2 功能清单

| 模块 | 功能 | 优先级 |
|---|---|---|
| 浏览 | 时间线（按天/月）、网格、虚拟滚动、懒加载、灯箱、键盘导航、缩放 | P0 |
| 媒体 | JPG/PNG/WebP 预览；MOV/MP4 播放或下载；EXIF/视频元数据 | P0 |
| 索引 | 多库目录、增量扫描、忽略规则、手动/定时扫描、扫描进度 | P0 |
| 元数据 | 标签、收藏、评分（可选）、相册（虚拟） | P1 |
| 搜索 | 文件名、日期范围、标签、类型、收藏；分页 | P1 |
| 写操作 | 上传、删除（回收站）、重命名、新建相册（虚拟） | P1 |
| 视频 | ffprobe 元数据、ffmpeg 海报、可选 MP4 派生 | P1 |
| 运维 | `/healthz`、任务列表、日志、配置校验、缓存清理 | P0 |
| 体验 | PWA、移动端布局、暗色模式、i18n | P2 |
| 安全 | 认证模式、CSRF、路径校验、上传校验、速率限制 | P0 |

---

## 9. 领域模型

### 9.1 核心实体

| 实体 | 字段（要点） |
|---|---|
| `Library` | `id`, `name`, `root_path`, `read_only`, `ignore_globs`, `enabled` |
| `MediaItem` | `id` (ULID), `library_id`, `rel_path`, `content_hash` (blake3), `kind` (image/video/other), `size`, `mtime`, `width`, `height`, `duration_ms`, `taken_at`, `camera`, `orientation`, `status` |
| `Tag` | `id`, `name`, `color` |
| `ItemTag` | `item_id`, `tag_id`, `created_at` |
| `Album` | `id`, `name`, `description`, `cover_item_id`, `created_at` |
| `AlbumItem` | `album_id`, `item_id`, `position` |
| `Job` | `id`, `type` (scan/thumb/transcode), `state`, `progress`, `payload`, `error`, `created_at`, `updated_at` |
| `User`（可选） | `id`, `username`, `password_hash` (Argon2id), `role` |
| `Session`（可选） | `id`, `user_id`, `expires_at`, `csrf_token` |

### 9.2 身份与重命名策略

`id` 使用 ULID，路径只是属性。扫描时：

1. `(library_id, rel_path)` 已存在 → 复用 `id`，更新 size/mtime/hash；
2. 新路径但 `content_hash` 命中一个“失去路径”的旧项 → 视为重命名，复用 `id`；
3. 否则创建新 `id`。

这样标签/收藏/相册在重命名后仍保留；内容被修改时同路径仍复用 `id`（更新 hash）。

### 9.3 虚拟相册与标签

- 不移动、不改名原文件；
- 相册/标签是 DB 中的元数据；
- 后续可增加“智能相册”（保存查询条件），v1 不做。

---

## 10. 存储设计

### 10.1 目录布局（默认）

```text
/data/                         # 数据目录（配置项 data_dir）
├── photoself.db              # SQLite
├── config.toml                # 可选：默认从 /etc/photoself/config.toml 或 --config 读取
├── cache/
│   ├── thumbs/                # 缩略图（内容寻址）
│   │   └── ab/cd/<hash>-320.jpg
│   ├── posters/               # 视频海报
│   ├── derived/               # 可选转码产物
│   └── tmp/                   # 原子写临时文件
├── trash/                     # 回收站（写模式）
│   └── 2026-11-26/<item-id>/
└── logs/                      # 可选：文件日志
```

### 10.2 SQLite

- 默认使用 **SQLite（WAL 模式）**：单文件、事务、生态成熟、备份简单；
- Rust 侧：`rusqlite` + `tokio-rusqlite`（异步封装）+ 内嵌迁移（`rusqlite_migration`）；
- 关键 pragma：`journal_mode=WAL`、`busy_timeout=5000`、`foreign_keys=ON`、`synchronous=NORMAL`；
- 核心表：`libraries`、`items`、`tags`、`item_tags`、`albums`、`album_items`、`jobs`、`settings`；
- 索引：`(library_id, rel_path)` 唯一、`content_hash`、`taken_at`、`kind`、`(favorite)`；
- 搜索 v1 用 `LIKE` + 标签过滤；FTS5 作为后续可选项（需确认构建特性）；
- 迁移：版本号表 + 顺序迁移；启动时自动迁移，失败拒绝启动并给出手动指引。

> 备选：`redb`（纯 Rust、无 C 依赖）可作为“极简构建”特性；v1 默认 SQLite，优先保证通用性与工具链。
> 交叉编译说明：`rusqlite` 使用 `bundled` 特性时需 C 编译器；CI 使用 `cargo-zigbuild`，OpenWrt 构建使用 SDK 工具链，详见第 17 章。

### 10.3 缩略图缓存

- 内容寻址：`<content_hash>-<size>.<format>`；哈希变化即自动失效；
- 原子写：`cache/tmp/xxx` → `rename`；
- 支持尺寸列表（如 320/1280），前端用 `srcset`；
- 可配置 JPEG/WebP；默认 JPEG（兼容性与 CPU 更稳）；
- 清理：按 LRU/总容量/最大年龄；提供 `photoself cache prune` 子命令。

### 10.4 数据库可重建性

- `items` 可从文件系统重建；
- 用户元数据（标签、收藏、相册）支持导出/导入 JSON；
- `photoself export-metadata` / `import-metadata` 子命令，作为备份与迁移手段。

---

## 11. API 设计

### 11.1 风格

- Base：`/api/v1`；JSON；错误使用 RFC 7807 `application/problem+json`；
- 分页：cursor（`taken_at,id` 或 `id`），避免 offset 深分页；
- 缓存：列表/详情返回 `ETag`，缩略图长期缓存 + 内容哈希文件名；
- 任务进度：Server-Sent Events（`/api/v1/events`），避免额外 WebSocket 依赖；
- OpenAPI 3.1：`utoipa` 生成，前端用 `openapi-typescript` 生成 TS 类型/客户端。

### 11.2 端点草案

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/healthz` | 存活/就绪 |
| GET | `/api/v1/info` | 版本、能力、模式、库列表摘要 |
| GET | `/api/v1/libraries` | 库列表 |
| POST | `/api/v1/scan` | 触发扫描（可指定库） |
| GET | `/api/v1/items` | 列表/时间线；`library/from/to/kind/tag/q/favorite/sort/cursor/limit` |
| GET | `/api/v1/items/{id}` | 详情 + 相邻项 |
| PATCH | `/api/v1/items/{id}` | 更新 favorite/tags/rating |
| DELETE | `/api/v1/items/{id}` | 移入回收站（写模式） |
| GET | `/api/v1/items/{id}/thumb/{size}` | 缩略图（standalone 模式） |
| GET | `/api/v1/items/{id}/media` | 原文件/视频（standalone，支持 Range） |
| GET/POST | `/api/v1/tags` | 标签 |
| GET/POST | `/api/v1/albums` | 相册 |
| POST/DELETE | `/api/v1/albums/{id}/items` | 相册成员 |
| POST | `/api/v1/items/{id}/transcode` | 请求生成 web 版本（写模式/后台） |
| GET | `/api/v1/jobs` / `/jobs/{id}` | 任务列表/详情 |
| POST | `/api/v1/upload` | 上传（写模式） |
| GET | `/api/v1/events` | SSE 任务进度 |
| GET | `/metrics` | 可选 Prometheus 指标（feature） |

### 11.3 媒体 URL 生成

- `media.serve_mode = "proxy"`：返回 `/media/<rel_path>`（由反向代理映射到库根目录）；
- `media.serve_mode = "standalone"`：返回 `/api/v1/items/{id}/media`（Rust 流式 + Range）；
- 缩略图同理：`/thumbs/...` 或 `/api/v1/items/{id}/thumb/...`。

前端只消费 API 返回的 URL，不关心部署模式。

---

## 12. 扫描与媒体处理

### 12.1 扫描器

- 遍历库根目录，支持忽略 glob（默认 `._*`、`.DS_Store`、`@eaDir`、`#recycle`、缓存目录）；
- 符号链接默认不跟随（可配置）；
- 增量判断：`mtime + size` 未变则跳过元数据解析；
- 触发：启动时（可关）、定时（cron 表达式/间隔）、手动 API、可选 inotify（Linux，带防抖）；
- 并发：扫描并发可配置，默认 2；DB 写入批量事务；
- 扫描结果：新增/更新/缺失/错误计数，写入 `jobs` 与日志；
- 缺失项处理：标记 `missing`，不立即删除元数据；可配置保留天数后清理。

### 12.2 元数据

- 图片：`kamadak-exif` 读取 `DateTimeOriginal`、相机制造商/型号、方向、GPS（可选）；
- 视频：`ffprobe -show_streams -show_format -of json` 获取时长、宽高、编码、创建时间；
- 回退：文件名日期（`IMG_20230101_...`）→ 文件 mtime；
- 时区：统一存 UTC；展示时用浏览器本地时区；
- HEIC/RAW：v1 标记为 `unsupported` 并提供下载；后续通过 ffmpeg/libheif 或预处理生成 JPEG 代理。

### 12.3 缩略图

- 图片：`image` crate 解码 + 缩放；**单任务并发默认 1**，按设备 profile 调整；
- 视频：`ffmpeg -ss <n> -i <src> -frames:v 1 -vf scale=... -q:v 4 <out>`；
- 生成时机：前端请求未命中时入队 + 可选的“预热扫描”；`photoself thumb --all` 批量预热；
- 内存保护：解码前检查像素数上限（可配置，如 100MP）；超大图使用 ffmpeg 缩放；
- 失败重试：指数退避 + 最大次数；失败原因写 DB，前端显示占位图。

### 12.4 视频派生（可选）

- 默认关闭；`video.transcode.enabled = false`；
- 开启后：后台任务队列，单并发，`nice`/`ionice`，超时与磁盘配额；
- 目标：H.264 + AAC MP4，`-movflags +faststart`，限制最大高度；
- 只处理明确请求或配置的规则，不在请求链路转码；
- 产物进 `cache/derived`，可清理；
- 浏览器不支持的原片仍提供下载。

---

## 13. 前端设计

### 13.1 技术栈

| 层 | 选型 |
|---|---|
| 框架 | Vue 3 + TypeScript + Vite |
| 状态 | Pinia（UI 状态）+ `@tanstack/vue-query`（服务端状态/缓存） |
| 路由 | Vue Router |
| UI | Naive UI 或 Element Plus（按需引入） |
| 灯箱 | PhotoSwipe 5 |
| 虚拟列表 | `vue-virtual-scroller` 或 CSS `content-visibility` |
| 日期 | date-fns |
| API | OpenAPI 生成的 TypeScript 客户端 |
| 测试 | Vitest + Playwright |

### 13.2 页面与交互

- `/` 时间线：按天/月分组，虚拟化 justified grid；缩略图 `srcset` + 懒加载 + 主色/模糊占位；
- `/albums`、`/tags`、`/search`；
- 详情：灯箱 + 侧栏（EXIF、标签、收藏、下载、删除/回收站）；
- 任务页：扫描/缩略图/转码进度（SSE）；
- 设置：只读展示配置摘要（不暴露敏感项）；
- 移动端：底部导航、手势滑动、PWA；
- 无障碍：键盘导航、焦点管理、alt 文本（文件名）。

### 13.3 性能

- 首屏只请求第一页（如 100 项）+ 缩略图 320；
- 滚动预取下一页；详情按需请求 1280 缩略图/原图；
- 图片 `decoding="async"`、`loading="lazy"`；
- 路由级代码分割；UI 库按需；
- 构建产物 gzip 目标 < 1MB。

---

## 14. 配置设计（通用核心）

### 14.1 原则

- 格式：**TOML**（可读、支持注释）；支持环境变量覆盖与 CLI 参数；
- 优先级：CLI > 环境变量（`PHOTOSELF__SECTION__KEY`）> 配置文件 > 默认值；
- 所有环境差异都进配置，代码不出现设备判断；
- 提供 `photoself config check` / `--check-config`，启动失败时给出明确路径与字段；
- 提供 profile 预设：`low-memory`、`nas`、`docker`，用户可覆盖。

### 14.2 通用示例

```toml
[server]
mode = "reverse-proxy"        # standalone | reverse-proxy
listen = "127.0.0.1:8787"     # standalone 时可 0.0.0.0:8080
base_path = "/"
ui_embedded = true
trusted_proxies = ["127.0.0.1/32", "::1/128"]

[data]
dir = "/data"

[[libraries]]
id = "default"
name = "Photos"
path = "/photos"
read_only = true
ignore_globs = ["._*", ".DS_Store", "@eaDir", "#recycle"]

[media]
serve_mode = "proxy"          # proxy | standalone
public_media_path = "/media"
public_thumb_path = "/thumbs"

[thumbnails]
dir = "/data/cache/thumbs"
sizes = [320, 1280]
format = "jpeg"
quality = 82
concurrency = 2
max_pixels = 100_000_000
prewarm_on_scan = false

[ffmpeg]
enabled = true
ffmpeg_path = "ffmpeg"
ffprobe_path = "ffprobe"
concurrency = 1
timeout_seconds = 120

[video]
poster_enabled = true
transcode_enabled = false
max_height = 1080

[auth]
mode = "session"              # none | proxy | session
# session 模式首次启动生成管理员密码哈希，或通过环境变量提供
session_ttl_hours = 720

[scan]
on_start = true
interval = "0"                # "0" 关闭定时扫描；或 "15m" / cron
concurrency = 2
follow_symlinks = false
missing_retention_days = 30

[trash]
enabled = true
retention_days = 30
dir = "/data/trash"

[logging]
level = "info"
format = "text"               # text | json
```

### 14.3 低资源 / OpenWrt profile（示例，不是核心特例）

```toml
# profiles/low-memory.toml（用户可基于此覆盖）
[thumbnails]
concurrency = 1
sizes = [320]
max_pixels = 50_000_000

[ffmpeg]
concurrency = 1

[video]
transcode_enabled = false

[scan]
concurrency = 1
```

一个具体的 低内存 OpenWrt 部署可以这样配置——只是配置，不是代码分支：

```toml
[server]
mode = "reverse-proxy"
listen = "127.0.0.1:8787"
base_path = "/photo"

[[libraries]]
id = "family"
path = "/mnt/photos"
read_only = true

[data]
dir = "/mnt/usb/photo-app/data"

[media]
serve_mode = "proxy"
public_media_path = "/photo/media"
public_thumb_path = "/photo/thumbs"

[thumbnails]
dir = "/mnt/usb/photo-app/data/cache/thumbs"
concurrency = 1
sizes = [320, 1280]

[ffmpeg]
enabled = true
concurrency = 1

[auth]
mode = "proxy"                # 由 lighttpd Basic Auth 负责

[scan]
ignore_globs = ["._*", ".DS_Store"]
```

---

## 15. 安全设计

### 15.1 威胁模型

- 暴露在局域网/远程 WireGuard 的 Web 应用；
- 恶意/误操作的上传、删除、路径穿越；
- 通过图片/视频解析触发的资源耗尽；
- 反向代理配置错误导致的未授权访问。

### 15.2 措施

| 领域 | 措施 |
|---|---|
| 认证 | `none`（仅可信 LAN）、`proxy`（信任代理头，校验来源 CIDR）、`session`（Argon2id + HttpOnly/Secure/SameSite=Strict Cookie） |
| CSRF | 同源 + 自定义头校验；session 模式使用双提交 token |
| 路径 | 只使用相对路径；`canonicalize` 后校验库根前缀；拒绝 `..`；符号链接默认不跟随 |
| 上传 | 大小限制、磁盘余量检查、临时文件 + 原子 rename、扩展名/魔数校验、文件名清洗 |
| 删除 | 进 `trash/`，保留期可配置，支持恢复；禁止递归删除任意路径 |
| 外部进程 | `Command` 传参、不经过 shell；超时 + 并发限制；输出路径固定 |
| 资源 | 请求体/并发/速率限制；缩略图像素数上限；任务队列上限 |
| 响应头 | CSP、`X-Content-Type-Options: nosniff`、`Referrer-Policy`、HSTS（代理层） |
| 运行 | 尽量非 root；只读挂载照片库；数据目录最小权限；日志脱敏 |
| 依赖 | `cargo-deny` + Dependabot；发布 SBOM；安全公告流程 |

### 15.3 反向代理样例

- Nginx：`/api/` → `127.0.0.1:8787`；`/media/`、`/thumbs/` 直接 `alias`；`auth_basic`；
- Caddy：`reverse_proxy /api/* 127.0.0.1:8787`；`file_server` + `basicauth`；
- lighttpd：`proxy.server` + `alias.url` + `auth.require`。

所有样例放在 `deploy/`，并在文档中标注“媒体目录只读、认证必须开启”。

---

## 16. 性能与容量

### 16.1 目标

| 指标 | 目标 |
|---|---|
| 最低内存 | 512MB 设备可运行（low-memory profile） |
| 常驻 RSS | 典型 < 150MB（不含 ffmpeg 峰值） |
| 索引 10k 文件 | < 30 秒（ARM A53，mtime 无变化） |
| 缩略图吞吐 | 单并发 > 1 张/秒（12MP JPEG，A53） |
| API p95 | 局域网 < 150ms（列表/详情） |
| 前端首屏 | 局域网 < 2 秒 |
| 规模 | 10 万项（设计目标），100 万项不承诺 |

### 16.2 内存控制

- 缩略图/ffmpeg 并发默认 1（low-memory）；
- 解码前像素数检查；超大图走 ffmpeg；
- 扫描器流式遍历，不一次性载入全部路径；
- API 分页 + 流式 JSON；
- `procd`/systemd 配置内存上限，OOM 时重启而不是拖死系统。

---

## 17. 构建、打包与发布

### 17.1 仓库结构（建议）

```text
photoself/
├── Cargo.toml                 # workspace
├── crates/
│   ├── photoself-server/     # 二进制入口（axum、配置、路由）
│   ├── photoself-core/       # 领域模型、扫描、索引、缩略图、任务
│   ├── photoself-media/      # ffmpeg/ffprobe 封装、图像处理
│   ├── photoself-db/         # SQLite、迁移、仓储
│   └── photoself-api-types/  # DTO、OpenAPI 模型
├── ui/                        # Vue 3 + Vite
├── deploy/
│   ├── systemd/photoself.service
│   ├── openwrt/photoself.init
│   ├── nginx/photoself.conf
│   ├── caddy/Caddyfile
│   └── lighttpd/94-photoself.conf
├── docs/                      # 设计、ADR、部署、API
├── scripts/                   # 构建、交叉编译、发布
├── config.example.toml
├── Dockerfile
└── .github/workflows/
```

### 17.2 目标平台

| 平台 | 说明 |
|---|---|
| Linux x86_64 | 主开发/服务器 |
| Linux aarch64 (musl) | NAS、ARM 小主机、路由器 |
| Linux armv7 (musl) | 老 ARM 设备 |
| Docker | `linux/amd64`、`linux/arm64`、`linux/arm/v7` |
| OpenWrt | 可选 ipk/apk；文档提供 procd 与 SDK 构建说明 |
| macOS / Windows | 开发/桌面试用（非首要目标） |

### 17.3 构建

- 前端：`pnpm build` → `ui/dist` → `include_dir`/`rust-embed` 嵌入（feature `embed-ui`）；
- 后端：`cargo build --release`；
- musl 交叉编译：
  - 纯 Rust 依赖：`cargo build --target aarch64-unknown-linux-musl`（可用 `rust-lld`）；
  - 含 bundled SQLite：使用 `cargo-zigbuild` 或 OpenWrt SDK 的 `aarch64-openwrt-linux-musl-gcc`；
- 发布：GitHub Releases 附带校验和、SBOM、changelog；Docker Hub/GHCR 多架构镜像。

### 17.4 版本与兼容

- SemVer；API `/api/v1` 稳定后只做向后兼容变更；
- DB 迁移只增不改；配置字段只增不删（废弃字段保留一个 minor 版本）；
- 配置 schema 版本号，启动时校验。

---

## 18. 测试与质量

| 层 | 内容 |
|---|---|
| 单元 | 路径校验、忽略规则、日期解析、哈希、缩略图命名 |
| 集成 | 临时目录 + 小样本图片/视频；扫描 → DB → API 全链路 |
| API | 契约测试；OpenAPI 校验；错误码 |
| 前端 | Vitest 组件测试；Playwright 端到端（浏览、灯箱、标签、扫描进度） |
| 安全 | 路径穿越、上传伪造、超大图、ffmpeg 超时、CSRF |
| 性能 | 基准：扫描、缩略图、列表查询；内存峰值 |
| 多架构 | QEMU 冒烟测试 aarch64/armv7 二进制 |
| 迁移 | 从上一版本 DB 升级；配置兼容 |
| 静态检查 | fmt、clippy `-D warnings`、cargo-deny、license 检查 |

测试样本只使用自制/CC0 图片与短视频，避免版权问题。

---

## 19. 可观测性

- `tracing` 结构化日志；请求 ID；慢查询/慢任务日志；
- `/healthz`（进程/DB/库可读性）；`/api/v1/info` 能力摘要；
- 任务历史与失败原因；
- 可选 `/metrics`（Prometheus）：请求数、延迟、任务队列长度、缩略图命中率、缓存大小；
- 可选 OpenTelemetry（feature），不默认启用；
- 默认**不收集遥测**，保护隐私。

---

## 20. 许可证与合规

- 推荐 **MIT OR Apache-2.0** 双许可，利于生态采用；
- ffmpeg：
  - 默认作为**外部命令**调用，不链接、不随二进制分发；
  - Docker 镜像若包含 ffmpeg，需在镜像与文档中标注其 LGPL/GPL 义务；
  - 提供 `-slim` 镜像（不含 ffmpeg）；
- 前端依赖 licenses 汇总；`cargo-deny` 检查；
- 不内置任何需要授权的人脸/识别模型；
- 隐私：照片与元数据不出本机，无云端依赖。

---

## 21. 路线图与验收

### v0.1 验收

- 在 512MB ARM 设备或 1GB 容器中索引 10k 文件；
- 时间线滚动 60fps 级别（桌面浏览器），移动端可用；
- 缩略图生成时 RSS < 200MB；
- 删除数据库后能重建索引；用户元数据可导出；
- 扫描/缩略图失败有明确日志，不崩溃；
- README + Docker + 配置示例 + 反向代理样例齐全。

### 里程碑

1. **M1**：索引 + API + 时间线（只读）
2. **M2**：缩略图 + 灯箱 + 视频海报
3. **M3**：标签/收藏/搜索
4. **M4**：写模式（上传/删除/回收站）
5. **M5**：可选转码 + PWA + 发布 v1.0

---

## 22. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 缩略图内存峰值 | OOM/服务重启 | 并发 1、像素上限、ffmpeg 兜底、内存限制 |
| 视频转码过慢 | CPU 占满 | 默认关闭、后台队列、超时、建议离线处理 |
| SQLite 交叉编译 | musl 构建失败 | CI 用 zigbuild；OpenWrt 用 SDK；文档给出命令 |
| 写操作数据丢失 | 用户照片损坏 | 默认只读、回收站、原子写、只读挂载、备份提示 |
| 范围膨胀 | 做成第二个 Immich | 明确非目标；按里程碑冻结范围 |
| 安全漏洞 | 数据泄露 | 安全设计、审计、依赖扫描、默认只暴露 LAN |
| 维护成本 | 项目停更 | 小核心、少依赖、自动化测试/发布、ADR |
| 前端性能 | 大库卡顿 | 虚拟滚动、分页、懒加载、srcset |

---

## 23. 开放问题

1. 默认 DB 是否坚持 SQLite？是否需要 `redb` 作为“无 C 依赖”构建？
2. v1 是否内置多用户？还是先单管理员 + 反向代理认证？
3. 上传是否直接走内置 API，还是优先推荐外部 WebDAV（配置项）？
4. 视频是否默认只生成海报、不转码？（当前设计：是）
5. 是否需要 inotify 实时扫描，还是手动/定时足够？
6. 分享链接（带过期/密码）是否进入 v1？
7. HEIC/RAW 的支持深度（代理图/原图下载/元数据）？
8. 插件化（缩略图引擎、存储后端）是否 v2 再考虑？

---

## 附录 A：配置字段总览

| 段 | 关键字段 |
|---|---|
| `server` | `mode`, `listen`, `base_path`, `ui_embedded`, `trusted_proxies` |
| `data` | `dir` |
| `libraries[]` | `id`, `name`, `path`, `read_only`, `ignore_globs`, `enabled` |
| `media` | `serve_mode`, `public_media_path`, `public_thumb_path` |
| `thumbnails` | `dir`, `sizes`, `format`, `quality`, `concurrency`, `max_pixels`, `prewarm_on_scan` |
| `ffmpeg` | `enabled`, `ffmpeg_path`, `ffprobe_path`, `concurrency`, `timeout_seconds` |
| `video` | `poster_enabled`, `transcode_enabled`, `max_height` |
| `auth` | `mode`, `session_ttl_hours` |
| `scan` | `on_start`, `interval`, `concurrency`, `follow_symlinks`, `missing_retention_days` |
| `trash` | `enabled`, `retention_days`, `dir` |
| `logging` | `level`, `format` |

## 附录 B：API 响应示例

```json
{
  "items": [
    {
      "id": "01J8Z6Y8Q4M2V7K3T9R1X5C0AB",
      "library_id": "family",
      "kind": "image",
      "filename": "IMG_0008.JPG",
      "taken_at": "2023-11-10T08:12:33Z",
      "width": 4032,
      "height": 3024,
      "duration_ms": null,
      "favorite": false,
      "tags": ["旅行", "2023"],
      "thumb_url": "/thumbs/ab/cd/<hash>-320.jpg",
      "preview_url": "/media/2023/IMG_0008.JPG",
      "media_url": "/media/2023/IMG_0008.JPG"
    }
  ],
  "next_cursor": "MjAyMy0xMS0xMFQwODoxMjozM1p8MDFKOFo2..."
}
```

## 附录 C：参考

- Photofield（只读浏览与索引思路）：<https://github.com/SmilyOrg/photofield>
- Immich 架构与资源要求（反面参照）：<https://immich.app/docs/install/requirements>
- PhotoPrism 资源要求（反面参照）：<https://docs.photoprism.app/getting-started/>
- Piwigo / Lychee（功能参照）：<https://piwigo.org/>、<https://lycheeorg.dev/>
- axum：<https://github.com/tokio-rs/axum>
- tower-http（静态文件/Range）：<https://github.com/tower-rs/tower-http>
- rusqlite：<https://github.com/rusqlite/rusqlite>
- utoipa（OpenAPI）：<https://github.com/juhaku/utoipa>
- PhotoSwipe：<https://photoswipe.com/>
- Vue 3：<https://vuejs.org/>
- cargo-zigbuild：<https://github.com/rust-cross/cargo-zigbuild>
- OpenWrt procd 文档：<https://openwrt.org/docs/guide-developer/procd-init-scripts>

---

## 附录 D：命名记录

项目最终定名为 photoSelf。以下候选仅作为命名调研记录：

- photoSelf
- PixVault
- Lumina Gallery
- FrameDeck
- OpenAlbum
- Galleryd

命名原则：短、易拼写、可搜索、不侵犯现有商标；仓库名建议与二进制名一致。
