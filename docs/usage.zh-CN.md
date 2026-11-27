# 使用说明

简体中文 | [English](usage.md)

## 第一次使用

1. 启动服务：

   ```bash
   photoself serve -c config.toml
   ```

2. 打开 `http://127.0.0.1:8787/`。
3. 点击 **扫描**（或命令行执行 `photoself scan -c config.toml`）。
4. 时间线按天分组；可以搜索、收藏、打开灯箱。

如果启用了可选的媒体库管理（见下文「媒体库」），可以直接在「媒体库」页添加目录，
无需改配置或重启。

扫描只读取文件，不会移动、重命名或删除照片。

## 界面说明

| 区域 | 作用 |
|---|---|
| 时间线 | 按天分组的网格、懒加载缩略图、无限滚动 |
| 相册 | 创建相册、增删条目、封面视图 |
| 回收站 | 恢复或永久删除回收站条目 |
| 媒体库 | （v0.5+，需 `manage.enabled`）添加/停用/移除媒体库、单库扫描、服务端目录选择器 |
| 搜索/标签 | 文件名与标签筛选 |
| 收藏 | 只看已收藏条目 |
| 扫描 | 为所有启用的媒体库启动后台扫描并显示进度 |
| 灯箱 | 图片缩放/拖动、视频播放、元数据、标签、相册、下载 |
| 语言 | 中文 / English 切换；默认中文，选择会记住 |
| 主题 | 顶部浅色/深色切换；首次跟随系统，之后记住选择 |
| 右键 / 长按 | 缩略图菜单：查看、下载、加入相册、删除 |
| 选择 | 多选模式；Ctrl/Cmd 单击切换，Shift 单击选择范围，底部栏批量操作 |

### 缩略图选择与批量操作

- **右键单击**缩略图（触屏长按）打开菜单：**查看、下载、加入相册、删除**。
  右键未选中的图会先选中它；在已有选择上右键会保留多选。
- 顶栏点「**选择**」进入多选模式；`Ctrl`/`Cmd` 单击切换选中，`Shift` 单击
  选择连续范围。触屏可先点「选择」再点按缩略图（或长按打开菜单）。
- 底部**选择栏**显示已选数量，并提供 **查看 / 下载 / 加入相册 / 删除 / 取消**。
  选择只作用于当前时间线或相册视图，切换视图或搜索时会清空。
- 批量**删除**仅在选中的全部条目都属于可写媒体库（`read_only = false`）且
  `[write]`、`[trash]` 均开启时可用；逐条移入回收站，部分失败会提示。
- 批量**下载**会逐个触发下载，浏览器可能提示“允许多个文件下载”。
- 批量**加入相册**只写元数据，只读媒体库也可以使用。

### 灯箱与竖屏布局

- **缩放**：滚轮、双指捏合、双击/双指双击，或 `−` / `+` 按钮（中间的百分比
  按钮复位）。范围 1×–5×。
- **拖动**：放大后拖拽查看局部；左右箭头或键盘方向键切换条目，切换时自动复位缩放。
- **键盘**：`+` / `-` 缩放，`0` 复位，`←` / `→` 上一张/下一张，`Esc` 关闭。
- 放大到约 **125%** 以上时会在预览图上层加载原图，保证细节清晰。图片灯箱默认使用
  **1280 px JPEG 预览**（`preview_url`），这样即使浏览器不能解码 HEIC 也能显示；
  下载链接始终指向原文件。
- **竖屏界面（不限宽度）**：元数据、标签和操作移入底部抽屉，默认收起，点击
  文件名栏展开；抽屉内容可独立滚动。左右滑动切换图片；旋转设备时自动切换布局。
  **横屏/桌面**时照片铺满窗口，只留 5 px 边距；信息面板浮在右侧，点 ⓘ 可收起/展开，
  关闭、缩放和翻页按钮始终可见。缩放画面会被裁剪在查看区域内，不会溢出屏幕。

## 媒体库

每个库有 `id`、路径和忽略规则：

```toml
[[libraries]]
id = "family"
name = "Family photos"
path = "/photos"
read_only = true
ignore_globs = ["._*", ".DS_Store", "@eaDir", "#recycle"]
```

支持多个库。Sidecar 按库隔离：

```text
<sidecar_dir>/<library-id>/<relative-path>.<size>.jpg
<sidecar_dir>/<library-id>/<relative-path>.meta.json
```

避免两个库里有同名文件时互相串图。

### 在界面管理媒体库（v0.5+）

管理功能**默认关闭**。首次导入后，媒体库以 SQLite 为准，可以在「媒体库」页添加、
停用或移除。先在配置里开启一次并重启：

```toml
[manage]
enabled = true
allowed_roots = ["/mnt/sda1", "/mnt/photos"]
require_auth = true
```

操作流程：

1. 打开「媒体库」页。
2. 点「添加媒体库」，可以直接输入绝对路径，或点「浏览目录」在服务端目录选择器中
   逐级选择。
3. 可选填显示名称，建议保持勾选「只读」。
4. 点「添加并扫描」。库会立即出现并启动扫描，状态栏显示进度。

每张卡片可以**扫描**、**启用/停用**、**移除**；界面不会修改 `config.toml`。

- 界面只能添加 `allowed_roots` 内的绝对路径；内置目录选择器不会越过白名单。
  符号链接会先解析，解析后仍必须在白名单内。
- 服务端会拒绝与 `data.dir`、缩略图缓存、sidecar/回收站目录或已有媒体库重叠的路径
  （嵌套目录会被重复索引）。
- **移除**只删除索引、标签和相册关联，原文件和预生成 sidecar 不会被删除；
  该库的回收站索引行会被清掉，但回收站里的文件保留在磁盘上。
- 停用会保留索引，但不再参与扫描和时间线；对配置来源的库，停用状态重启后保持。
- `config.toml` 只用于补充新 id 和升级 v0.5 之前的旧记录；「从配置重新导入」
  会用配置强制覆盖同 id 记录的名称、路径、启用状态、忽略规则，并清除已删除墓碑；
  界面新增的库不受影响。
- `auth.mode = "none"` 时默认拒绝管理接口；确实需要无认证局域网管理时，
  显式设置 `require_auth = false`。

## 扫描

- **手动**：点顶栏「扫描」扫描所有启用的库；或在「媒体库」卡片上点「扫描」只扫
  该库。API 分别是 `POST /api/v1/scan` 和 `POST /api/v1/libraries/{id}/scan`。
- **命令行**：`photoself scan -c config.toml`（可加 `--library family`）。
  命令行会先把配置里的库导入数据库，再按数据库中的注册表扫描，与界面看到的库一致。
- **启动时扫描**：`scan.on_start = true`（示例配置默认）。
- **增量**：size+mtime 未变的文件复用已有哈希；USB/NAND 上的重复扫描很轻。
- **缺失文件**：标记为 missing，不再从 API 返回，但不会删除数据库记录。

扫描任务会报告状态和计数：

```json
{
  "state": "completed",
  "progress": 100,
  "message": "scanned 12345, errors 0, missing 0"
}
```

## 收藏与搜索

```bash
# 所有 POST/PUT/PATCH/DELETE 都要带 X-Requested-With: photoSelf
# 收藏
curl -u admin:password -X PATCH \
  -H 'X-Requested-With: photoSelf' \
  -H 'Content-Type: application/json' \
  -d '{"favorite":true}' \
  http://127.0.0.1:8787/api/v1/items/<id>

# 搜索
curl -u admin:password \
  'http://127.0.0.1:8787/api/v1/items?q=IMG_2023&limit=50'
```

## API

接口前缀 `/api/v1`，认证方式由 `auth.mode` 决定。

```bash
PASS='your-password'

curl -u "admin:$PASS" http://127.0.0.1:8787/api/v1/info
curl -u "admin:$PASS" 'http://127.0.0.1:8787/api/v1/items?limit=100'
curl -u "admin:$PASS" -H 'X-Requested-With: photoSelf' -X POST http://127.0.0.1:8787/api/v1/scan
curl -u "admin:$PASS" http://127.0.0.1:8787/api/v1/jobs/<job-id>
```

游标分页：把返回的 `next_cursor` 作为 `cursor` 传入。可用过滤：
`library`、`kind`（`image`/`video`）、`q`、`favorite`、`from`、`to`、`limit`。

`GET /api/v1/info` 会返回 `manage_enabled`，前端据此决定是否显示管理入口。

### 媒体库管理 API（v0.5+）

需要 `[manage] enabled = true`；写操作还要带 `X-Requested-With: photoSelf`。
`auth.mode = "none"` 且 `require_auth = true` 时，以下接口全部返回 `403`。

```bash
AUTH='-u admin:password'
J='-H Content-Type:application/json -H X-Requested-With:photoSelf'
B=http://127.0.0.1:8787/api/v1

# 列出媒体库（path、source、enabled、item_count 等）
curl $AUTH "$B/libraries"

# 添加媒体库，并自动启动扫描
curl $AUTH $J -X POST -d '{"path":"/mnt/sda1/photos","name":"照片","read_only":true}' \
  "$B/libraries"

# 改名 / 停用 / 启用 / 改忽略规则
curl $AUTH $J -X PATCH -d '{"name":"家庭照片","enabled":false}' "$B/libraries/<id>"

# 单库扫描
curl $AUTH $J -X POST "$B/libraries/<id>/scan"

# 用 config.toml 强制覆盖同 id 记录
curl $AUTH $J -X POST "$B/libraries/import-config"

# 移除媒体库（只删索引，不动原文件）
curl $AUTH $J -X DELETE "$B/libraries/<id>"

# 服务端目录浏览（限定在 manage.allowed_roots 内）
curl $AUTH "$B/fs/dirs?path=/mnt/sda1"
curl $AUTH "$B/fs/dirs"        # 列出允许的根目录
```

`POST /api/v1/libraries` 接受 `path`（必填）、`name`、`read_only`（默认
`true`）、`ignore_globs`（默认 `["._*", ".DS_Store", "@eaDir", "#recycle"]`）。
路径会做 canonicalize；`data.dir`、缩略图/sidecar/回收站目录以及重叠的库都会被拒绝。

媒体接口支持 HTTP Range：

```bash
curl -u "admin:$PASS" -H 'Range: bytes=0-99' \
  http://127.0.0.1:8787/api/v1/items/<id>/media
# 206 Partial Content
```

## Sidecar 预览

低功耗设备没有软件 HEIC/HEVC 解码器时，在 PC/NAS 上预生成：

```bash
./scripts/make-sidecars.py \
  --source /mnt/router/picture \
  --output /mnt/router/usb/photo-app/sidecars/family \
  --sizes 320,1280 --video-sizes 320 --jobs 2
```

配置：

```toml
[thumbnails]
sidecar_dir = "/mnt/usb/photo-app/sidecars"
```

- 服务端优先读取 sidecar，读不到才尝试解码原文件。
- `.meta.json` 在 ffprobe 无法解码时补齐宽高/时长/日期。
- 导入新的 HEIC/HEVC 素材后重跑；`--meta-only` 只补元数据；已有预览默认跳过，
  加 `--force` 强制重生成。

## 标签、相册、上传与回收站

- **标签**：打开条目，在灯箱里输入/选择标签，顶部可按标签筛选。
  `POST /api/v1/tags` 创建标签；`POST /api/v1/items/{id}/tags` 挂载。
- **相册**：切到「相册」创建相册，在灯箱中把当前条目加入相册。
  相册只是虚拟关系，不移动文件。
- **上传**：`[write] enabled = true` 且库可写时，点击「上传」选择图片/视频。
  服务端先写临时文件，随后立即索引。
- **删除/回收站**：灯箱的「删除」会把文件移到
  `<data.dir>/trash/<library-id>/` 并把行标记为 missing。打开「回收站」恢复
  或永久删除。
- **搜索过滤**：文件名、收藏、标签过滤作用于时间线；API 还支持
  `kind`、`library`、`from`、`to`、`album`。

## 网页版视频

设置 `video.transcode_enabled = true`，并确保 `ffmpeg` 带 `libx264` 和 `aac`。
在灯箱中点击「生成网页版」；服务端会在 `data/cache/derived/` 排队生成，
并返回 `play_url`（H.264/AAC MP4）。

## 定时扫描与 PWA

- `scan.interval = "15m"` 每 15 分钟后台扫描一次。支持 `s`、`m`、`h`、`d`；
  `"0"` 关闭。
- UI 带 PWA manifest 和 service worker。用 Chrome/Edge（HTTPS 或 localhost）
  安装后可独立窗口打开，并缓存应用壳。

## 认证

`auth.mode = "none"` 只适合可信局域网。单管理员：

```bash
photoself hash-password --password '你的密码'
```

```toml
[auth]
mode = "basic"
username = "admin"
password_hash = "$argon2id$..."
```

在可信反向代理后：

```toml
[auth]
mode = "proxy"
```

代理必须完成认证并设置 `X-Forwarded-User`；只有 `server.trusted_proxies`
（CIDR/裸 IP）中的网段可以发送该头，绝不要直接信任来自公网的请求。

## 数据、备份与恢复

| 路径 | 内容 |
|---|---|
| `data.dir/photoself.db` | SQLite 索引、用户元数据和运行期媒体库注册表 |
| `data.dir/cache/thumbs/` | 生成的缩略图（可丢弃） |
| `data.dir/cache/tmp/` | 临时文件 |
| `thumbnails.sidecar_dir` | 预生成预览/元数据（可选） |

- 收藏、标签、相册或界面维护的媒体库列表重要时，备份 `data.dir`。
- 删除数据库是安全的：重新扫描即可重建索引；收藏/标签和界面新增的媒体库会丢失
  （配置里的 `[[libraries]]` 会在下次启动时重新导入）。
- 照片根目录永远不会被修改，界面移除媒体库也一样。
- Sidecar 可随时重新生成。

## 512 MB 设备的性能建议

```toml
[thumbnails]
sizes = [320]
concurrency = 1

[ffmpeg]
concurrency = 1

[video]
transcode_enabled = false

[scan]
on_start = false
concurrency = 1
```

- 优先使用 sidecar，避免在设备上解码 HEVC/HEIF。
- 缓存和 SQLite 放可写 USB 盘，不要写 NAND。
- 媒体文件交给反向代理，不要用 Rust 转发视频。
- 常驻内存约 5 MB；扫描峰值约 20–30 MB（外加外部 ffmpeg）。

## 已知行为与限制

- 写入模式默认关闭；开启后 API 可以修改照片库。
- 界面媒体库管理默认关闭，需要显式配置 `[manage].allowed_roots` 白名单并要求认证。
- 移除媒体库会永久删除索引、标签和相册关联，没有撤销；源目录不受影响。
- 回收站不会自动过期，需要手动清除旧条目。
- 视频默认播放原文件；生成 web MP4 后才能在所有浏览器播放。MOV/HEVC 兼容性
  取决于浏览器（Safari/Apple 通常可以，Chrome/Firefox 可能不行）。
- HEIC 可以索引并通过 sidecar 预览；原文件仍是 HEIC。
- 无时区 EXIF 日期按本地裸时间存储。
- 搜索是对文件名做 `LIKE`，不是全文搜索。
- UI 目前把所有库混在一条时间线里；按库筛选在计划中。
- 标签/相册只是元数据，不会自动整理文件。
- 界面默认简体中文；切换英文会按浏览器记住（`localStorage`）。服务端错误信息仍是英文。
- 改名可通过 `PATCH /api/v1/libraries/{id}` 完成；界面目前提供启用/停用、扫描和移除。
