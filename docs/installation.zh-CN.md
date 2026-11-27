# 安装文档

简体中文 | [English](installation.md)

photoSelf 以单个 Rust 二进制运行，前端可选内嵌。它直接读取现有目录，不会导入或搬动照片。

> 预编译 Linux 二进制（x86_64、aarch64、armv7）和 OpenWrt `.ipk`
> 安装包发布在 [Releases](https://github.com/shinvdu/photoSelf/releases)。
> 其他架构请从源码构建。

### 预编译发布包

| 文件 | 用途 |
|---|---|
| `photoself-<version>-x86_64-unknown-linux-musl.tar.gz` | 静态 Linux x86_64 二进制 |
| `photoself-<version>-aarch64-unknown-linux-musl.tar.gz` | ARM64 / aarch64 Linux |
| `photoself-<version>-armv7-unknown-linux-musleabihf.tar.gz` | ARMv7 硬浮点 |
| `photoself_<version>_aarch64_cortex-a53.ipk` | OpenWrt 23.05/24.10 opkg，常见 ARM64 目标 |
| `photoself_<version>_arm_cortex-a7_neon-vfpv4.ipk` | OpenWrt 23.05/24.10 opkg，常见 ARMv7 目标 |
| `photoself_<version>_x86_64.ipk` | OpenWrt x86_64 |
| `photoself-<version>-openwrt-aarch64.tar.gz` | apk 版 OpenWrt / 手工安装 |
| `photoself-<version>-openwrt-armv7.tar.gz` | apk 版 OpenWrt / 手工安装 |
| `photoself-<version>-openwrt-x86_64.tar.gz` | OpenWrt x86_64 apk / 手工安装 |

`.ipk` 会安装 `/usr/bin/photoself`、`/etc/init.d/photoself`、
`/etc/photoself/config.toml.example`，并在首次安装时创建
`/etc/photoself/config.toml`。**启动前先编辑配置**（路径是占位符；默认
`auth.mode = "none"`，仅适合可信局域网）。升级会保留已有配置。

```sh
opkg install photoself_<version>_aarch64_cortex-a53.ipk
vi /etc/photoself/config.toml
/etc/init.d/photoself enable
/etc/init.d/photoself start
```

apk 版 OpenWrt（25.12+）可把 `openwrt-*.tar.gz` 解压到 `/`（或从源码构建），
同样先编辑配置。未列出的架构请使用原始二进制 tar.gz 或从源码构建。

## 环境要求

| 组件 | 说明 |
|---|---|
| 系统 | Linux（glibc 或 musl）、OpenWrt、macOS/Windows 开发环境 |
| CPU | x86_64、aarch64、armv7；OpenWrt 建议静态 musl |
| Rust | stable（本项目在 1.97 上测试） |
| Node.js | 20+（仅构建前端时需要） |
| C 工具链 | bundled SQLite 需要；musl 可用 zig 或 OpenWrt SDK |
| ffmpeg/ffprobe | 可选；视频海报和视频元数据 |
| Python 3 + Pillow | 可选；在 PC 上生成 sidecar 预览 |
| heif-convert | 可选；sidecar 的 HEIC 解码（libheif-examples） |

实际常驻内存约 **50 MB**（API）；缩略图和外部 ffmpeg 任务会有峰值。512 MB
设备可用，建议 `thumbnails.concurrency = 1`。

---

## 方式 A — 源码构建（开发推荐）

```bash
git clone https://github.com/shinvdu/photoSelf.git
cd photoSelf

# 1. 构建前端
cd ui
npm ci
npm run build
cd ..

# 2. 配置
cp config.example.toml config.toml
$EDITOR config.toml

# 3. 运行
cargo run -p photoself-server -- serve -c config.toml
```

打开 `http://127.0.0.1:8787/`，点击 **Scan**。

### 单二进制（前端内嵌）

```bash
cd ui && npm ci && npm run build && cd ..
cargo build -p photoself-server --release --features embed-ui
# 产物：target/release/photoself
./target/release/photoself serve -c config.toml
```

`embed-ui` 特性要求编译时 `ui/dist/` 已存在；之后无需单独的 UI 目录。

### 交叉编译 ARM64 musl（OpenWrt / NAS）

bundled SQLite 需要 C 编译器，`cargo-zigbuild` 会同时处理 Rust 和 C 部分：

```bash
rustup target add aarch64-unknown-linux-musl
cargo install cargo-zigbuild --locked

# Zig 0.14.x，放到 PATH（或设置 ZIG）
export PATH="$HOME/opt/zig-x86_64-linux-0.14.1:$PATH"

(cd ui && npm ci && npm run build)
cargo zigbuild --release --target aarch64-unknown-linux-musl \
  -p photoself-server --features embed-ui

file target/aarch64-unknown-linux-musl/release/photoself
# ELF 64-bit LSB executable, ARM aarch64, statically linked
```

也可以使用 OpenWrt SDK 工具链，把 `CC` /
`CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER` 指向
`aarch64-openwrt-linux-musl-gcc`。

---

## 方式 B — Docker

```bash
docker build -t photoself .
cp deploy/docker/config.example.toml config.docker.toml
# 修改 /photos、/data 等路径

docker run -d --name photoself \
  -p 8787:8787 \
  -v /srv/photos:/photos:ro \
  -v photoself-data:/data \
  -v "$PWD/config.docker.toml:/data/config.toml:ro" \
  photoself
```

Dockerfile 使用 `public.ecr.aws/docker/library/*`（官方镜像的 AWS Public ECR
镜像源）。如果构建容器需要代理：

```bash
docker build --network=host \
  --build-arg HTTP_PROXY=http://127.0.0.1:1081 \
  --build-arg HTTPS_PROXY=http://127.0.0.1:1081 \
  -t photoself .
```

---

## 方式 C — OpenWrt / 低功耗 Linux

1. 按上面的方式交叉编译（`cargo zigbuild`）。
2. 安装程序和 init：

   ```sh
   scp target/aarch64-unknown-linux-musl/release/photoself root@router:/usr/bin/photoself
   scp config.example.toml root@router:/etc/photoself/config.toml
   scp deploy/openwrt/photoself.init root@router:/etc/init.d/photoself
   ssh root@router 'chmod +x /etc/init.d/photoself
     /etc/init.d/photoself enable
     /etc/init.d/photoself start'
   ```

3. 程序/配置放 NAND，`data.dir` 放可写 USB 盘。
4. 设置 `thumbnails.concurrency = 1`、`video.transcode_enabled = false`。
5. 前面用 lighttpd/Nginx 做 TLS、认证和媒体服务。参考
   [`../deploy/openwrt/README.md`](../deploy/openwrt/README.md) 与
   [`../deploy/lighttpd/94-photoself.conf`](../deploy/lighttpd/94-photoself.conf)。

如需 `.ipk`（opkg）或 `.apk`（apk 版 OpenWrt），用 OpenWrt SDK / buildroot，
或使用发行版工具把二进制和 init 脚本打包。仓库内的
`deploy/openwrt/photoself.init` 是 procd 服务。

---

## 配置

最小 `config.toml`：

```toml
[server]
listen = "127.0.0.1:8787"

[data]
dir = "/var/lib/photoself"

[[libraries]]
id = "photos"
name = "Photos"
path = "/photos"
read_only = true

[thumbnails]
sizes = [320, 1280]
concurrency = 1

[ffmpeg]
enabled = true
concurrency = 1

[auth]
mode = "none"
```

关键选项：

| 键 | 含义 |
|---|---|
| `server.listen` | 反向代理后面用 `127.0.0.1:8787`；可信局域网用 `0.0.0.0:8787` |
| `data.dir` | SQLite、缩略图缓存、临时文件 |
| `[[libraries]].path` | 照片根目录；尽量只读挂载 |
| `media.serve_mode` | `standalone` 或 `proxy` |
| `auth.mode` | `none`、`basic`、`proxy` |
| `thumbnails.sidecar_dir` | 可选的预生成预览根目录 |
| `scan.hash` | BLAKE3 内容哈希，用于重命名识别 |

在 UI 或 CLI 触发扫描：

```bash
photoself scan -c config.toml
```

### 认证

生成 Argon2id 哈希：

```bash
photoself hash-password --password '你的密码'
```

写入配置：

```toml
[auth]
mode = "basic"
username = "admin"
password_hash = "$argon2id$..."
```

Basic 模式是单用户。若在可信反向代理后，可用 `auth.mode = "proxy"`；代理必须
完成认证并设置 `X-Forwarded-User`。


### 写入模式、回收站与视频派生

写操作默认关闭：

```toml
[write]
enabled = true
max_upload_bytes = 536870912

[[libraries]]
id = "photos"
path = "/photos"
read_only = false     # 上传/删除必须可写

[trash]
enabled = true
retention_days = 30
```

- `POST /api/v1/upload` 接收 multipart `library` + `file`。
- `DELETE /api/v1/items/{id}` 会把文件移到 `<data.dir>/trash/<library-id>/`，
  并把条目标记为 missing；通过 `/api/v1/trash` 恢复或永久清除。
- 开启写入模式意味着 API（以及认证）可以修改照片库；必须放在认证之后，
  绝不能直接暴露到公网。
- 视频网页版：设置 `video.transcode_enabled = true`，并安装带 H.264 编码器的
  ffmpeg；UI/API 会排队生成并返回 `play_url`。
- 定时扫描：`scan.interval = "15m"`（支持 `s`、`m`、`h`、`d`）。
- UI 是 PWA；通过 HTTPS（或 localhost）在浏览器中安装。

---

## systemd

```bash
sudo useradd --system --home /var/lib/photoself --shell /usr/sbin/nologin photoself 2>/dev/null || true
sudo install -m 0755 target/release/photoself /usr/local/bin/photoself
sudo install -d -o photoself -g photoself /var/lib/photoself
sudo install -d /etc/photoself
sudo install -m 0600 config.toml /etc/photoself/config.toml
sudo install -m 0644 deploy/systemd/photoself.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now photoself
```

参考 [`../deploy/systemd/photoself.service`](../deploy/systemd/photoself.service)。

---

## 反向代理

- TLS 和认证可以在 Nginx/Caddy/lighttpd 终止。
- `media.serve_mode = "proxy"` 时，把 `/media/<library-id>/<relative-path>` 映射到
  库根目录，把 `/thumbs/...` 映射到 `data/cache/thumbs/...`。
- Rust API 默认只绑定 `127.0.0.1`，除非你明确要直接暴露。
- 不要在无认证、无 TLS 的情况下把 photoSelf 暴露到公网。

---

## 低功耗设备上的 HEIC / HEVC

很多 OpenWrt 的 ffmpeg 没有软件 HEVC/HEIF 解码器。如果库里有 HEIC（可能被命名为
`.JPG`）或 HEVC MOV，请在 PC/NAS 上生成 sidecar：

```bash
./scripts/make-sidecars.py \
  --source /mnt/router/picture \
  --output /mnt/router/usb/photo-app/sidecars/family \
  --sizes 320,1280 --video-sizes 320 --jobs 2
```

```toml
[thumbnails]
sidecar_dir = "/mnt/usb/photo-app/sidecars"
```

sidecar 位于 `<sidecar_dir>/<library-id>/`；更多说明见
[`usage.zh-CN.md`](usage.zh-CN.md) 和脚本注释。

---

## 升级与备份

- SQLite 是缓存 + 用户元数据，备份 `data.dir`。
- 缩略图/缓存可丢弃；sidecar 可重新生成。
- 保留上一版本二进制以便回滚。
- 扫描和浏览不会修改照片库。

## 排错

| 现象 | 检查 |
|---|---|
| API 401 | `auth.mode`、用户名密码、`password_hash` |
| UI 能打开但没有内容 | 库路径、权限、扫描任务状态 |
| HEIC 缩略图 500 | 生成 sidecar，或使用带 HEIF 的 ffmpeg |
| 视频海报 500 | ffmpeg 是否支持该编码 |
| 开机服务退出 | USB 未挂载或 `data.dir` 不存在 |
| `Address already in use` | 修改 `server.listen` |
