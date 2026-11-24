# Pods 原生 Linux Mobile 适配设计

日期：2026-11-24。上游与 fork 基线：`349be73c8bc2a3340e2b6187df1c0c676b63be82`（2026-10-22）。工作分支：`feat/linux-mobile-docker`。

本设计在产品源码修改前编写并存档到 Google Drive。后续测试发现的必要调整记录在验证报告中，保留本设计作为开发前基线。

## 目标与运行基线

将现有 Pods 适配为可从应用抽屉启动的原生 Linux 手机容器管理器。以普通用户管理手机本机 Docker Engine，继续支持桌面 Linux 与现有 Podman 功能，提供不依赖 Flatpak 的 Alpine APKBUILD。

目标环境来自用户提供的 `wave16-app-spec.md.md`：Pixel 2 walleye，骁龙 835、4 GB RAM；Wave15 r73 候选镜像，postmarketOS v26.06 / Alpine 3.24.3，aarch64 musl，4 KiB 页；systemd 261；Plasma Mobile / KWin 6.6.6，Wayland 1.25；GTK 4.22.4、GLib 2.88.1、libadwaita 1.9.2；Docker 29.9.2。桌面缩放 295%，主要窗口尺寸为 366×651 和 651×366 逻辑像素。键盘是 Plasma Keyboard，包含中文拼音输入。

设备规格是本次验收输入，未连接设备的构建环境不能验证这些实机状态。构建、模拟尺寸、真实 Docker、真机验证分别记录证据。

## 源码与架构结论

现有分层为 Rust / GTK GObject model、Blueprint / libadwaita view、`engine` API 与 DTO/Opts 门面。`Engine::Docker` 使用 Bollard 0.21，`Engine::Podman` 使用 podman-api 0.11。异步 I/O 在 Tokio，界面更新在 GLib 主循环。保留这些边界，避免另建移动客户端或替换 GUI 框架。

Docker 已有容器列表、创建、生命周期、Inspect、日志、终端 exec/resize、进程、统计、文件复制、镜像拉取/构建/推送/标签、卷及事件处理。`Capabilities` 已控制 Docker 下不可用的 Podman 专有功能。本次完整支持的范围为 Pods 已有 Docker 管理能力及其手机端可达性；独立网络编辑器、Compose 和 Swarm 属于额外产品功能，不纳入本次适配。

| 已有自适应实现 | 源码位置 | 处理决定 |
| --- | --- | --- |
| 668sp 断点与 NavigationSplitView 折叠 | `src/view/client_view.blp` | 保留窄屏单栏、返回栈和宽屏双栏 |
| 主窗口最低 360×180 | `src/view/window.blp` | 在目标 366 宽运行验证，按测试发现调整 |
| 列表动作随折叠移到底栏 | `src/view/{containers,images,pods,volumes}_panel.rs` | 保留现有触控导航与操作入口 |
| Adw.Dialog、ToolbarView、PreferencesPage | 创建、复制、清理、提交等 opts_dialog | 复用自适应弹窗和滚动表单，逐项测试，不普改页面 |
| 资源、端口、挂载等 WrapBox | `container_resources.blp`、`port_binding_creation_row.blp` 等 | 保留换行逻辑 |
| 日志/Inspect 正文换行、缩放、搜索退出 | `container_log_page.blp`、`scalable_text_view_page.blp` | 保留正文与现有 HeaderBar 搜索开关 |
| 终端 resize 与导航退出时关闭 exec | `container_terminal.rs`、`container_terminal_page.rs` | 保留旋转/折叠时的会话生命周期 |

## 有依据的改动范围

### 本机 Docker 连接与权限

连接页当前固定 `unix:///var/run/docker.sock`，默认选 Podman。增加可独立测试的本机 socket 发现：显式本地 `DOCKER_HOST=unix:///...`，存在的 `$XDG_RUNTIME_DIR/docker.sock`，系统 Docker socket。远程地址继续使用明确的自定义连接；已有保存连接保持原 URL。只有发现 Docker 而没有 Podman 时自动选 Docker，两者都有时保留原先优先顺序。

使用真实 I/O 错误区分 socket 不存在、权限拒绝、daemon 未监听等情况，显示可读且可保留的错误说明。权限提示包含实际路径、管理员分配访问权限、组变更后完整注销/登录。Pods 保持普通用户运行，不执行 sudo、不改变 socket mode、不自动添加用户组。Docker 组对系统 daemon 具有 root 等价控制能力，设置说明在对应权限步骤明确交代。

帮助应同时识别 systemd 与 OpenRC 的适用命令，目标候选镜像使用 systemd。长命令、路径和解释可换行/滚动，复制操作保留完整原文。自定义 URL 输入使用 GTK 的 URL input-purpose。

连接初始化对 API 版本进行协商并以真实 Unix HTTP 请求测试验证；保持 Docker/Podman 识别与能力表。修复连接取消/失败后 creating 状态复位。Docker stop 事件映射到已退出状态，避免最终 stop 事件把容器重新留在 Stopping。

### 窄屏搜索、终端与软键盘

共享搜索组件 `text_search_entry` 目前手工把文本、三个匹配选项、计数横向排放，外层另有上一项/下一项按钮。用 libadwaita 断点在窄屏把选项收入 Popover，宽屏保留原先直达选项；修复负宽分配风险，保留 regex、大小写、整词与匹配导航。

终端窄屏增加 Esc、Tab、Ctrl-C、历史方向键、粘贴等辅助入口，复用现有 VTE 与 exec 输入通道，不启动第二个 shell。按钮不抢走终端输入焦点，旋转和软键盘调整高度时仅发送 resize。宽屏继续使用原有布局，辅助键应有清晰可访问名称。

镜像建议列表目前强制至少 200px 高。改为可收缩的自然高度上限，保留可滚动列表，让软键盘弹出后的剩余空间能够容纳建议项。

输入全部使用 GTK/VTE 原生路径，不额外实现输入法或注入 Wayland 协议。中文 preedit、键盘出现/隐藏和焦点滚动需在 Plasma Keyboard 实机验收。

### 内存和任务生命周期

已确认风险包括日志正文/时间戳与历史预取队列无界增长、Docker stats 无界并发、完整 tar 上传与镜像构建上下文驻留内存、已关闭消费者仍可能等待空闲流。

处理策略为有界资源和及时释放：限制 stats 同时请求数并回收停止容器的缓存，避免零分母产生非有限百分比；关闭消费者时取消底层 stream；日志显示和历史预取设置明确的行数/字节上限，并保留完整日志流式导出和可说明的历史阅读方式；Docker copy/build 使用流或临时文件，取消/完成后释放文件与任务。保留 Podman 接口行为。对用户可见的日志窗口限制必须有清楚提示，并提供完整数据获取入口。

Tokio 的 I/O worker 数以小型客户端需求设置上限，不改变容器运行线程；大文件归档在后台处理。终端保留有限 scrollback。性能判断通过修改前后同场景测量，不把桌面软件渲染结果当作 Adreno 540 性能。

### 原生构建和 APKBUILD

真实依赖要求由锁定 crate 确认：Rust 至少 1.92，GTK 至少 4.22，GLib/GIO 至少 2.88，libadwaita 至少 1.9。现有 Meson 和 README 仍声明 GTK 4.18 / libadwaita 1.7，需要同步。目标 Alpine 3.24 仓库已提供所需 aarch64 包。

修复 Meson Cargo 调用、移除写死 Flatpak 的测试 PATH、使用 Cargo.lock 与离线构建选项、保留原生环境的 Cargo 配置和交叉目标参数。发行 vendor 筛选加入 aarch64/x86_64 musl，验证 gettext vendoring 不破坏 checksum。

APKBUILD 使用原生 `aarch64` / `x86_64` 架构与 Meson 安装，安装现有 desktop、图标、GSettings schema、翻译、GResource 与 metainfo。依赖包括所需 GTK/VTE/GtkSourceView、桌面 schemas 和图标；无需 Podman daemon、Flatpak 或完整 GNOME 桌面。构建任务限制并行度，使用可校验的源码和锁定依赖；生成的包及源码校验记录与具体提交关联。

保留桌面默认渲染选择。Pixel 2 上可测试 `GSK_RENDERER=opengl`，避免该镜像的 Lavapipe CPU Vulkan 路径；任何设备专用环境建议放在设备说明中，不写死到所有桌面用户的入口。

## 验收标准

| 验收层 | 必须检查 | 证据与通过条件 |
| --- | --- | --- |
| 构建与静态检查 | 锁定依赖构建、rustfmt、Clippy、Blueprint 编译/格式、desktop/schema/metainfo | 保存命令、工具版本、退出码；基线既有问题单列 |
| 后端单元/协议 | socket 发现与错误、取消状态、Docker/Podman 能力、版本、stats、端口/卷/配置、流取消与内存上限 | 调用真实实现；mock 明确标为协议测试 |
| 原生 Alpine | musl 编译、Meson 安装、APKBUILD、apk 文件内容与依赖、干净启动 | 实际构建/安装结果；aarch64 构建与 x86_64 构建分别记录 |
| 尺寸与交互 | 366×651、651×366、366×300、360 最小宽、1024×768；英文、中文、长名/路径 | 实际 GTK 测量/截图；关键操作可达、无横向裁剪/负分配、宽屏功能保持 |
| 真实 Docker | 连接、创建/启停/暂停/重启/改名/删除、日志、exec/resize、stats、卷/端口、copy、pull/build/commit/tag/inspect、事件 | 隔离 daemon 和明确命名的测试资源；prune 仅在隔离 daemon 执行 |
| 资源稳定性 | idle RSS/CPU、持续日志、重复打开关闭页面、多个 stats、大文件 copy/build、任务取消 | 保存测量条件/基线/峰值；随输入增长的缓存达到上限后稳定，关闭资源可释放 |
| Pixel 2 真机 | 应用抽屉启动、295%缩放、触控长按/滚动、中文输入、OSK避让、旋转保留exec、Portal、休眠恢复、权限注销重登、耗电/温度 | 需要该设备实际执行并记录；模拟测试不能勾选此层 |

真实 daemon 无法运行或缺少设备时交付可执行验证脚本与明确待验项，报告具体阻碍及已经取得的证据。不得将源码存在、mock 通过或 x86_64 构建通过写为 Pixel 2 实机通过。

## 开发与交付组织

改动按构建/打包、Docker 连接和正确性、移动 UI、资源与测试拆成可审查提交。沿用上游 Rust/Blueprint 格式、翻译机制和 Conventional Commits。新字符串进入 POTFILES/gettext 流程，关键手机提示提供中文覆盖。只在测试证据支持时扩大页面修改范围。

交付包括 fork 分支、原生 APKBUILD 与构建说明、设计存档、功能与依赖审查、验证记录、实机操作清单及可复跑脚本。上游 PR 的最终说明应准确列出已验证范围和残留风险；本次不向上游自动发送消息。

## 来源

- 最新上游源码：[marhkb/pods @ 349be73c](https://github.com/marhkb/pods/tree/349be73c8bc2a3340e2b6187df1c0c676b63be82)。用户 fork：[Mr-Metatron/pods](https://github.com/Mr-Metatron/pods)。
- 用户附件：`wave16-app-spec.md.md`，本设计运行基线。
- Docker socket 权限：[Linux post-installation](https://docs.docker.com/engine/install/linux-postinstall/)。
- Docker API：[Engine API reference](https://docs.docker.com/reference/api/engine/)。
- Docker 事件顺序：[docker system events](https://docs.docker.com/reference/cli/docker/system/events/)。
- 上游真实反馈：[连接错误 toast 不可读 #891](https://github.com/marhkb/pods/issues/891)、[本机连接默认设置 #740](https://github.com/marhkb/pods/issues/740)、[KDE 图标缺失 #915](https://github.com/marhkb/pods/issues/915)、[统计休眠恢复修复 #980](https://github.com/marhkb/pods/pull/980)。这些反馈用于定位测试，最终取舍依据当前源码和复现结果。
- Alpine 原生包：[v3.24 aarch64 main](https://dl-cdn.alpinelinux.org/alpine/v3.24/main/aarch64/)、[community](https://dl-cdn.alpinelinux.org/alpine/v3.24/community/aarch64/)。
- GTK 运行环境：[Running GTK Applications](https://docs.gtk.org/gtk4/running.html)。
