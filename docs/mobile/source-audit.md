# Pods 移动适配源码审查

审查基线为上游 `349be73c8bc2a3340e2b6187df1c0c676b63be82`，版本 3.3.0，审查日期为 2026-11-24。目标设备环境以用户提供的 Wave16 规格为准：Pixel 2 walleye、4 GB 内存、aarch64 musl、Alpine 3.24 / postmarketOS、Plasma Mobile Wayland，主要测试尺寸为 366×651 与 651×366 逻辑像素。

**现有架构已经适合这次移植。** Pods 已有 Docker 后端、libadwaita 折叠导航和自适应弹窗；需要集中补齐本机连接、密集搜索栏、终端辅助输入、资源上限和原生打包。保留原有 model / engine / view 分层，可以同时维持桌面与 Podman 功能。

本报告说明源码发现和由此确定的改动范围。运行结果记录在验证报告中，设备触控、输入法、休眠和功耗留给 Pixel 2 实机验收。开发前需求与验收标准见[设计方案](design.md)。

## 架构和 Docker 功能范围

`src/engine` 使用统一的 API、DTO 和 Opts 门面，分别调用 Bollard 0.21 的 Docker 接口与 podman-api 0.11 的 Podman 接口。`src/model` 维护 GObject 数据模型和列表；`src/view` 与 `src/widget` 使用 Rust、Blueprint、GTK4 和 libadwaita。Tokio 处理异步 I/O，GLib 主循环更新界面。

`Capabilities` 已根据后端控制功能。Docker 连接不会创建 Podman 的 PodList，也不会展示 Kubernetes YAML、手动 Podman health check 等专有操作。保留这些判断能避免在手机入口上出现无法执行的动作。

| 已有 Docker 能力 | 主要源码 | 审查结果 |
| --- | --- | --- |
| 容器列表、创建、启动、停止、暂停、继续、重启、删除、改名 | `engine/api/{container,containers}.rs` | 已实现，重点验证手机操作可达、状态事件与错误处理 |
| 环境变量、端口发布、bind mount、命名卷、内存限制、重启策略 | `engine/opts/container_create_opts.rs` | 已有 Docker 配置转换，适合通过协议测试与真实容器核对 |
| Inspect、日志、exec 终端、resize、进程和资源统计 | `engine/api/container.rs`、`api/exec.rs`、`dto/container_stats.rs` | 已实现，终端输入和持续数据的资源占用需要补强 |
| 文件复制与容器提交为镜像 | `engine/api/container.rs`、`model/container_copy_to_action.rs` | 已实现；上传 tar 完整驻留内存，需改为有界传输 |
| 镜像拉取、构建、推送、标签、历史、Inspect、删除和清理 | `engine/api/{image,images}.rs` | 已实现；构建上下文完整 tar 驻留内存 |
| 卷的列举、创建、Inspect、删除和清理 | `engine/api/{volume,volumes}.rs` | 已实现 |
| 事件与列表更新 | `engine/mod.rs`、`model/*_list.rs` | 已实现，需要覆盖 stop 等最终事件的状态转换 |

本次功能验收覆盖上表及其移动操作。独立网络编辑器、Compose 与 Swarm 在基线中没有相应 API、model 和 view，作为后续产品扩展记录。

## 现有自适应布局应继续复用

| 已有设计 | 源码证据 | 适配决定 |
| --- | --- | --- |
| 668sp 断点、NavigationSplitView 折叠与返回栈 | `view/client_view.blp` | 保留手机单栏与桌面双栏 |
| 主窗口最低 360×180 | `view/window.blp` | 在 366 宽及键盘缩短后的高度运行测量 |
| 窄屏列表动作移到底部 | `view/{containers,images,pods,volumes}_panel.rs` | 复用已有创建、排序、清理和多选入口 |
| Adw.Dialog、ToolbarView 和可滚动 PreferencesPage | 各 `*_opts_dialog.blp` | 按具体运行问题修改，保留现有表单和提交动作 |
| 资源信息、端口、设备、挂载条目自动换行 | `container_resources.blp`、`port_binding_creation_row.blp`、`device_row.blp`、`mount_row.blp` | 保留 WrapBox 与现有断点 |
| 日志和 Inspect 正文换行、缩放、搜索开关 | `container_log_page.blp`、`scalable_text_view_page.blp` | 保留正文和可触控搜索退出入口 |
| 终端按行列变化 resize，离开页面时结束 exec | `container_terminal.rs`、`container_terminal_page.rs` | 保留旋转和侧栏折叠时的会话生命周期 |

基线已经修复过侧栏折叠导致终端 exec 中断的问题；后续输入辅助条应复用同一 VTE 和 exec 通道，避免改变这一生命周期。

## 有源码依据的界面修改

共享 `text_search_entry` 自行计算文本、三个匹配选项和结果计数的横向宽度，外层还附有上一项、下一项按钮。基线没有为剩余输入宽度设置非负下限，计数也可能比预估宽。这里应采用窄屏选项 Popover，宽屏保留直接开关，并在真实 GTK 分配中检查输入框、匹配导航和全部搜索选项。

终端页面原有菜单和键盘快捷键适合桌面，手机软键盘通常缺少 Esc、Tab、Ctrl-C 和历史方向键。增加仅在窄屏显示的辅助键栏，复用 VTE 粘贴与既有 exec 输入，点击辅助键后保留终端焦点。旋转和键盘出现应只触发 resize。

镜像建议列表原先设置 200px 最低高度。改为自然高度上限和可滚动内容，使它能适应软键盘出现后的剩余高度。连接帮助中的长命令也需要换行和可滚动弹层；自定义连接地址使用 URL 输入语义。

这些问题集中在共享组件及少数入口。其余页面以运行测试结果决定是否扩大修改范围。

## 本机 Docker 连接与权限

基线的本机 Docker 地址固定为 `unix:///var/run/docker.sock`，连接页默认选中 Podman。增加本地 socket 发现后，明确的 `DOCKER_HOST=unix:///...`、存在的运行时 rootless socket 和系统 socket 都能有可解释的优先顺序。已保存的连接继续使用其原 URL；远程地址从自定义连接入口选择。

连接失败需要保留具体路径及可读错误。基线仅以短暂 toast 展示底层错误，手机阅读和处理不便。按实际 I/O 错误区分缺失 socket、权限拒绝和 daemon 未监听，展示可关闭的持久说明。普通用户只需要对所选 socket 拥有访问权；GUI 不执行提权或更改权限。系统 Docker 的组权限具有 root 等价控制能力，管理员授权及完整注销后重新登录应写在对应设置步骤。

帮助需要同时区分 systemd 和 OpenRC。用户提供的候选镜像采用 systemd，传统 Alpine 则可能采用 OpenRC；两套服务命令分别标明适用环境，避免把启用 `docker.socket` 当作共同前提。

基线建立 Bollard 连接后直接请求版本，加入 API 协商可使客户端适配服务器支持的版本范围，并保持已有 Docker/Podman 识别和能力判断。协议测试需要检查实际发出的版本路径，真实 daemon 测试再覆盖服务端行为。

## 内存和后台任务

| 基线中的增长或开销来源 | 适配方向 | 需要观测的结果 |
| --- | --- | --- |
| 日志 TextBuffer、时间戳及历史预取队列持续累积 | 设置显示和预取的行数、字节上限；完整日志保持流式导出 | 持续输出达到上限后，保留数据量稳定 |
| Docker 对全部运行容器同时请求 stats | 设置并发上限，清除停止容器的计数缓存，处理零分母 | 多容器时请求并发有界，CPU/内存百分比保持有限数 |
| Docker copy/build 将完整 tar 放入内存 | 后台流式生成或临时文件传输，并处理取消清理 | 大目录不会按 tar 大小占用同等应用堆内存 |
| 已关闭消费者仍可能等待空闲流 | 在消费者关闭时取消生产任务 | 离开页面后连接与任务及时释放 |
| Tokio 默认按 CPU 数创建 I/O worker | 针对小型 GUI 客户端设置合理上限 | 不影响容器线程，减少应用自身线程开销 |

原有流桥接已经使用容量为 10 的 bounded channel；应继续利用这一机制。容器 Inspect 的写层大小计算与客户端周期性刷新会产生额外 I/O，改变这些语义前需要单独测量。

Pixel 2 的硬件 GLES 由 Freedreno 提供，规格中的 Vulkan 则通过 CPU Lavapipe 执行。实机应比较 GTK 默认路径与 `GSK_RENDERER=opengl`，确认实际使用的渲染器。通用桌面入口保留正常渲染选择。

## 依赖和原生打包

锁定 crate 的声明显示，Pods 当前实际需要 Rust 至少 1.92；`gtk4` 的 `gnome_50` feature 启用 GTK 4.22 和 GIO 2.88，`libadwaita` 的 `v1_9` feature 要求 libadwaita 1.9。Meson 与 README 原先声明的较低版本需要同步。

| 组件 | 实际要求 | Alpine 3.24 aarch64 已查到的软件包 |
| --- | --- | --- |
| Rust / Cargo | ≥1.92 | 1.96.1 |
| GTK4 | ≥4.22 | `gtk4.0-dev` 4.22.4 |
| GLib / GIO | ≥2.88 | 目标规格为 2.88.1；本次 x86_64 构建环境为 2.88.4 |
| libadwaita | ≥1.9 | `libadwaita-dev` 1.9.2 |
| GtkSourceView | `gtksourceview-5` | `gtksourceview5-dev` 5.20.0 |
| VTE | GTK4 接口 ≥0.70 | `vte3-dev` / `vte3-gtk4` 0.84.0 |
| Blueprint | 当前模板所需的编译器 | `blueprint-compiler` 0.20.4 |

本机应用代码没有发现固定 x86 SIMD 或固定页大小的假设。使用 Alpine 原生工具链和动态系统库可保持 musl 路径；最终 aarch64 包仍需检查 ELF 架构、`/lib/ld-musl-aarch64.so.1` 和共享库依赖。

Cargo 构建应共享发行版准备阶段使用的缓存、遵守 Cargo.lock，并提供离线构建选项。原先的 Clippy PATH 写死 Flatpak SDK，另开 target 目录，也需要改为继承环境和共享缓存。发行 vendor 脚本原先只筛 GNU targets，且删除 gettext tar 后仅移除旧版本 checksum 键；完整保留锁定依赖和 crate 内容可覆盖 musl 并维持校验一致性。

对 Meson Cargo 调用的一项初步判断已通过最小实验校正：**在 Alpine 的 Meson 1.11.1 / Ninja 上，基线 command 数组中的 `&& cp` 会正常作为 shell 命令运行。** 构建 helper 的改动依据是自动识别 Cargo 实际产物路径、支持指定 target 和目录外构建时的项目 Cargo 配置，并验证构建失败时不复制产物。

原生 APKBUILD 复用既有 Meson 安装：二进制、desktop、图标、GSettings schema、三份 GResource、metainfo 和翻译都已有规则。打包附带 Adwaita 图标、桌面 schemas、Dconf、CA 证书和 SSH 客户端，维持在 Plasma 中的现有功能。构建默认限制并发并采用 thin LTO；Docker daemon、用户组和 socket 权限由系统管理。

包源使用固定 Git 提交的快照和真实 SHA-512，依赖在 prepare 阶段取得，build/check 使用离线模式。操作方法见[原生 APKBUILD 说明](../../packaging/alpine/README.md)。

## 验收证据如何组织

源码覆盖、协议 mock、真实 Docker、实际 GTK 尺寸和 Pixel 2 分别记录。桌面端至少回归 1024×768，手机模拟尺寸包括 366×651、651×366、366×300 与 360 最低宽度；英文、中文、长名、长路径及错误弹层都进入运行检查。

真实 Docker 使用隔离 daemon 或明确命名的测试资源，覆盖容器生命周期、日志、终端输入和 resize、端口与卷、copy、镜像 pull/build/commit/tag，以及事件回写。清理全部资源的 prune 仅在隔离 daemon 中运行。

Pixel 2 需执行应用抽屉启动、295% 缩放、触屏滚动与长按、中文 preedit、软键盘避让、旋转保留 exec、文件 Portal、休眠恢复和组权限重新登录。性能记录需同时给出容器数量、日志速率、持续时间、RSS/PSS 与 CPU，才能比较改动效果。

## 主要来源

- [Pods 审查基线源码](https://github.com/marhkb/pods/tree/349be73c8bc2a3340e2b6187df1c0c676b63be82)，以及用户提供的 Wave16 应用适配规格。
- [Docker API 参考](https://docs.docker.com/reference/api/engine/)、[Docker Linux 权限设置](https://docs.docker.com/engine/install/linux-postinstall/)、[Docker events](https://docs.docker.com/reference/cli/docker/system/events/)。
- [Alpine 3.24 aarch64 community 索引](https://dl-cdn.alpinelinux.org/alpine/v3.24/community/aarch64/)、[Cargo 包](https://pkgs.alpinelinux.org/package/v3.24/main/aarch64/cargo)、[Alpine Rust/Meson 包参考](https://github.com/alpinelinux/aports/blob/3.24-stable/community/amberol/APKBUILD)。
- [Cargo vendor](https://doc.rust-lang.org/cargo/commands/cargo-vendor.html)、[Rust aarch64 musl target](https://doc.rust-lang.org/rustc/platform-support/aarch64-unknown-linux-musl.html)、[GTK 运行环境](https://docs.gtk.org/gtk4/running.html)。
- 上游反馈与修复：[连接错误可读性 #891](https://github.com/marhkb/pods/issues/891)、[本机默认连接 #740](https://github.com/marhkb/pods/issues/740)、[KDE 图标 #915](https://github.com/marhkb/pods/issues/915)、[统计休眠恢复 #980](https://github.com/marhkb/pods/pull/980)。这些条目用于定位回归场景，具体修改依照当前源码和运行结果。
