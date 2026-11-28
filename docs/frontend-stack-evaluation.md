# photoSelf 前端技术栈取舍评估

> 评估日期：2026-11-28
> 对应版本：photoSelf v0.7.3
> 评估对象：当前实现 vs 早期设计稿中“未采用”的 Pinia / Vue Router / Naive UI / PhotoSwipe / OpenAPI / SSE / session
> 结论性质：阶段性技术判断，不是永久结论
> 执行记录（2026-11-28）：`App.vue` 2,004 → 1,199 行，抽出 `components/` 与 `composables/`；引入 `vue-router` 4（hash）与 URL 深链接；新增 12 个 Playwright 交互用例（12/12 通过）。Pinia / Naive UI / PhotoSwipe / OpenAPI / SSE / session 仍按本文判断暂不引入。

---

## 0. 结论速览

**以当前产品目标（家庭/小团队、低资源、单管理员、局域网、PWA）为标准，当前取舍总体是“好”的；以“长期多人协作 + 通用开源平台”为标准，则存在几处明显欠债。**

最值得优先补的不是换库，而是（2026-11-28 已全部执行）：

1. 拆分过大的单文件前端（`App.vue` / `styles.css`）；
2. 增加 URL 路由与深链接；
3. 给灯箱/缩放/批量操作补浏览器回归测试。

其余“未采用”的库/机制，多数属于**按需采用**，现在引入大概率是负收益。

---

## 1. 评估基准与当前事实

### 1.1 产品约束

- 主要用户：家庭/小团队，单管理员。
- 访问方式：局域网或 WireGuard，Basic Auth / 反向代理认证。
- 资源：可运行在 512 MB–2 GB ARM 设备、OpenWrt 路由器。
- 交付：单二进制 + 嵌入式 SPA + PWA；离线壳可用。
- 规模：当前约 4,300 个媒体项；目标 1 万–10 万项。

### 1.2 当前前端事实（v0.7.3）

| 项目 | 数值 |
|---|---|
| 生产 JS | 约 218 KB（gzip 约 77 KB），引入 Router 与组件化后 |
| 生产 CSS | 约 27 KB（gzip 约 5.5 KB） |
| 运行依赖 | `vue`、`vue-i18n`、`vue-router`（3 项） |
| 主要代码 | `App.vue` 约 1,199 行；`styles.css` 约 1,874 行；`components/` + `composables/` 分担视图与逻辑 |
| 构建 | Vite 6 + `vite-plugin-pwa` |
| 状态/路由 | 无 Pinia；已用 Vue Router（hash）驱动视图/深链接；状态仍以 `ref/computed` 为主 |
| UI/灯箱 | 自定义 CSS 主题与灯箱（无 UI 库、无 PhotoSwipe） |
| API | 手写 REST + 前端 `fetch`；无 OpenAPI |
| 任务进度 | 轮询 `/api/v1/jobs/{id}`；无 SSE |
| 认证 | `none` / `basic` / `proxy`；无 session |

这些事实决定了下面的判断：当前实现是“小而专注”的，而不是“技术落后”。

---

## 2. 逐项评估

| 选择 | 当前收益 | 主要代价/风险 | 判定 |
|---|---|---|---|
| Pinia | 依赖少、启动快；单页状态直观 | 原 `App.vue` 2,004 行，状态与视图耦合；已拆组件/composables 缓解 | **暂不引入**；composables 已落地，继续观察 |
| Vue Router | 深链接、刷新恢复、浏览器前进/后退；hash 模式适配嵌入式静态部署 | 包体略增（当前 gzip 77 KB） | **已补**（hash 模式） |
| Naive UI / Element Plus | 自定义粉紫主题统一，体积小，风格自由 | 表单/表格/无障碍需自维护 | **当前保留自定义**；管理后台复杂化再评估 |
| PhotoSwipe | 灯箱与竖屏抽屉、相册/批量操作深度整合 | 手势/加载/层级/关闭等边界自己承担；已出现过关闭按钮被遮挡类问题 | **保留自定义 + 补测试**；手势 bug 反复出现再考虑 |
| OpenAPI / utoipa | 手写 API + 单前端，零生成链路、编译快 | 无机器可读契约；文档与实现可能漂移 | **暂不引入**；出现第三方客户端/SDK/多团队时再加 |
| SSE | 轮询简单、断线可恢复、代理友好 | 任务多时有延迟与请求开销 | **暂不引入**；实时任务面板需求明确再上 |
| session 登录 | Basic/Proxy 单管理员模型简单，攻击面小 | 无真正退出、无多用户、无分享链接；Basic 每请求带凭据 | **条件采用**；做多用户/分享/角色时必需 |

---

## 3. 分项说明

### 3.1 Pinia：现在不需要，但组件拆分后应重新评估

当前界面只有一个主视图（timeline / albums / album / trash / manage），状态集中在 `App.vue`，用 `ref/computed` 足够直接。引入 Pinia 会：

- 增加一个运行时依赖与一层间接；
- 但不会自动解决 `App.vue` 过大的问题；
- 只有把界面拆成多个组件、跨组件共享灯箱/选择/媒体库状态后，Pinia 的收益才明显。

**建议**：先抽 `useLightbox`、`useSelection`、`useLibraryManager`、`useJobPolling` 等 composable；如果拆分后仍觉得状态边界混乱，再引入 Pinia。

### 3.2 Vue Router：已补齐（hash 模式）

此前没有路由导致以下问题；2026-11-28 已引入 Vue Router 4（`createWebHashHistory`）并完成 URL 状态同步：

- 刷新页面回到时间线；
- 浏览器后退/前进不生效；
- 无法收藏或分享「某个相册 / 某个搜索 / 某个标签」的 URL；
- 未来做分享链接、多用户时缺少基础。

成本不高：Vue Router 体积可控；嵌入式 SPA 建议用 `createWebHashHistory()`，避免反向代理需要额外的 history fallback 配置。

**已实现映射**：

```text
#/timeline?q=&tag=&favorite=
#/albums
#/albums/:id
#/trash
#/libraries
```

已验证：`#/albums`、`#/albums/:id` 刷新后保持视图；浏览器后退回到上一视图；`#/timeline?favorite=true` 直接恢复筛选；未知路由重定向时间线。

### 3.3 Naive UI / Element Plus：自定义是对的，但要补“可维护性”

自定义 CSS 带来了：

- 与产品一致的粉紫主题、圆角、渐变；
- 更小的包体与更少的依赖；
- 对竖屏抽屉、灯箱、批量栏的特殊布局完全可控。

代价是：

- 表单/表格/弹窗/无障碍要自己维护；
- 没有组件库的默认交互规范，容易出现“这个按钮为什么这样”的不一致。

**建议**：保留自定义主题，但可以引入**无样式的 headless 组件**（如 dialog/popover/focus trap）或至少抽出通用 `Button/Modal/Field` 组件；不建议现在换整套 UI 库。

### 3.4 PhotoSwipe：自研灯箱换来了贴合度，也承担了全部边界

自研灯箱支撑了：

- 横屏铺满 5 px 边距 + 右侧信息浮层；
- 竖屏底部信息/操作抽屉；
- 与相册、批量、右键菜单的整合；
- 缩放/平移/双击/双指/键盘。

但灯箱是交互密集区，已经出现过：

- 横屏 viewer 覆盖关闭按钮，导致点击无效；
- 图片加载/布局时序导致的空图/裁切；
- 自动加载与视图切换的时序问题。

**建议**：不急着换 PhotoSwipe；先给灯箱补 Playwright 回归：

- 横屏打开/关闭；
- 竖屏抽屉收起/展开；
- 缩放 1×↔2.5×、缩放后拖拽、切换图片复位；
- 滑动切图、Esc、左右键；
- 关闭按钮在不同层级下可点击。

如果后续手势/动画需求继续增长，再评估 PhotoSwipe 或专门的手势库。

### 3.5 OpenAPI / utoipa：当前是过度设计，未来是接口契约

目前唯一的 API 消费者是自带前端，手写接口 + `fetch` 够用；OpenAPI 会带来：

- Rust proc macro/生成器与构建复杂度；
- 前端类型生成链路；
- 版本与契约维护成本。

只有当出现第三方客户端、移动端 SDK、或其他团队对接时，机器可读契约的收益才会超过成本。

**建议**：目前保留手写 API；先把 API 文档（`docs/usage.zh-CN.md` 的端点表）与实现对齐，必要时再引入 OpenAPI。

### 3.6 SSE：轮询在当前离线规模下更稳

当前扫描/转码任务少，前端 1 秒轮询 `jobs/{id}`：

- 不需要长连接；
- 断线自动恢复；
- 对反向代理和移动网络更友好；
- 实现与调试简单。

SSE 的优势在“任务多、需要实时进度条/日志流”时；当前没有这个需求。

**建议**：保留轮询；如果未来做任务中心、实时日志或大批量导入，再上 SSE/WebSocket。

### 3.7 session 登录：单管理员场景下 Basic 更合适

当前 `none/basic/proxy` 足够：

- 家庭局域网 + WireGuard；
- 单管理员；
- Basic Auth 由浏览器/反向代理处理，服务端无 cookie/session 状态。

但 session 是以下能力的**前置条件**：

- 多用户与角色；
- 真正的“退出登录”；
- 分享链接与过期控制；
- 审计与用户级权限。

**建议**：若产品路线明确要做“多用户/分享”，应尽早设计 session + 用户表 + CSRF token；否则不要为“看起来更现代”而引入。

---

## 4. 当前实现真正的三项欠债

### 4.1 单文件前端过大（已缓解）

原 `App.vue` 2,004 行、`styles.css` 1,874 行，超过“舒适维护”阈值。2026-11-28 已拆出 `AppHeader/AlbumsView/TrashView/LibraryManager/SelectionBar/ContextMenu/AlbumDialog/AlbumPicker` 与 `useTheme/useLightbox/useSelection`；`App.vue` 约 1,199 行。灯箱模板仍在 App（全局覆盖层），继续加交互时仍需关注：

- 改动影响面难判断；
- 测试困难；
- 多人协作冲突；
- 代码审查成本高。

**已落地拆分**：

```text
components/           # 实际已拆
  AppHeader.vue
  AlbumsView.vue
  AlbumDialog.vue
  AlbumPicker.vue
  TrashView.vue
  LibraryManager.vue
  SelectionBar.vue
  ContextMenu.vue
composables/
  useTheme.js
  useLightbox.js
  useSelection.js
src/
  api.js / toast.js / format.js / placeholder.js / router.js
tests/e2e/            # 12 个 Playwright 用例
```

后续可继续把灯箱模板/时间线网格移到组件，目标 300–500 行；目前先保持风险可控。

### 4.2 没有 URL 状态（已补齐）

如 3.2 所述。已通过 Vue Router hash 模式补齐；分享链接/多用户仍需要后端的分享 token/账号体系，但前端基础设施已具备。

### 4.3 交互边界缺少自动化回归（已补）

灯箱、批量操作、上传、相册删除确认等都属于“状态多、时序敏感”的区域。当前依赖人工验证，已经多次出现“本地好了但实际某路径失效”的情况（如横屏关闭按钮、加载更多、上传反馈）。

**已补**：12 个 Playwright 用例覆盖灯箱/缩放/关闭、相册新建/二次删除、多选、右键菜单、路由深链/刷新/后退、收藏筛选、上传 toast、媒体库/回收站直达、主题/竖屏、相册内上传自动加入。本地 `npm run test:e2e` 12/12 通过；尚未接入 GitHub Actions（避免 CI 波动，可在稳定后添加独立 job）。

---

## 5. 决策表

| 事项 | 当前决策 | 触发升级的条件 | 升级成本 |
|---|---|---|---|
| Pinia | 不引入，先用 composables | 组件拆分后跨组件状态仍混乱 | 中 |
| Vue Router | **已补（hash）** | 分享链接需后端支持 | 已完成 |
| UI 组件库 | 保留自定义 | 管理后台出现大量表格/表单 | 中–高 |
| PhotoSwipe | 保留自定义 | 手势/动画 bug 反复，或需要成熟手势生态 | 中 |
| OpenAPI | 不引入 | 第三方客户端/SDK/多团队 | 中–高 |
| SSE | 不引入 | 实时任务中心/大批量作业 | 中 |
| session | 条件采用 | 多用户、分享链接、角色权限 | 高 |
| 组件拆分 | **已完成（App.vue 约 1,199 行）** | 灯箱模板仍可继续拆 | 中 |
| Playwright 回归 | **已完成（12 用例）** | 接入 CI 独立 job | 低–中 |

---

## 6. 推荐路线

### 近期（不改产品行为）

1. ~~抽组件与 composables~~（已完成，App.vue 约 1,199 行）；
2. ~~加 Vue Router（hash）与 URL 状态恢复~~（已完成）；
3. ~~补灯箱/批量/上传的 Playwright 回归~~（已完成 12 用例）；
4. 在 `docs/design.md` 与 README 中记录这些决策。

### 中期（按产品需求）

- 若做多用户/分享：先设计 session、用户表、权限与 CSRF，再考虑 Pinia/OpenAPI；
- 若做管理后台：评估 headless 组件或轻量 UI 库，而不是整套重主题库；
- 若做实时任务中心：评估 SSE；
- 若出现第三方集成：生成 OpenAPI 并冻结 `/api/v1` 契约。

### 不建议现在做

- 为了“技术栈看起来现代”而同时引入 Pinia + Router + Naive UI + PhotoSwipe；
- 在没有多用户需求时引入 session；
- 在没有外部客户端时引入 OpenAPI/SSE。

---

## 7. 一句话总结

**当前实现是“针对当前约束的正确解”，不是“技术落后”。** 它的主要问题不在“没用某些库”，而在“单文件过大、没有 URL、交互回归不足”。先把这三件事做好，比换栈更能提升质量。

---

## 相关文档

- 技术设计（as-built）：[`docs/design.md`](design.md)
- 使用说明：[`docs/usage.zh-CN.md`](usage.zh-CN.md)
- 本地开发环境：[`docs/local-development.md`](local-development.md)
- 项目仓库：<https://github.com/shinvdu/photoSelf>