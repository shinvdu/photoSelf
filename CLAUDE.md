# CLAUDE.md（photoSelf 项目工作说明）

本文件给在此仓库中工作的 Claude Code 使用。请优先遵守本文件的约定；如果与临时指令冲突，先和用户确认。

## 项目概览

photoSelf 是一个轻量的自托管照片/视频画廊：

- 后端：Rust workspace，单个可执行文件 `photoself`
- 前端：Vue 3 + Vite（`ui/`），可嵌入二进制（`embed-ui`）
- 存储：直接索引已有目录，SQLite 仅作索引与元数据
- 面向：OpenWrt 路由器、NAS、ARM 小机器（512 MB–2 GB 内存）
- 当前版本：见 `Cargo.toml` 的 `workspace.package.version`（前端版本同步在 `ui/package.json`）

重要文档：

- `README.md` / `README.zh-CN.md`：功能与配置总览
- `docs/usage.md` / `docs/usage.zh-CN.md`：使用说明（功能变化后要同步更新）
- `docs/local-development.md`：本地开发环境搭建与启动
- `docs/design.md`：技术设计（中文）
- `DEPLOYMENT.local.md`：**本机部署细节，已 gitignore，不要提交**

## 仓库结构

```
crates/photoself-core/    # 核心逻辑（不依赖 HTTP）
  config.rs               # 配置结构与校验
  db.rs                   # SQLite schema、迁移、所有查询
  scan.rs                 # 目录扫描与单文件索引
  thumb.rs                # 图片缩略图 / 视频海报 / sidecar
  video.rs                # 转码队列辅助
  util.rs                 # 时间/路径/游标等工具
crates/photoself-server/  # HTTP 层
  main.rs                 # CLI：serve / scan / hash-password
  api.rs                  # 所有 /api/v1 路由与处理函数
  state.rs                # AppState：db、配置、库快照、信号量
  auth.rs                 # none / basic / proxy 认证
  ui.rs                   # SPA / 静态资源服务
ui/                       # Vue 3 单页前端
  src/App.vue             # 几乎全部界面逻辑与模板
  src/styles.css          # 主题变量 + 组件样式
  src/i18n.js, locales/   # 中英文文案
  vite.config.js          # 开发代理、PWA manifest
scripts/                  # dev.sh、make-ipk.sh、make-sidecars.py 等
deploy/                   # OpenWrt/systemd/docker 部署文件
.github/workflows/        # CI 与 Release
```

## 常用命令

```bash
# 后端（本地开发配置见 docs/local-development.md）
PHOTOSELF_CONFIG=config.dev.toml ./scripts/dev.sh     # 后端 + Vite 一键启动
cargo run -p photoself-server -- serve -c config.dev.toml
cargo run -p photoself-server -- scan -c config.dev.toml
cargo run -p photoself-server -- hash-password --password '...'

# 前端
cd ui && npm install
npm run dev        # 127.0.0.1:5173，/api 代理到 127.0.0.1:8787
npm run build      # 产出 ui/dist

# 发布/嵌入 UI 的二进制（必须先有 ui/dist）
cargo build -p photoself-server --release --features embed-ui

# 质量检查（CI 会执行，提交前请全跑）
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

注意：本机 `127.0.0.1:8787` 可能被其它进程占用；本地开发说明与排查见 `docs/local-development.md`。

## 配置与运行时行为

- 主配置：`config.toml`；示例见 `config.example.toml`
- `[[libraries]]`：媒体库种子。**首次导入后 SQLite 的 `libraries` 表为准**；启动时只会补新增 id / 升级旧行（`source IS NULL`），不会覆盖界面里改过的库
- `[manage]`：界面媒体库管理
  - `enabled` 默认 false
  - `allowed_roots` 是安全白名单：界面只能添加这些根目录下的路径，目录选择器也不能越界
  - `require_auth` 默认为 true；`auth.mode = "none"` 时管理接口会 403
- `[write] enabled` + `[trash] enabled`：上传/删除/回收站。库还需 `read_only = false` 才可写
- `auth.mode`：`none` / `basic` / `proxy`；`proxy` 只信任 `server.trusted_proxies`
- 删除媒体库只删索引，**绝不删除源文件**；已删除库会留下 tombstone，重启不会被 config 复活；「从配置重新导入」是显式覆盖操作
- 回收站文件不会自动清理（`retention_days` 目前未启用自动清扫）

## 数据库与迁移约定

- SQLite：WAL、`foreign_keys=ON`；数据库可删除后重建（收藏/标签/界面新增库会丢失）
- `Db::migrate()` 里用 `CREATE TABLE IF NOT EXISTS`，新增列通过 `ensure_library_columns()` 之类的 `ALTER TABLE` 兼容旧库
- `libraries` 表：`enabled` / `ignore_globs` / `source` / `deleted` 等字段
- `items`：缺失文件标记 `missing`，不直接删除；相册计数与封面选择会忽略 `missing` 项
- 改 schema 时必须同时考虑：
  1. 新库建表语句
  2. 旧库迁移
  3. 迁移测试（参考 `db.rs` 中的 `migrates_legacy_libraries_table`）
  4. 扫描/API 的读写路径

## API 约定

- 前缀 `/api/v1`；错误返回 `application/problem+json`
- 所有 POST/PUT/PATCH/DELETE 必须带 `X-Requested-With: photoSelf`（CSRF 防护）
- 媒体库管理接口：`/api/v1/libraries`、`/libraries/{id}`、`/libraries/{id}/scan`、`/libraries/import-config`、`/fs/dirs`
- `GET /api/v1/info` 返回 `manage_enabled`，前端据此显示管理入口
- 单条目操作沿用 `/items/{id}`；批量删除/加入相册目前由前端逐条调用，没有专用批量 API
- 路径校验：绝对路径、canonicalize、白名单、数据/缩略图/sidecar/回收站目录保护、重复/嵌套库拒绝、5 秒超时

## 前端约定

- 单页应用，界面逻辑主要集中在 `ui/src/App.vue`
- **文案必须同时加到** `ui/src/locales/zh-CN.js` 和 `en-US.js`；默认中文
- 主题：`<html data-theme="light|dark">`，CSS 变量在 `:root` 与 `:root[data-theme='dark']`；请使用变量与 `color-mix()`，避免硬编码深色值
- 语言/主题选择存 `localStorage`（`photoself-lang`、`photoself-theme`）
- 响应式：横屏与竖屏都支持；竖屏/窄屏（≤860px）以及**任意竖屏方向**使用底部抽屉、网格顶栏
- 灯箱：缩放/平移、双击/双指、横屏铺满（5px 边距）、竖屏底部信息抽屉、ⓘ 切换信息面板
- 缩略图：右键/长按菜单、多选（Ctrl/Cmd、Shift）、底部批量操作栏；只读库不允许批量删除
- 相册页：新建相册用按钮 + 弹窗；删除按钮悬停显示，触屏常显；删除相册需二次确认
- 版本号显示在页面底部 footer
- PWA：`vite-plugin-pwa` 自动生成；改了 UI 后浏览器可能因 Service Worker 缓存仍显示旧版，验证时先清缓存/刷新

## 测试与 CI

CI（`.github/workflows/ci.yml`）执行：

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cd ui && npm ci && npm run build`

提交前至少本地跑完这四项。新增后端逻辑（尤其路径校验、迁移、API 权限）要补测试；新增界面行为尽量做浏览器验证。

## 发布流程（版本发布 / 升级路由器）

1. 升版本：
   - `Cargo.toml` 的 `workspace.package.version`
   - `Cargo.lock` 中 `photoself-core` / `photoself-server`
   - `ui/package.json` / `ui/package-lock.json`（可用 `npm version x.y.z --no-git-tag-version`）
   - 更新 `CHANGELOG.md`
2. 构建与检查：UI `npm run build`、`cargo fmt/clippy/test`
3. 提交并推送 `main`
4. 打 tag：`git tag vX.Y.Z && git push origin vX.Y.Z`
5. 等待 GitHub Actions `Release` 成功，产出二进制与 `.ipk`
6. 升级 OpenWrt（命令中的 `<router>`、密码等不要写进仓库）：
   ```bash
   gh release download vX.Y.Z --pattern 'photoself_X.Y.Z-1_aarch64_cortex-a53.ipk'
   scp photoself_X.Y.Z-1_aarch64_cortex-a53.ipk root@<router>:/tmp/
   ssh root@<router> '
     opkg install /tmp/photoself-X.Y.Z.ipk
     chmod 755 /usr/bin/photoself /etc/init.d/photoself
     chmod 600 /etc/photoself/config.toml
     /etc/init.d/photoself restart
   '
   ```
7. 验证：`photoself --version`、`/api/v1/info`（version、item_count、write/trash/manage）、一张缩略图、配置是否保留

发布前若用户说「先本地验证 / 先不要发布」，**不要提交 tag、不要部署**，只在本地构建与验证，等用户确认。

## 部署环境注意事项

- OpenWrt 自带的 ffmpeg 往往没有软件 HEVC 解码器，视频海报可能 500；可用 PC 预生成 sidecar（`scripts/make-sidecars.py`）或关闭 `[video] poster_enabled`
- 路由器容量/内存有限：`thumbnails.concurrency = 1`、关闭不必要的转码
- 缓存与 SQLite 放在可写 USB/磁盘，避免写 NAND
- 部署细节（地址、盘符、密码文件路径等）写在 `DEPLOYMENT.local.md`，不要提交到仓库

## 安全与数据保护红线

- 未经明确许可，**不要删除或移动用户的原始照片/视频**
- 「删除」= 移入回收站；「移除媒体库」= 只删索引，保留源文件
- 不要执行格式化磁盘、`rm -rf` 照片目录、直接改库文件等高风险操作
- 不要把密码、token、私钥、个人路径写进可提交文件
- `config.dev.toml`、`DEPLOYMENT.local.md`、`data/`、`ui/dist/`、`target/` 已在 `.gitignore` 中
- 对路由器/生产机的写操作，先说明将执行什么；用户未确认时不要自动发布、升级或删除

## 常见坑

- 改了配置但忘记重启：大多数配置只在启动时读取；媒体库管理 UI 的增删改则立即生效（数据库为准）
- `allowed_roots` 里的路径必须在启动时存在；不存在的条目会被忽略，需重启后再试
- 本地 8787 被占用时 Vite 代理会连到错误服务；先腾端口
- 相册数量与可见条目不一致：确认是否统计了 `missing` 项（应为 0 后不再计入）
- 上传/删除按钮不出现：检查 `[write]`、`[trash]` 与库的 `read_only`
- 界面不更新：清 Service Worker / 强制刷新
- 添加媒体库报 400：检查是否越出 `allowed_roots`、与已有库/数据目录重叠、路径是否可读