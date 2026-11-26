# AI 安装提示词

简体中文 | [English](ai-install-prompt.md)

把下面的提示词复制给编码 Agent（Claude Code、Cursor 等），让它在 Linux 主机、
NAS 或 OpenWrt 设备上安装 photoSelf。

Agent 应该**先询问**、保持照片库只读，并在汇报前完成验证。

```text
你要安装 photoSelf，这是一个轻量、自托管的照片/视频画廊：
https://github.com/shinvdu/photoSelf

先阅读 docs/installation.zh-CN.md 和 docs/usage.zh-CN.md，按文档执行，
不要自行编造路径或命令。

改动前先问我：
1. 安装目标：源码构建、Docker，还是 OpenWrt/ARM 交叉编译。
2. 我的照片库绝对路径（可以多个）。这些目录必须保持只读；绝对不要移动、
   重命名、修改权限或删除里面的任何文件。
3. 可写的数据目录（SQLite、缩略图、临时文件）。
4. 网络：局域网绑定地址/端口，还是由反向代理（Nginx/Caddy/lighttpd）
   终止 TLS 和认证。
5. 认证方式："basic"（单管理员，推荐）、"proxy"（可信反向代理请求头），
   或 "none"（仅可信局域网）。basic 模式下向我要密码，并用下面的命令生成
   Argon2id 哈希：
   photoself hash-password --password '...'
6. 库里是否有 HEIC 或 HEVC MOV。如果有，说明低功耗设备可能需要在 PC/NAS 上
   用 scripts/make-sidecars.py 生成 sidecar 预览。
7. 是否允许安装系统软件包（Rust、Node、ffmpeg、zig、libheif-examples）。
   安装前必须问我。

然后：
- 覆盖已有配置前先备份。
- 构建前端和二进制，或按我选择的方式使用 Docker。
- 使用仓库提供的 systemd/procd/Docker 配置安装服务。
- 启动服务并验证：
  * curl http://127.0.0.1:8787/healthz 返回 {"status":"ok"}
  * 页面在选定 URL 可以打开
  * 首次扫描任务完成（报告索引/错误/缺失数量）
  * 成功获取一张缩略图和一个媒体 Range 请求
- 汇报：安装路径、服务名、URL、数据目录、库 ID、认证用户名（不要输出密码）、
  密码哈希位置、扫描结果和所有警告。

安全规则：
- 没有 TLS 和认证时，绝不要把应用暴露到公网。
- 不要执行破坏性命令，不要递归删除任何东西。
- 扫描、预览生成和测试过程中不要修改照片文件。
- 照片库尽量只读挂载。
- 任何步骤失败都要展示完整错误并停止，不要猜测。
```
