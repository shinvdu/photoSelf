# photoSelf 本地开发环境

本文描述如何在一台 Linux（WSL2）开发机上，用仓库源码跑起 **后端 + 前端**，并准备一套可随时丢弃、可重建的示例照片库。所有命令均为 127.0.0.1 回环地址，不部署、不接触路由器、不修改 `/etc/photoself`。

- 仓库路径：`~/tasks/photoSelf`（下文命令可直接复制执行）
- 版本：v0.6.0
- 示例数据目录：`/tmp/photoself-dev`（仓库外，重启后可能被清空，重建方法见 [清空并重建](#清空并重建)）
- 开发配置：仓库根目录 `config.dev.toml`（已加入 `.gitignore`，不会被提交）

> 只用于本机开发。`auth.mode = "none"` 且 `manage.require_auth = false` 绝不能暴露到局域网或公网。

## 前置条件与实测版本

系统：Ubuntu 24.04.4 LTS（WSL2，内核 7.0.0-43-generic），12 核 / 19 GiB 内存。

| 工具 | 实测版本 | 用途 |
|---|---|---|
| rustc | 1.97.1 (8bab26f4f 2026-07-14) | 编译 Rust workspace |
| cargo | 1.97.1 (c980f4866 2026-06-30) | 构建、测试、运行 |
| node | v24.19.0 | Vite / Vue 开发服务器 |
| npm | 11.17.0 | 安装前端依赖 |
| vite | 6.4.6（`ui/package.json` 中为 `^6.0.0`） | 前端开发服务器 |
| vue | 3.5.46 | UI 框架 |
| ffmpeg / ffprobe | 6.1.1-3ubuntu5 | 视频封面、HEIC 兜底解码 |
| curl | 8.5.0 | 验证 API |
| python3 | 3.12.3 | 可选：`scripts/make-sidecars.py` |
| Pillow | 10.2.0 | 可选：sidecar 脚本依赖 |
| heif-convert | 已安装（libheif） | 可选：生成 HEIC sidecar |
| sqlite3 CLI | **未安装** | 不需要；用 API 查数据 |

检查命令：

```bash
rustc --version && cargo --version
node --version && npm --version
ffmpeg -version | head -1
ffprobe -version | head -1
python3 --version
```

如果 `ffmpeg`/`ffprobe` 不存在：图片浏览和 JPG 缩略图仍可用（Rust `image`），但视频封面/探测不可用。此时把 `config.dev.toml` 中 `[ffmpeg] enabled = false`、`[scan] probe_videos = false`，并在文档下方“常见问题”中查看限制。

## 一次性初始化

### 1. 目录与示例照片

```bash
mkdir -p /tmp/photoself-dev/data /tmp/photoself-dev/photos /tmp/photoself-dev/sidecars
# 从 ~/Pictures 取前 3 张 JPG（不要复制整个目录）
i=0
for f in "$HOME"/Pictures/*.jpg; do
  cp "$f" /tmp/photoself-dev/photos/
  i=$((i + 1))
  [ "$i" -ge 3 ] && break
done
ls -la /tmp/photoself-dev/photos
```

说明：

- 只复制了 3 张 JPG，`~/Pictures` 原文件不动。
- 本机 `~/Pictures` 下没有视频，因此示例库跳过视频。需要测试视频时，复制任意 MP4 到 `/tmp/photoself-dev/photos/` 后重新扫描即可。
- `data/` 放 SQLite、缩略图缓存、临时文件、回收站；`photos/` 是示例库根目录；`sidecars/` 是预生成预览根目录（当前为空）。
- 示例照片约 15 MB，位于 `/tmp`，机器重启后可能消失，重新执行本节即可。

### 2. 开发配置 `config.dev.toml`

已在仓库根目录创建，关键配置：

| 配置 | 值 | 说明 |
|---|---|---|
| `server.mode` | `standalone` | Rust 同时提供 API 和媒体 |
| `server.listen` | `127.0.0.1:8787` | 仅回环 |
| `server.ui_dir` | `ui/dist` | 无 `embed-ui` 时从磁盘读前端 |
| `server.ui_embedded` | `false` | 字段保留；嵌入是编译期特性 |
| `data.dir` | `/tmp/photoself-dev/data` | 索引、缓存、回收站 |
| `[[libraries]] id/path` | `dev` / `/tmp/photoself-dev/photos` | 示例库，只读 |
| `auth.mode` | `none` | 无登录 |
| `scan.on_start` | `false` | 启动不自动扫描，便于观察 |
| `scan.hash` / `probe_videos` | `true` / `true` | 有 ffmpeg，开启 |
| `manage.enabled` | `true` | 可从 UI/API 管理媒体库 |
| `manage.require_auth` | `false` | 本机无认证时仍允许管理（仅回环） |
| `manage.allowed_roots` | `["/tmp/photoself-dev"]` | 示例库父目录，可添加同级测试库 |
| `thumbnails.concurrency` | `1` | 低资源、可复现 |
| `thumbnails.sidecar_dir` | `/tmp/photoself-dev/sidecars` | sidecar 根目录 |
| `ffmpeg.enabled` | `true` | 实测 6.1.1 可用 |
| `write.enabled` | `false` | 上传/删除保持关闭（需要时自行打开） |

该文件已被 `.gitignore` 中的 `/config.dev.toml` 忽略。若文件丢失，按文末 [附录](#附录重建-configdevtoml) 重建。

验证配置能解析并建立索引（在仓库根执行）：

```bash
cd ~/tasks/photoSelf
cargo run -p photoself-server -- scan -c config.dev.toml
```

预期输出：

```text
dev: scanned 3, errors 0, missing 0
```

## 日常启动 / 停止

### 方式 A：两个终端（推荐，日志清楚）

终端 1 —— 后端：

```bash
cd ~/tasks/photoSelf
cargo run -p photoself-server -- serve -c config.dev.toml
```

看到 `photoSelf starting listen=127.0.0.1:8787 ...` 即成功。`Ctrl-C` 停止。

终端 2 —— 前端开发服务器：

```bash
cd ~/tasks/photoSelf/ui
npm run dev
```

首次运行若没有 `ui/node_modules`，脚本会报错；先执行 `npm install`（需要网络时先 `source ~/proxy.sh`）：

```bash
cd ~/tasks/photoSelf/ui
npm install
npm run dev
```

看到 `Local: http://127.0.0.1:5173/` 即成功。`Ctrl-C` 停止。

### 方式 B：一条命令

`scripts/dev.sh` 会在后台启动后端，再前台运行 Vite；`Ctrl-C` 退出 Vite 时会同时停掉后端（脚本内 trap 已处理）。脚本用 `PHOTOSELF_CONFIG` 选择配置：

```bash
cd ~/tasks/photoSelf
PHOTOSELF_CONFIG=config.dev.toml ./scripts/dev.sh
```

本仓库未修改 `scripts/dev.sh`；它默认读 `config.toml`，所以必须传 `PHOTOSELF_CONFIG`。

### 后台运行后如何停止

如果不用上面的前台方式，而是自己 `setsid` 放后台，请按进程组停止，避免只杀掉 cargo 而留下 `photoself`：

```bash
cd ~/tasks/photoSelf
setsid cargo run -p photoself-server -- serve -c config.dev.toml \
  > /tmp/photoself-dev/server.log 2>&1 &
echo $! > /tmp/photoself-dev/server.pid
# 停止：
kill -- -"$(cat /tmp/photoself-dev/server.pid)"
```

确认端口已释放：

```bash
ss -ltn | grep -E ':(8787|5173)\b' || echo "8787/5173 free"
```

## 访问地址与账号

| 地址 | 内容 |
|---|---|
| <http://127.0.0.1:5173/> | Vite 开发版 UI（热更新） |
| <http://127.0.0.1:8787/> | 后端直接提供的 `ui/dist`（需先 `npm run build`） |
| <http://127.0.0.1:8787/healthz> | 健康检查，返回 `{"status":"ok"}` |
| <http://127.0.0.1:8787/api/v1/info> | 版本、库数量、管理开关等 |

- 认证：`auth.mode = "none"`，**没有用户名/密码**，浏览器直接打开即可。
- Vite 把 `/api` 和 `/healthz` 代理到 `http://127.0.0.1:8787`（见 `ui/vite.config.js`），所以开发时只访问 5173 也能调 API。
- 后端日志中若出现 `UI build not found`，说明 `ui/dist` 不存在；开发时用 5173，或先构建 UI。

## 常用操作

以下命令默认在仓库根目录执行：

```bash
cd ~/tasks/photoSelf
```

### 扫描

通过 API（服务运行时；写操作需要 `X-Requested-With` 头）：

```bash
curl -sS -H 'X-Requested-With: photoSelf' -X POST http://127.0.0.1:8787/api/v1/scan
# 返回 {"job_id":"..."}；查看进度：
curl -sS http://127.0.0.1:8787/api/v1/jobs/<job_id>
```

或用 CLI（建议先停掉后端，避免两个进程同时写索引；CLI 扫描完即退出）：

```bash
cargo run -p photoself-server -- scan -c config.dev.toml
cargo run -p photoself-server -- scan -c config.dev.toml --library dev
```

查看条目：

```bash
curl -sS 'http://127.0.0.1:8787/api/v1/items?limit=5' | python3 -m json.tool
```

### 清空并重建

删除的只是索引和缓存，示例照片与 `~/Pictures` 原图都不动：

```bash
# 先停止后端（若在运行）
rm -rf /tmp/photoself-dev/data/*
cargo run -p photoself-server -- scan -c config.dev.toml
```

注意：这会丢失收藏、标签、相册和 UI 里添加的库；`config.dev.toml` 中的 `[[libraries]]` 会在下次启动/扫描时自动重新导入。

### 代码检查与测试

```bash
cd ~/tasks/photoSelf
cargo fmt --all -- --check      # 只检查，不修改
cargo fmt --all                 # 需要格式化时
cargo clippy --workspace --all-targets
cargo test --workspace
```

实测：`fmt` 通过、`clippy` 无警告、`cargo test --workspace` 14 个测试全部通过（core 8 + server 6）。

### 构建 UI

```bash
cd ~/tasks/photoSelf/ui
npm run build
```

产物在 `ui/dist/`（含 `sw.js`、`workbox-*.js`、PWA manifest）。`8787` 直接读取该目录，无需重启后端；但浏览器可能被 Service Worker 缓存，见 [PWA 缓存](#pwa-service-worker-缓存导致页面不更新)。

### 构建带嵌入 UI 的单文件二进制

```bash
cd ~/tasks/photoSelf
./scripts/build-release.sh
```

等价分步：

```bash
cd ~/tasks/photoSelf/ui && npm run build
cd ~/tasks/photoSelf
cargo build -p photoself-server --release --features embed-ui
```

产物：`target/release/photoself`。嵌入是编译期行为（`--features embed-ui` 会把 `ui/dist` 编进二进制），此时 `config.dev.toml` 里的 `ui_embedded` 字段不起作用，也不需要 `ui/dist` 在运行机上存在。

```bash
cd ~/tasks/photoSelf
./target/release/photoself serve -c config.dev.toml
```

### 查看日志与调试

- 前端：Vite 终端输出 + 浏览器 DevTools Console。
- 后端：前台终端输出；后台方式见 `/tmp/photoself-dev/server.log`。
- 提高日志级别：

```bash
cd ~/tasks/photoSelf
RUST_LOG=debug cargo run -p photoself-server -- serve -c config.dev.toml
```

- 数据库位置：`/tmp/photoself-dev/data/photoself.db`（无 `sqlite3` CLI，可用 API 或 Python 的 `sqlite3` 模块查看）。

## 本地测试媒体库管理

`config.dev.toml` 已启用：

```toml
[manage]
enabled = true
allowed_roots = ["/tmp/photoself-dev"]
require_auth = false
```

- UI 的 **Libraries** 标签页可以浏览、添加、禁用、删除媒体库，并按库扫描。
- 只能添加 `allowed_roots` 内的绝对路径；目录选择器也限制在其中。
- 会拒绝与 `data.dir`、缩略图/sidecar/回收站目录或其他库重叠的路径（防止重复索引）。
- 删除库只删索引、标签和相册关联，**源文件不动**。

准备一个可添加的测试库：

```bash
mkdir -p /tmp/photoself-dev/extra
cp /tmp/photoself-dev/photos/*.jpg /tmp/photoself-dev/extra/
```

API 示例（`auth.mode = "none"` 时无需 `-u`）：

```bash
BASE=http://127.0.0.1:8787/api/v1
J='-H Content-Type:application/json -H X-Requested-With:photoSelf'

# 列出库（含 item_count、path、source）
curl -sS "$BASE/libraries" | python3 -m json.tool

# 浏览允许根目录
curl -sS "$BASE/fs/dirs?path=/tmp/photoself-dev" | python3 -m json.tool

# 添加测试库（会立即触发一次扫描）
curl -sS $J -X POST \
  -d '{"path":"/tmp/photoself-dev/extra","name":"Extra","read_only":true}' \
  "$BASE/libraries"

# 扫描单个库 / 禁用 / 删除索引（id 换成返回的 id）
curl -sS $J -X POST "$BASE/libraries/<id>/scan"
curl -sS $J -X PATCH -d '{"enabled":false}' "$BASE/libraries/<id>"
curl -sS $J -X DELETE "$BASE/libraries/<id>"
```

> 管理接口的完整字段和错误语义见 [`docs/usage.md`](usage.md#library-management-api-v05)。

## 目录结构

```text
photoSelf/
├── Cargo.toml                  # workspace（core + server，v0.6.0）
├── config.example.toml         # 生产/部署示例配置
├── config.dev.toml             # 本机开发配置（gitignore）
├── crates/
│   ├── photoself-core/         # config/db/scan/thumb/video/model/util
│   └── photoself-server/       # 二进制 photoself：main/api/auth/state/ui
├── ui/                         # Vue 3 + Vite 前端
│   ├── vite.config.js          # 5173 端口；/api、/healthz 代理到 8787
│   ├── src/                    # 组件与页面
│   ├── dist/                   # npm run build 产物（gitignore）
│   └── node_modules/           # npm install（gitignore）
├── scripts/
│   ├── dev.sh                  # 后端后台 + Vite 前台一键启动
│   ├── build-release.sh        # 构建 UI + 嵌入 UI 的 release 二进制
│   ├── make-sidecars.py        # 预生成 HEIC/HEVC 预览
│   └── make-ipk.sh             # OpenWrt 打包（本地开发不需要）
├── docs/                       # 安装、使用、设计、本文件
└── deploy/                     # 部署与 OpenWrt 配置（本地开发不需要）
```

没有 Makefile；构建和检查统一用 `cargo` / `npm`。

本地数据（仓库外）：

```text
/tmp/photoself-dev/
├── data/                       # SQLite、cache/thumbs、cache/tmp、trash
├── photos/                     # 示例库（3 张 JPG，只读）
├── sidecars/                   # sidecar 根目录（当前为空）
└── extra/                      # 可选：媒体库管理测试用
```

## 当前实测记录（2026-11-27）

| 项目 | 命令 / 端口 | 结果 |
|---|---|---|
| 首次扫描 | `cargo run -p photoself-server -- scan -c config.dev.toml` | `scanned 3, errors 0, missing 0` |
| 后端启动 | `serve -c config.dev.toml`，127.0.0.1:8787 | 启动成功 |
| 健康检查 | `GET /healthz` | 200 `{"status":"ok"}` |
| 信息 | `GET /api/v1/info` | 200，`item_count=3`，`manage_enabled=true`，`version=0.6.0` |
| 列表 | `GET /api/v1/items?limit=5` | 200，3 条 |
| 缩略图 | `GET /api/v1/items/<id>/thumb/320` | 200，`image/jpeg`，29 KB |
| 触发扫描 | `POST /api/v1/scan` + `X-Requested-With` | 200，返回 job_id |
| 媒体库列表 | `GET /api/v1/libraries` | 200，`item_count=3`，`source=config` |
| 目录浏览 | `GET /api/v1/fs/dirs?path=/tmp/photoself-dev` | 200 |
| 前端 | `npm run dev`，127.0.0.1:5173 | 200 `text/html` |
| Vite 代理 | `GET 5173/api/v1/info`、`5173/healthz` | 200 |
| 一键启动 | `PHOTOSELF_CONFIG=config.dev.toml ./scripts/dev.sh` | 两端口均可用、代理正常 |
| 清空重建 | `rm -rf data/*` 后 CLI 扫描 | 重新索引 3 条 |
| 静态检查 | fmt / clippy / test | 通过；14 tests passed |
| UI 构建 | `npm run build` | 生成 `ui/dist` 与 SW |
| 嵌入构建 | `cargo build --release --features embed-ui` | 成功（约 1 分 25 秒，`target/release/photoself` 5.7 MB） |
| 嵌入二进制 | 在 `/tmp`（无 `ui/dist`）启动，8787 | `/` 200 `text/html`、JS 资源 200、`/api/v1/info` 200 |

> **为什么用隔离网络命名空间验证**：本机 8787 端口当前被一个与本项目无关的 Node 演示服务占用（见 FAQ）。为严格按 `config.dev.toml` 的 8787/5173 验证，且不影响该进程，完整链路是在 `unshare -rn`（独立网络命名空间）里执行的；这等价于验证配置和代码，但**不等于宿主机 8787 现在空闲**。宿主机直接运行前请先按 FAQ 腾出端口。

## 常见问题

### 8787 端口被占用

本机现状（2026-11-27 检查）：

```text
LISTEN 127.0.0.1:8787  node server.js
```

这是一个与本项目无关、长期运行的 Node 演示站点（不是 photoSelf）。**不要直接 kill 别人的进程**；先确认它是什么、是否还需要：

```bash
ss -ltnp | grep ':8787'
ps -fp <pid>
readlink /proc/<pid>/cwd
```

处理方式：

1. 确定不需要时，用它自己的启动方式/进程管理停止，然后启动 photoSelf。
2. 不能停时，只能临时把后端 `server.listen` 和 `ui/vite.config.js` 的 proxy 目标一起改到别的端口（例如 8788）。这是本地未提交改动，验证后应还原；本次环境没有采用这种方式，因为需求指定 8787。

前端症状：如果 8787 被别的服务占着，Vite 的 `/api` 代理会把请求发给那个服务（可能返回 HTML 或 404），而不是 photoSelf。

确认 photoSelf 没留下残留：

```bash
ps -eo pid,cmd | grep -E 'photoself|vite' | grep -v grep || echo "clean"
ss -ltn | grep -E ':(8787|5173)\b' || echo "ports free"
```

### 5173 端口被占用

Vite 默认会自动换到 5174 并在终端打印实际地址；若使用 `--strictPort` 则直接失败。代理配置不随端口改变，仍指向 8787，因此换端口不影响 API。按终端提示的新地址访问即可。

### 安装依赖 / 拉取 crates 需要代理

本项目本地运行不需要外网；只有 `npm install`、`cargo fetch`、首次下载 crates 时需要：

```bash
source ~/proxy.sh
cd ~/tasks/photoSelf/ui && npm install
cd ~/tasks/photoSelf && cargo fetch
```

代理脚本会设置 `http_proxy`/`https_proxy` 等环境变量。若访问本机地址被代理拦截，可在当前终端临时清除：

```bash
unset http_proxy https_proxy all_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY
```

（具体变量以 `~/proxy.sh` 内容为准。）

### HEIC / HEVC 依赖 ffmpeg

- 本机 ffmpeg 6.1.1 含 `hevc` 解码器、`libx264`、`aac`；视频封面和可选转码可用。
- 检查：

```bash
ffmpeg -hide_banner -decoders | grep -Ei 'hevc|heif'
ffmpeg -hide_banner -encoders | grep -E 'libx264|aac'
```

- 普通图片缩略图由 Rust `image` 处理；HEIC/HEIF 等格式若 `image` 解不了会回退到 ffmpeg。iPhone 有时把 HEIC 存成 `.JPG` 后缀，浏览器无法直接显示，需要预生成预览：

```bash
mkdir -p /tmp/photoself-dev/sidecars/dev
python3 ~/tasks/photoSelf/scripts/make-sidecars.py \
  --source /tmp/photoself-dev/photos \
  --output /tmp/photoself-dev/sidecars/dev \
  --sizes 320,1280 --jobs 2
```

`config.dev.toml` 已设置 `thumbnails.sidecar_dir = "/tmp/photoself-dev/sidecars"`，sidecar 按 `<sidecar_dir>/<library-id>/...` 查找。脚本需要 Pillow 与 `heif-convert`（本机均已有）；若缺失，按脚本头部说明安装，但不要用 `sudo` 安装系统包超出本任务范围。

- 没有 ffmpeg 时：设置 `[ffmpeg] enabled = false`、`[scan] probe_videos = false`；图片仍可浏览，视频不会有封面/时长。

### PWA Service Worker 缓存导致页面不更新

`npm run build` 后如果浏览器仍显示旧界面，通常是旧 Service Worker 缓存。处理步骤：

1. 打开 DevTools → **Application** → **Service Workers**，点 **Unregister**。
2. 同一面板 → **Storage** → **Clear site data**（勾选 Cache storage / Service workers）。
3. `Ctrl+Shift+R` 强制刷新；或直接用无痕窗口访问。
4. 若仍不更新，换端口访问一次（如 `http://127.0.0.1:5173/`），确认不是代理/缓存层问题。

开发时推荐用 5173 的 Vite 模式：`vite-plugin-pwa` 没有启用 `devOptions`，开发服务器通常不注册 SW，能避开大部分缓存问题。

### `/tmp/photoself-dev` 重启后不见了

WSL/系统重启可能清空 `/tmp`。重新执行 [目录与示例照片](#1-目录与示例照片) 和扫描命令即可；`config.dev.toml` 在仓库里，不会丢。若希望长期保留，可把数据目录换到 `~/photoself-dev`，同时修改 `config.dev.toml` 的 `data.dir`、库路径和 `allowed_roots`。

### 没有 `sqlite3` 命令

本机未安装 sqlite3 CLI，不影响运行。查数据用 API：

```bash
curl -sS http://127.0.0.1:8787/api/v1/items?limit=5 | python3 -m json.tool
```

或 Python：

```bash
python3 - <<'PY'
import sqlite3
con = sqlite3.connect('/tmp/photoself-dev/data/photoself.db')
print(con.execute('select count(*) from items').fetchone())
PY
```

（表名如与版本不符，用 `.tables` 等价方式在 Python 里查询 `sqlite_master`。）

### 权限 / 路径错误

- 库路径必须存在且可读；`allowed_roots` 必须是绝对路径。
- 不允许把 `data.dir`、缓存、sidecar、回收站或已有库的子目录/父目录再加成库，管理接口会拒绝。
- 本环境只使用 `/tmp/photoself-dev`，不要指向 `/mnt/drive`（该盘有介质错误）或生产照片目录。

## 安全与清理

- 所有监听都是 `127.0.0.1`；不要改成 `0.0.0.0`，不要把端口转发到局域网。
- `auth.mode = "none"` + `manage.require_auth = false` 仅限本机；`allowed_roots` 已限制在 `/tmp/photoself-dev`。
- `config.dev.toml` 不含任何密码或密钥；该文件被 gitignore，示例数据在 `/tmp`。
- 结束开发：

```bash
# 停掉前台进程用 Ctrl-C；后台进程按上文进程组 stop
ps -eo pid,cmd | grep -E 'photoself|vite' | grep -v grep || echo "clean"
# 可选：删除全部本地测试数据（不会碰 ~/Pictures）
rm -rf /tmp/photoself-dev
```

## 附录：重建 `config.dev.toml`

若文件丢失，在仓库根目录执行：

```bash
cd ~/tasks/photoSelf
cat > config.dev.toml <<'EOF'
# photoSelf local development configuration (not for production).
[server]
mode = "standalone"
listen = "127.0.0.1:8787"
ui_dir = "ui/dist"
ui_embedded = false
trusted_proxies = ["127.0.0.1/32", "::1/128"]

[data]
dir = "/tmp/photoself-dev/data"

[[libraries]]
id = "dev"
name = "Dev sample library"
path = "/tmp/photoself-dev/photos"
read_only = true
enabled = true
ignore_globs = ["._*", ".DS_Store", "@eaDir", "#recycle"]

[manage]
enabled = true
allowed_roots = ["/tmp/photoself-dev"]
require_auth = false

[media]
serve_mode = "standalone"

[thumbnails]
sidecar_dir = "/tmp/photoself-dev/sidecars"
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
transcode_max_width = 1280
transcode_crf = 23

[auth]
mode = "none"
session_ttl_hours = 720

[scan]
on_start = false
interval = "0"
concurrency = 1
follow_symlinks = false
hash = true
probe_videos = true

[trash]
enabled = true
retention_days = 30

[write]
enabled = false
max_upload_bytes = 536870912

[logging]
level = "info"
format = "text"
EOF
```

确认仍被忽略：

```bash
cd ~/tasks/photoSelf
git check-ignore -v config.dev.toml
```