# 使用说明

简体中文 | [English](usage.md)

## 第一次使用

1. 启动服务：

   ```bash
   photoself serve -c config.toml
   ```

2. 打开 `http://127.0.0.1:8787/`。
3. 点击 **Scan**（或命令行执行 `photoself scan -c config.toml`）。
4. 时间线按天分组；可以搜索、收藏、打开灯箱。

扫描只读取文件，不会移动、重命名或删除照片。

## 界面说明

| 区域 | 作用 |
|---|---|
| 时间线 | 按天分组的网格、懒加载缩略图、无限滚动 |
| 搜索 | 文件名搜索（`q` 参数） |
| 收藏 | 只看已收藏条目 |
| 扫描 | 启动后台扫描任务并显示进度 |
| 灯箱 | 大图预览（1280 JPEG）、视频播放、元数据、下载 |
| ★ | 切换收藏 |

图片灯箱使用 **1280 px JPEG 预览**（`preview_url`），而不是原文件；这样即使浏览器
不能解码 HEIC 也能正常显示。下载链接仍然指向原文件。

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

## 扫描

- **手动**：UI 点击 **Scan**，或 `POST /api/v1/scan`。
- **命令行**：`photoself scan -c config.toml`（可加 `--library family`）。
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
# 收藏
curl -u admin:password -X PATCH \
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
curl -u "admin:$PASS" -X POST http://127.0.0.1:8787/api/v1/scan
curl -u "admin:$PASS" http://127.0.0.1:8787/api/v1/jobs/<job-id>
```

游标分页：把返回的 `next_cursor` 作为 `cursor` 传入。可用过滤：
`library`、`kind`（`image`/`video`）、`q`、`favorite`、`from`、`to`、`limit`。

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

代理必须完成认证并设置 `X-Forwarded-User`；绝不要直接信任来自公网的该请求头。

## 数据、备份与恢复

| 路径 | 内容 |
|---|---|
| `data.dir/photoself.db` | SQLite 索引 + 用户元数据 |
| `data.dir/cache/thumbs/` | 生成的缩略图（可丢弃） |
| `data.dir/cache/tmp/` | 临时文件 |
| `thumbnails.sidecar_dir` | 预生成预览/元数据（可选） |

- 收藏/元数据重要时备份 `data.dir`。
- 删除数据库是安全的：重新扫描即可重建索引；收藏会丢失。
- 照片根目录永远不会被修改。
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

- 标签、相册、上传/删除、回收站、分享链接尚未实现。
- 视频按原文件播放；MOV/HEVC 兼容性取决于浏览器（Safari/Apple 通常可以，
  Chrome/Firefox 可能不行）。
- HEIC 可以索引并通过 sidecar 预览；原文件仍是 HEIC。
- 无时区 EXIF 日期按本地裸时间存储。
- 搜索是对文件名做 `LIKE`，不是全文搜索。
- UI 目前把所有库混在一条时间线里；按库筛选在计划中。
