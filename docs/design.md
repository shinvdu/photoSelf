# photoSelf — 自托管照片 / 视频画廊技术设计

> 实现状态：**v0.7.3（2026-11-27）**，已在 OpenWrt（CMCC RAX3000M / 23.05.2 / aarch64）实机部署
> 文档版本：v1.0（as-built，2026-11-28）
> 技术栈：Rust（axum + rusqlite）+ Vue 3（Vite）
> 项目名：photoSelf（早期工作代号 PhotoShelf，正式名已统一）
> 仓库：<https://github.com/shinvdu/photoSelf>

---

## 1. 摘要

photoSelf 是一个**轻量、自托管、配置驱动**的照片与视频画廊，直接索引已有目录，不搬动、不导入原始文件。文件系统是唯一内容来源，SQLite 只保存索引与用户元数据；删除数据库后可从文件重新扫描恢复。

后端为 Rust workspace，产出单个静态二进制；前端为 Vue 3 单页应用，可构建后嵌入二进制一起发布。默认只读；上传/删除/回收站、界面媒体库管理等写能力都通过配置显式开启，并受路径白名单与库级 `read_only` 双重约束。

当前版本已经落地：时间线浏览、灯箱缩放、标签/收藏、相册、上传/回收站、web MP4 转码队列、PWA、中英文界面、浅/深色主题、竖屏底部信息抽屉、右键菜单/多选批量操作、界面媒体库管理。

---

## 2. 目标与非目标

### 2.1 目标

1. **低资源**：512 MB–2 GB 的 ARM 盒子、NAS、迷你主机、路由器可运行；常驻内存小，缩略图并发可控。
2. **通用**：不绑定设备；路径、端口、认证、缓存、转码、并发均由配置决定，核心代码没有设备分支。
3. **零迁移**：直接索引已有目录，支持多个库、只读挂载。
4. **浏览优先**：时间线、懒加载、灯箱、视频播放、EXIF、缩放、移动端布局。
5. **安全写入**：默认只读；写模式显式开启；删除进回收站；上传原子写；路径不越界。
6. **可重建**：数据库可删；索引可重建；用户元数据保存在 SQLite 中，需单独备份。
7. **易部署**：单二进制 + 嵌入式前端；OpenWrt ipk、Docker、systemd、反向代理示例。
8. **可维护**：依赖少、无重型运行时；CI 覆盖格式、lint、测试、多架构发布。

### 2.2 非目标（当前版本仍不做）

- 人脸/物体识别、语义搜索、AI 相册；
- 多用户租户、复杂权限、公网 SaaS；
- 原生 iOS/Android App（先做 PWA）；
- 云盘同步、去重合并、照片编辑、视频剪辑；
- 插件市场、脚本系统；
- 与 Immich/PhotoPrism 数据模型兼容。

---

## 3. 版本演进与当前状态

| 版本 | 已落地能力 |
|---|---|
| 0.2 | 扫描、时间线、标签、收藏、相册、上传/删除/回收站、web MP4 队列、定时扫描、PWA |
| 0.3 | 中英文界面（默认中文）、语言记忆 |
| 0.4 | 灯箱缩放/平移、双击/双指、竖屏底部信息抽屉、滑动切图 |
| 0.5 | 界面媒体库管理（`[manage]` 白名单）、SQLite 库注册表、tombstone、目录选择器 |
| 0.6 | 粉紫浅/深色主题、竖屏按方向布局、移动顶栏网格、版本号 footer |
| 0.7 | 右键/长按菜单、多选批量操作、相册新建弹窗、相册内上传自动加入、删除二次确认、横屏灯箱铺满、自动加载修复 |

当前设计文档描述的是 **as-built** 架构，而不是最早的构想。早期草案中的 Pinia / Naive UI / PhotoSwipe / OpenAPI / SSE / session 登录仍未采用；**Vue Router 已在 2026-11-28 的工程重构中引入（hash 模式）**，同时完成了组件拆分与 Playwright 回归测试。原因与执行记录见第 12、17 节及 `docs/frontend-stack-evaluation.md`。

> 工程重构（未发布）：`App.vue` 从约 2,004 行降到约 1,200 行，抽出了 `components/` 与 `composables/`；新增 12 个 Playwright 交互用例；生产 JS 约 218 KB（gzip 77 KB）。

---

## 4. 总体架构

```text
┌──────────────────────────────────────────────────────────────┐
│                     Browser（Vue 3 SPA / PWA）                │
│  时间线 / 相册 / 回收站 / 媒体库 / 灯箱 / 批量操作 / 主题     │
└───────────────▲──────────────────────────────▲───────────────┘
                │ HTTP / Basic Auth            │ 静态资源、缩略图
                │                              │ 原图/视频（Range）
┌───────────────┴───────────────┐   ┌──────────┴───────────────┐
│ 反向代理（可选）               │   │ 静态文件层（可选）        │
│ Nginx / Caddy / lighttpd      │   │ /media → 照片目录         │
│ TLS / Basic Auth / OIDC       │   │ /thumbs → 缩略图缓存      │
│ /api → 127.0.0.1:8787         │   └──────────┬───────────────┘
└───────────────┬───────────────┘              │
                │                              │
┌───────────────▼──────────────────────────────▼───────────────┐
│                    photoself-server（Rust）                   │
│  axum API · 认证/CSRF · SPA/媒体服务 · 扫描任务 · 媒体库管理   │
└───────────────┬──────────────────────────────▲───────────────┘
                │                              │
┌───────────────▼──────────────────────────────┴───────────────┐
│                    photoself-core（Rust）                     │
│  Config · SQLite · Scanner · Metadata · Thumbnail · Video     │
└───────────────┬──────────────────────────────────────────────┘
                │
        ┌───────▼────────┐       ┌─────────────────────────────┐
        │  媒体库根目录   │       │ 数据目录（SQLite/缓存/回收站）│
        │ （可只读挂载）  │       │ sidecar / 转码产物           │
        └────────────────┘       └─────────────────────────────┘
```

### 4.1 请求路径

- 浏览：SPA → `/api/v1/items` → SQLite → JSON（游标分页）。
- 缩略图：`<img src="/api/v1/items/{id}/thumb/{size}">`（standalone）或反向代理直接读缓存目录（proxy 模式）。
- 原图/视频：standalone 由 Rust 流式返回并支持 Range；proxy 模式由静态层提供。
- 扫描：UI/API/定时器 → 后台 job → 遍历目录 → 元数据/哈希 → DB。
- 转码：任务队列 → ffmpeg → `data/cache/derived/` → `play_url`。

### 4.2 关键运行期对象

`AppState` 持有：

- `Arc<Config>`：启动时加载的不可变配置；
- `Db`：SQLite 连接（`Arc<Mutex<Connection>>`，WAL）；
- `libraries: Arc<RwLock<Vec<Library>>>`：运行期媒体库快照，启动时从 DB 载入；
- `manage_roots`：`[manage].allowed_roots` 的 canonicalize 结果；
- 缩略图/转码信号量、`scanning` 原子锁、认证缓存、可信代理列表。

扫描与 API 都读取 `libraries` 快照；界面增删库后立即 reload，无需重启。

---

## 5. 仓库结构（实际）

```text
photoSelf/
├── Cargo.toml                      # workspace（core + server）
├── crates/
│   ├── photoself-core/
│   │   ├── config.rs               # 配置结构/校验/路径
│   │   ├── db.rs                   # schema、迁移、查询、库/相册/标签/回收站
│   │   ├── scan.rs                 # 目录扫描、单文件索引、哈希/重命名识别
│   │   ├── thumb.rs                # 图片缩略图、视频海报、sidecar、方向处理
│   │   ├── video.rs                # ffmpeg/ffprobe、转码辅助
│   │   ├── model.rs                # Library/StoredLibrary/MediaItem/Album/...
│   │   └── util.rs                 # 时间/游标/路径工具
│   └── photoself-server/
│       ├── main.rs                 # CLI：serve / scan / hash-password
│       ├── api.rs                  # 所有 /api/v1 路由与处理
│       ├── state.rs                # AppState、库快照、信号量
│       ├── auth.rs                 # none / basic / proxy
│       └── ui.rs                   # SPA/静态资源服务（standalone + embed-ui）
├── ui/
│   ├── src/App.vue                 # 应用壳：顶栏、状态栏、时间线、全局灯箱
│   ├── src/components/             # Header/Albums/Trash/LibraryManager/菜单/弹窗
│   ├── src/composables/            # useTheme / useLightbox / useSelection
│   ├── src/router.js               # Vue Router（hash）
│   ├── src/styles.css              # 主题变量与组件样式
│   ├── src/i18n.js + locales/      # 中英文
│   ├── tests/e2e/                  # Playwright（mock API）
│   ├── index.html                  # 主题预加载脚本
│   └── vite.config.js              # Vite + PWA + /api 代理
├── deploy/
│   ├── openwrt/                    # procd init、示例配置、打包说明
│   ├── systemd/
│   └── docker/
├── scripts/
│   ├── dev.sh                      # 后端 + Vite 一键开发
│   ├── make-ipk.sh                 # OpenWrt ipk 打包
│   ├── make-sidecars.py            # 预生成 HEIC/HEVC 预览
│   └── build-release.sh
├── docs/
│   ├── design.md                   # 本文
│   ├── usage.zh-CN.md / usage.md
│   └── local-development.md
├── config.example.toml
├── Dockerfile
└── .github/workflows/              # ci.yml、release.yml
```

与早期草案的差异：

- 没有拆分 `photoself-db` / `photoself-media` / `photoself-api-types`，避免过度分层；DB/媒体逻辑在 core，HTTP 在 server。
- 没有引入 OpenAPI/utoipa、SSE、metrics feature；当前规模下直接 JSON + 轮询 job 足够。
- 前端已引入 Vue Router 4（hash）并拆分组件；仍未引入 TypeScript/Pinia/Naive UI/PhotoSwipe，保持少量依赖与离线优先。

---

## 6. 部署模式

| 模式 | 说明 | 适用 |
|---|---|---|
| **Standalone** | Rust 同时提供 SPA、API、缩略图、原图/视频（Range） | 单机、Docker、个人 NAS、OpenWrt |
| **Reverse Proxy** | Rust 只提供 API；TLS/Auth/静态媒体由 Nginx/Caddy/lighttpd 提供 | 已有 Web 基础设施、低资源设备 |

`server.mode` 决定 SPA/媒体行为，`media.serve_mode` 决定媒体 URL 形态。前端只消费 API 返回的 URL，不关心模式。

OpenWrt 实机采用 standalone + Basic Auth + procd：

```text
/usr/bin/photoself              # 静态 aarch64 musl 二进制
/etc/init.d/photoself           # procd，START=99（等挂载）
/etc/photoself/config.toml      # 600
/mnt/sda1/photo-app/data        # SQLite + 缓存 + trash
/mnt/sda1/photo-app/sidecars    # 预生成预览
/mnt/sda1/photo-app/uploads     # 可写上传库
```

---

## 7. 配置与运行期语义

### 7.1 配置优先级与读取

- 格式 TOML；`photoself serve -c <path>` 读取。
- 配置在启动时加载并校验；**修改配置必须重启**。
- 媒体库的界面增删改写数据库并立即生效，不需要重启。

### 7.2 媒体库：config 只做种子，SQLite 为准

- 启动时 `seed_config_libraries()` 把 `[[libraries]]` 导入 `libraries` 表：
  - 新 id：插入，`source='config'`；
  - 旧版行（`source IS NULL`）：从 config 升级一次；
  - 已有 `source='config'` 或 `source='ui'` 行：不覆盖。
- 界面新增库：写入 DB，`source='ui'`，无需 config。
- 界面移除库：不物理删除行，而是 `deleted=1`（tombstone）+ 删除 items/trash 索引；重启不会被 config 复活。
- 「从配置重新导入」：显式强制覆盖同 id 配置行，并清除 tombstone；界面新增库不受影响。
- 删除数据库会丢失界面新增库；config 里的库会在下次启动重新导入。

### 7.3 运行期开关

| 能力 | 条件 |
|---|---|
| 浏览/搜索/标签/相册 | 默认可用（只读库也可） |
| 上传 | `[write].enabled=true` + 目标库 `read_only=false` |
| 删除/回收站 | `[write]` + `[trash]` + 可写库 |
| 界面媒体库管理 | `[manage].enabled=true` + 认证通过；`auth.mode=none` 时拒绝（除非 `require_auth=false`） |
| 添加目录 | 路径在 `[manage].allowed_roots` 内，且不与数据/缩略图/sidecar/回收站/已有库重叠 |

### 7.4 认证

- `none`：仅可信局域网；
- `basic`：单管理员，Argon2id 哈希（`photoself hash-password`）；
- `proxy`：信任 `X-Forwarded-User`，仅接受 `server.trusted_proxies` 网段。

没有 session/多用户/登录页；Basic Auth 由浏览器或反向代理完成。

---

## 8. 数据模型与存储

### 8.1 SQLite

- WAL、`busy_timeout=5000`、`foreign_keys=ON`、`synchronous=NORMAL`；
- 单文件 `data/photoself.db`；`-wal`/`-shm` 同目录；
- 迁移：`CREATE TABLE IF NOT EXISTS` + `ensure_library_columns()` 等 `ALTER TABLE ADD COLUMN`；只增列，兼容旧库。

### 8.2 主要表

| 表 | 关键字段 | 说明 |
|---|---|---|
| `libraries` | `id,name,root_path,read_only,enabled,ignore_globs,source,deleted,created_at,updated_at` | 运行期注册表 |
| `items` | `id,library_id,rel_path,content_hash,kind,size,mtime,width,height,duration_ms,taken_at,camera,orientation,favorite,missing,...` | `UNIQUE(library_id,rel_path)`，外键级联 |
| `tags` | `id,name,color` | 名称唯一 |
| `item_tags` | `item_id,tag_id` | 多对多 |
| `albums` | `id,name,description,cover_item_id,created_at,updated_at` | 条目数实时统计 |
| `album_items` | `album_id,item_id,position` | 虚拟关系 |
| `trash` | `id,item_id,library_id,rel_path,trash_path,deleted_at` | 无外键；文件在 `trash/<lib>` |
| `derivatives` | `item_id,kind,path,created_at` | 转码产物（如 `web` MP4） |
| `jobs` | `id,kind,state,progress,total,message,error,...` | 扫描/转码任务 |

没有 users/sessions/settings 表；认证状态在配置层。

### 8.3 缺失项与相册计数

- 删除到回收站后条目标记 `missing=1`，不立即删 DB 行；文件移到 `data/trash/<library-id>/`。
- 列表/相册内容查询过滤 `missing=0`。
- 相册 `item_count` 与封面选择也过滤 `missing`，避免“列表 8 张、数量 9”的不一致。
- 永久删除（purge）删除 trash 文件与 trash 行；`items` 行在后续清理/重建中处理（当前版本保留 missing 行）。

### 8.4 数据目录

```text
<data.dir>/
├── photoself.db / -wal / -shm
├── cache/
│   ├── thumbs/          # <key>/<key>-<size>.<format>，内容寻址
│   ├── tmp/             # 原子写临时文件
│   └── derived/         # web MP4
└── trash/<library-id>/  # 回收站文件
```

---

## 9. 媒体库生命周期

```text
config [[libraries]] ──seed──▶ libraries(DB)
                                   ▲
界面新增/启停/改名 ────────────────┘
界面移除 ──▶ deleted=1（tombstone）+ 清 items/trash 索引（不删源文件）
```

### 9.1 路径校验（添加库）

- 必须绝对路径；`canonicalize` 后存在且是目录；
- 必须位于某个 `manage.allowed_roots` 内（根目录启动时 canonicalize；不存在的条目会被忽略）；
- 拒绝与 `data.dir`、缩略图目录、sidecar 目录、回收站目录重叠；
- 拒绝与已有库路径相同或嵌套（避免重复索引）；
- 检查可读性；文件系统操作放在 blocking 池并加 5 秒超时，防止坏挂载卡死 API；
- 符号链接先解析，解析后仍必须落在白名单内。

### 9.2 添加后的行为

- 生成 `lib-<uuid>` id；名称缺省用目录名；默认 `read_only=true`；
- 写入 DB（`source='ui'`）、reload 内存快照；
- 尽力自动启动该库扫描；若已有扫描在跑则添加成功但扫描需稍后手动触发。

### 9.3 删除语义

- 删除前需无扫描在跑；
- 删除 `items`、`trash` 索引行（trash 文件保留在磁盘，便于人工恢复）；
- 库行保留为 tombstone；
- 源文件、sidecar 不删除。

---

## 10. 扫描、缩略图与视频

### 10.1 扫描

- `walkdir` 遍历库根，应用 `ignore_globs`（默认 `._*`、`.DS_Store`、`@eaDir`、`#recycle`）；
- 默认不跟随符号链接；
- 增量：`size + mtime` 未变则不重新解析；
- 内容哈希 BLAKE3；新路径命中“缺失项”的 hash 时视为重命名，复用 item id（保留标签/相册）；
- 扫描是顺序执行；`scan.concurrency` 目前是预留项，未实际并行；
- 任务进度写 `jobs`，前端轮询 `/jobs/{id}`；
- 全量扫描与单库扫描共用扫描锁，同一时间只允许一个；
- 缺失文件标记 `missing`，不自动删除记录。

### 10.2 元数据

- 图片：`kamadak-exif`（日期、相机、方向等）；
- 视频：`ffprobe`（宽高/时长/创建时间），可用 `scan.probe_videos` 关闭；
- 日期回退：EXIF → 文件名 → mtime；
- HEIC/HEIF/AVIF 按扩展名索引；能否预览取决于解码器或 sidecar。

### 10.3 缩略图

- 内容寻址：`thumb_dir/<key[0:2]>/<key[2:4]>/<key>-<size>.<format>`；
- 临时文件唯一（uuid + pid），`rename` 原子落盘；
- 图片解码前检查 `max_pixels`；超大图可用 ffmpeg 兜底；
- 视频海报：`ffmpeg -ss 1 -frames:v 1 -vf scale=<size>:-2`，失败回退第 0 秒；
- 并发由 `thumbnails.concurrency` 限制；低资源设备建议 1；
- sidecar：优先读取预生成的 `<sidecar_dir>/<library-id>/<rel>.<size>.jpg`，适合 OpenWrt 缺少 HEVC/HEIF 解码的场景。

### 10.4 视频

- 默认只生成海报，不转码；
- 开启 `video.transcode_enabled` 后，灯箱可请求生成 web MP4（H.264/AAC）；
- 任务队列单并发，产物在 `cache/derived`，通过 `play_url` 访问；
- OpenWrt 自带 ffmpeg 常常没有软件 HEVC 解码器（`hevc_v4l2m2m` 无设备），HEVC 海报会 500；需要 sidecar 或关闭 `video.poster_enabled`。

---

## 11. API

### 11.1 约定

- Base `/api/v1`；JSON；错误 `application/problem+json`（`detail` 字段）。
- 所有 `POST/PUT/PATCH/DELETE` 必须带 `X-Requested-With: photoSelf`（CSRF 防护）。
- 认证由 `auth.mode` 决定；管理端点额外检查 `[manage]` 与认证要求。
- 列表分页使用游标：返回 `next_cursor`，下次作为 `cursor`；`limit` 默认/最大由实现控制。

### 11.2 实际端点

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/healthz` | 存活 |
| GET | `/api/v1/info` | 版本、模式、条目数、能力开关、库摘要 |
| GET | `/api/v1/items` | 时间线；`library/kind/q/favorite/tag/album/from/to/cursor/limit` |
| GET/PATCH/DELETE | `/api/v1/items/{id}` | 详情 / 收藏 / 删除到回收站 |
| GET | `/api/v1/items/{id}/thumb/{size}` | 缩略图/海报（standalone） |
| GET | `/api/v1/items/{id}/media` | 原文件（Range） |
| GET | `/api/v1/items/{id}/derived` | web MP4 |
| POST | `/api/v1/items/{id}/transcode` | 请求转码 |
| GET/POST | `/api/v1/tags` | 标签列表/创建 |
| DELETE | `/api/v1/tags/{id}` | 删除标签 |
| POST/DELETE | `/api/v1/items/{id}/tags[/{tag_id}]` | 挂载/移除标签 |
| GET/POST | `/api/v1/albums` | 相册列表/创建 |
| GET/DELETE | `/api/v1/albums/{id}` | 相册详情/删除 |
| GET/POST | `/api/v1/albums/{id}/items` | 相册条目/加入 |
| DELETE | `/api/v1/albums/{id}/items/{item_id}` | 从相册移除 |
| POST | `/api/v1/upload` | multipart 上传（写模式） |
| GET | `/api/v1/trash` | 回收站列表 |
| POST/DELETE | `/api/v1/trash/{id}[/restore]` | 恢复/永久删除 |
| POST | `/api/v1/scan` | 全量扫描 |
| GET | `/api/v1/jobs` / `/jobs/{id}` | 任务 |
| GET/POST | `/api/v1/libraries` | 媒体库列表/新增（管理） |
| PATCH/DELETE | `/api/v1/libraries/{id}` | 改名/启停/移除（管理） |
| POST | `/api/v1/libraries/{id}/scan` | 单库扫描（管理） |
| POST | `/api/v1/libraries/import-config` | 从 config 强制导入（管理） |
| GET | `/api/v1/fs/dirs?path=` | 白名单内目录浏览（管理） |

未实现：SSE `/events`、`/metrics`、OpenAPI、session 登录、批量 API（批量删除/加入相册由前端逐条调用）。

### 11.3 前端使用的关键字段

- `info.write_enabled / trash_enabled / manage_enabled / libraries[]`
- 条目：`thumb_url / preview_url / media_url / play_url / favorite / tags`
- 库：`id/name/path/read_only/enabled/source/item_count/ignore_globs`

---

## 12. 前端设计

### 12.1 技术栈与结构

- Vue 3（JavaScript）+ Vite + `vue-i18n` + `vue-router`（hash history）；`App.vue` 为应用壳，视图/弹窗/菜单拆到 `components/`，灯箱与选择逻辑在 `composables/`。
- 没有 TypeScript/Pinia/Naive UI/PhotoSwipe；状态仍以 `ref/computed` 为主，Pinia 仅在组件拆分后仍觉得状态边界混乱时再评估。
- 路由：`#/timeline?q=&tag=&favorite=`、`#/albums`、`#/albums/:id`、`#/trash`、`#/libraries`；支持刷新恢复、浏览器前进/后退与深链接。
- PWA：`vite-plugin-pwa` 生成 manifest 与 Service Worker；主题色随浅/深色调整。

### 12.2 布局与交互

- **顶部栏**：品牌、页签、搜索、标签筛选、收藏、选择、上传、扫描、主题、语言。移动/竖屏使用 grid 重排。
- **路由**：视图与搜索/标签/收藏状态写入 hash URL；直接访问、刷新、后退均可恢复；切换路由会关闭灯箱并清空选择。
- **时间线**：按天分组、懒加载缩略图；滚动位置检查自动加载下一页（替代了会因视图切换失效的 IntersectionObserver），保留「加载更多」按钮。
- **灯箱**：
  - 缩放（滚轮/双指/双击/按钮，1×–5×）、拖拽平移、键盘 `+/-/0`；
  - 横向/桌面：图片铺满窗口（5 px 边距），信息面板浮动在右侧，ⓘ 可收起，翻页箭头自动避让；
  - 竖屏/窄屏：底部信息/操作抽屉，默认收起，点文件名栏展开；左右滑动切图。
- **相册**：按钮 + 弹窗新建；卡片删除按钮悬停显示（触屏常显）；删除需二次确认；相册内上传自动加入当前相册；相册只存关系。
- **右键/长按菜单**：查看、下载、加入相册、删除；多选支持 Ctrl/Cmd、Shift 范围，底部批量栏；批量删除只允许全部为可写库条目。
- **媒体库**：列表、添加（目录选择器/手输）、扫描、启停、移除、从配置重新导入。
- **主题/语言**：浅/深色 `data-theme` + CSS 变量；中英文文案；选择均保存在 `localStorage`。
- **版本号**：页面底部 footer。

### 12.3 性能策略

- 首屏请求 100 条 + 320 缩略图；详情按需 1280 预览/原图；
- `loading="lazy"`、`decoding` 交给浏览器；
- 批量操作逐条请求，避免一次长请求；显示部分失败；
- PWA 缓存应用壳，媒体不缓存（避免占满）。

---

## 13. 安全设计

| 领域 | 当前措施 |
|---|---|
| 认证 | none / basic（Argon2id）/ proxy（来源 CIDR 校验） |
| CSRF | 所有写方法要求 `X-Requested-With: photoSelf` |
| 管理 | `[manage].enabled` + 认证；`allowed_roots` 白名单；`require_auth` |
| 路径 | 绝对路径 + canonicalize；库根前缀校验；数据/缓存/sidecar/trash 保护；拒绝重复/嵌套 |
| 上传 | 大小限制、临时文件 + 原子 rename、文件名清洗、父目录越界校验 |
| 删除 | 只对可写库；进回收站；支持恢复/永久删除；绝不删除源库其他文件 |
| 外部进程 | `Command` 直接传参（不走 shell）、超时、并发限制、固定输出路径 |
| 资源 | 缩略图并发/像素上限、上传上限、扫描单任务锁、目录校验超时 |
| 部署 | 建议只读挂载照片库；仅局域网或 WireGuard；Basic Auth 密码文件 root-only |
| 依赖 | Rust 依赖保持精简；CI fmt/clippy/test；发布产物带校验和 |

已知边界：

- 没有多用户/角色；Basic Auth 是单管理员模型。
- 反向代理模式必须正确配置认证与 `trusted_proxies`，否则可能越权。
- PWA/Service Worker 可能缓存旧前端，升级后需刷新。

---

## 14. 构建、测试与发布

### 14.1 构建

```bash
# UI
cd ui && npm install && npm run build

# 后端（开发）
cargo build -p photoself-server

# 单文件发布（嵌入 UI）
cargo build -p photoself-server --release --features embed-ui
```

交叉编译发布使用 `cargo-zigbuild`：

- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`
- `armv7-unknown-linux-musleabihf`

OpenWrt ipk 由 `scripts/make-ipk.sh` 打包；procd 脚本 `deploy/openwrt/photoself.init`，`START=99` 等待挂载，崩溃自动重启。

### 14.2 CI

`.github/workflows/ci.yml`：

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cd ui && npm ci && npm run build`

`.github/workflows/release.yml`：push tag `v*` 后构建 3 个架构的二进制/IPK，生成 SHA256SUMS，发布到 GitHub Releases。

### 14.3 发布流程

1. 升版本（Cargo workspace、Cargo.lock、ui package/lock、CHANGELOG）
2. 本地四项检查
3. 提交并推送 main
4. 打 tag `vX.Y.Z` 并推送
5. 等 Actions Release 成功
6. 下载 aarch64 ipk → scp 到 OpenWrt → `opkg install` → 重启
7. 验证 version、`/api/v1/info`、缩略图、配置保留

升级只加 DB 列，ipk 不覆盖 `/etc/photoself/config.toml`；升级前备份配置与 data。

---

## 15. 性能与容量

实际部署观察（CMCC RAX3000M，aarch64 Cortex-A53，OpenWrt 23.05.2）：

- 媒体条目约 4,300；数据目录约 114 MB；sidecar 约 560 MB（历史生成）。
- 缩略图并发 1；ffmpeg 并发 1；转码关闭。
- 首次浏览按需生成缩略图，之后走缓存。
- 增量扫描依赖 size+mtime，不重复读全库。
- 低内存建议：`sizes=[320]`（或 `[320,1280]` 但并发 1）、关闭转码、关闭定时扫描、缓存与 SQLite 放外接盘。

目标（设计层面）：

| 指标 | 目标 |
|---|---|
| 最低内存 | 512 MB 设备可运行 |
| 常驻 RSS | 典型 < 150 MB（不含 ffmpeg） |
| 索引 10k 文件（无变化） | < 30 秒（ARM A53 量级） |
| API p95 | 局域网列表/详情 < 150 ms |
| 规模 | 1 万–10 万项；百万项不承诺 |

---

## 16. 已知限制与后续路线

### 16.1 当前限制

- 无多用户、分享链接、租户隔离；认证是单管理员/代理模式。
- 搜索是文件名 `LIKE` + 标签过滤，不是全文检索。
- 无重命名/移动 UI；文件组织仍由外部工具完成。
- 时间线不按库过滤（API 支持 `library`，UI 未做选择器）。
- 回收站不自动过期；`retention_days` 未自动清扫。
- 上传不提供子目录选择；批量删除/加入相册是前端逐条请求。
- OpenWrt 自带 ffmpeg 可能缺少 HEVC 解码，视频海报会失败；需要 sidecar 或关闭海报。
- PWA 缓存可能导致升级后仍显示旧 UI。

### 16.2 后续候选

- 多用户与分享链接（带过期/密码）；
- SQLite FTS5 全文搜索；
- 重命名/移动 UI、缩略图预热；
- 回收站自动清理与容量策略；
- 按库筛选时间线、上传子目录；
- 系统化 sidecar 生成与孤儿缓存清理；
- 可选 OpenAPI/SSE/metrics（在确有集成需求时再加）。

---

## 17. 设计决策记录（摘要）

1. **SQLite 作为运行期权威，config 只做种子**：避免界面修改与配置文件互相覆盖；用 tombstone 防止删除的库被重启复活；「从配置重新导入」是显式逃生口。
2. **不采用 session/多用户登录**：目标场景是家庭/小团队、已有反向代理认证；Basic Auth/代理模式足够，减少攻击面与实现量。
3. **不引入 OpenAPI/SSE**：当前 API 规模小，轮询 job 足够；避免生成器与契约维护成本。
4. **拆分组件 + Router（hash），暂不引入 Pinia/UI 库**：`App.vue` 已从 2,004 行降到约 1,200 行；Router 提供深链接/后退/刷新恢复；Pinia 与 UI 库仍按需，避免无收益的依赖与主题对抗。
5. **缩略图按需生成 + 信号量限流**：低资源设备优先保证可用性；sidecar 作为 HEIC/HEVC 的兜底。
6. **删除=回收站、移除库=只删索引**：把“绝不破坏源文件”作为产品红线。
7. **管理功能 opt-in + 白名单**：默认关闭；路径必须显式授权，且与内部数据目录隔离。

---

## 18. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 缩略图内存峰值 | OOM/重启 | 并发 1、像素上限、ffmpeg 兜底 |
| 视频转码/海报过慢 | CPU 占满 | 默认关闭转码；海报失败用 sidecar；并发 1 |
| SQLite 交叉编译 | musl 构建失败 | CI 使用 cargo-zigbuild；OpenWrt 用 SDK |
| 写操作数据丢失 | 照片损坏 | 默认只读、库级 read_only、回收站、原子写、备份提示 |
| 误删库/照片 | 用户数据损失 | 删除二次确认、移除库只删索引、tombstone、只读库默认 |
| 管理接口暴露 | 路径越权 | 默认关闭、认证要求、allowed_roots、目录/重叠校验、超时 |
| HEVC 解码缺失 | 海报 500/无封面 | sidecar 预生成、关闭 poster、PC 端转码 |
| 范围膨胀 | 变成重型相册 | 明确非目标；按版本迭代；保持单二进制与低依赖 |
| PWA 缓存 | 升级后旧 UI | 强刷/清站点数据；CI 产物哈希校验 |

---

## 附录 A：配置字段（实际使用）

```toml
[server]
mode = "standalone"          # standalone | reverse-proxy
listen = "0.0.0.0:8787"
base_path = "/"
ui_embedded = true
trusted_proxies = ["127.0.0.1/32"]

[data]
dir = "/mnt/sda1/photo-app/data"

[[libraries]]
id = "mobile"
name = "Mobile"
path = "/mnt/sda1/syncthings/..."
read_only = true
enabled = true
ignore_globs = ["._*", ".DS_Store", "@eaDir", "#recycle"]

[manage]
enabled = true
allowed_roots = ["/mnt/sda1", "/mnt/drive", "/mnt/sdd3"]
require_auth = true

[media]
serve_mode = "standalone"    # standalone | proxy
public_media_path = "/media"
public_thumb_path = "/thumbs"

[thumbnails]
sidecar_dir = "/mnt/sda1/photo-app/sidecars"
sizes = [320, 1280]
format = "jpeg"
quality = 82
concurrency = 1
max_pixels = 100000000

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
transcode_concurrency = 1

[auth]
mode = "basic"
username = "admin"
password_hash = "$argon2id$..."
session_ttl_hours = 720      # 当前保留字段，未用于会话

[scan]
on_start = false
interval = "0"
concurrency = 1              # 预留；扫描当前顺序执行
follow_symlinks = false
missing_retention_days = 30  # 预留
hash = true
probe_videos = true

[trash]
enabled = true
retention_days = 30          # 预留；当前不自动清扫
# dir = "..."

[write]
enabled = true
max_upload_bytes = 536870912

[logging]
level = "info"
format = "text"
```

`[[libraries]]` 与 `[manage]` 的语义见第 7 节；界面新增库不在 config 中。

---

## 附录 B：API 响应示例

`GET /api/v1/info`：

```json
{
  "name": "photoSelf",
  "version": "0.7.3",
  "mode": "standalone",
  "auth_mode": "basic",
  "item_count": 4295,
  "thumb_sizes": [320, 1280],
  "write_enabled": true,
  "trash_enabled": true,
  "transcode_enabled": false,
  "manage_enabled": true,
  "libraries": [
    {"id": "family", "name": "Family photos", "read_only": true},
    {"id": "mobile", "name": "Mobile (Syncthing)", "read_only": true}
  ]
}
```

`GET /api/v1/items?limit=2`：

```json
{
  "items": [
    {
      "id": "01a218d2-4499-7341-9ac1-bdee7f60f4aa",
      "library_id": "mobile",
      "kind": "image",
      "filename": "IMG_5248.PNG",
      "size": 180224,
      "width": 1125,
      "height": 2436,
      "taken_at": "2026-01-01T00:19:57",
      "favorite": false,
      "tags": [],
      "thumb_url": "/api/v1/items/01a218d2-4499-7341-9ac1-bdee7f60f4aa/thumb/320",
      "preview_url": "/api/v1/items/01a218d2-4499-7341-9ac1-bdee7f60f4aa/thumb/1280",
      "media_url": "/api/v1/items/01a218d2-4499-7341-9ac1-bdee7f60f4aa/media",
      "play_url": null
    }
  ],
  "next_cursor": "..."
}
```

---

## 附录 C：参考

- 项目仓库：<https://github.com/shinvdu/photoSelf>
- 使用说明：`docs/usage.zh-CN.md`
- 前端技术栈取舍评估：`docs/frontend-stack-evaluation.md`
- 本地开发环境：`docs/local-development.md`
- axum：<https://github.com/tokio-rs/axum>
- tower-http：<https://github.com/tower-rs/tower-http>
- rusqlite：<https://github.com/rusqlite/rusqlite>
- Vue 3：<https://vuejs.org/>
- cargo-zigbuild：<https://github.com/rust-cross/cargo-zigbuild>
- OpenWrt procd：<https://openwrt.org/docs/guide-developer/procd-init-scripts>
- 早期调研：`OpenWrt相册管理工具-调研与选型.md`（外部调研稿）

---

## 附录 D：命名记录

早期工作代号 **PhotoShelf**，正式项目名统一为 **photoSelf**，仓库、二进制、ipk 包名、文档均已使用 photoSelf。历史设计稿中的 PhotoShelf 命名不再出现在当前代码与发布产物中。