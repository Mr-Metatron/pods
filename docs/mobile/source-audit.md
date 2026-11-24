# Pods 移动适配源码审查

审查基线为上游与用户 fork 的 `349be73c8bc2a3340e2b6187df1c0c676b63be82`，版本 3.3.0。适配范围以修订后的[设计](design.md)为准。与本次适配无关的上游业务问题只在[独立问题清单](upstream-issues.md)记录；当前补丁保留原业务逻辑。

## 架构

Pods 使用 Rust、GTK4、Blueprint 和 libadwaita。`src/engine` 通过统一 API、DTO、Opts 对接 Bollard 0.21 的 Docker 与 podman-api 0.11 的 Podman；`src/model` 维护 GObject 模型；`src/view` 与 `src/widget` 渲染界面。Tokio 执行 I/O，GLib 主循环更新 UI。

上游已经实现 Docker 容器生命周期、创建配置、日志、Inspect、exec/resize、资源和进程、文件复制，以及镜像和卷操作。`Capabilities` 控制 Podman 专有功能。本次复用这些接口，不新增 Docker 业务层，也不修正其原有结果、状态转换或错误语义。

## 已有自适应能力

| 已有实现 | 源码 | 本次决定 |
| --- | --- | --- |
| 折叠导航与返回栈 | `src/view/client_view.blp` | 复用现有组件；668sp展开时实测裁切，按M-LAYOUT局部修正阈值并验证非空/选择状态 |
| 主窗口最低 360 宽 | `src/view/window.blp` | 保留，以360/366检查相关页面与操作子控件 |
| 窄屏列表动作移到底部 | containers/images/pods/volumes panel | 保留，不批量修改列表页 |
| Adw.Dialog、ToolbarView、PreferencesPage | 创建和操作弹窗 | 复用；验证编辑、完成、错误状态和缩短高度，只调整确实失败的局部布局 |
| 自动换行的资源、端口、设备和挂载条目 | WrapBox 等现有组件 | 保留 |
| 终端 resize 和 exec | `src/view/container_terminal.rs` | 复用既有会话和生命周期 |

## 直接适配点

ClientView 的668sp双栏阈值在实测中小于子控件的最小宽，右侧按钮被裁切。只将这一阈值提高到720sp，保留现有 NavigationSplitView、侧栏选择与返回动作；同时检查新阈值两侧、原阈值、手机横竖屏与桌面。

非空长名称夹具显示，镜像标签和卷名会被右侧计数压缩到约100像素，产生多行窄列。镜像行、卷行及容器详情中的卷条目只把文字和计数之间的横排改为上游已使用的 Adw.WrapBox。文字需要时占据整行，计数转到下一行；操作按钮位置、模型、菜单和短内容的并排呈现保持原路径。

创建容器的三页切换原为等宽分配。英文 Integration 的84像素标题区在360/366宽下发生省略；仅关闭 ToggleGroup 的 homogeneous 后，该标题取得106像素并完整显示。中文对照没有省略，三按钮仍可达；不改变页名、绑定或创建选项。

空镜像页在原生稳定横屏截图中把拉取/构建按钮裁成1px横线。其内部BreakpointBin在声明断点后依赖手动最小尺寸，原height-request为1；StatusPage已有的滚动区因此未得到完整按钮高度。新增实际控件断言在651×366复现bin分配1px、child需要44px的失败。最小修正只把该高度改为106，覆盖原竖排两个44px按钮和18px间距；保留原336宽度、500sp方向断点、SizeGroup和操作逻辑，沿既有Scroller检查短屏可达。

共享搜索条将文本、匹配选项、计数和导航横向排列。新增窄屏模式把选项放入 Popover，保留可用输入宽度；宽屏恢复原先直接开关。实际尺寸测试发现新增断点最初被搜索条自然宽度限制，因此只在该组件上增加 `hexpand`，使断点根据可用栏宽切换。

终端增加窄屏触控辅助键，补足软键盘常缺少的 Esc、Tab、Ctrl-C 和历史方向键，并保留已有粘贴入口。新按钮使用既有输入通道，要求点击区域至少 44×44；由新辅助栏引入的 366 宽溢出属于本次必须处理的适配回归。

建议列表原先固定最低 200 高。改为自然高度上限与滚动，使键盘缩短可用空间时能够收缩。连接帮助使用可滚动、可换行的弹层；URL EntryRow 使用 URL 输入语义。

本机 Docker 地址按明确的本地 DOCKER_HOST、存在的运行时 rootless socket、系统 socket 选择。保存的连接保持原地址，远程地址继续从自定义入口使用。连接前的真实 socket 检查保留路径与错误原因，权限说明放在可关闭的持久对话框；GUI 不执行授权命令或修改 socket。系统 Docker 组具有 root 等价控制权限，说明中明确组变化后的重新登录要求。

## 资源预算与保持的行为

Tokio I/O worker 上限为 2，Docker stats 同时请求上限为 4。原百分比计算、缓存逻辑和容器事件转换保持上游实现。

Docker build 和容器文件上传改用用户缓存目录中的匿名临时文件和 ReaderStream，避免完整 tar 与输入大小等量占用应用堆内存。生成的 tar 结构、请求选项与已有完成/失败/取消路径保持不变。Docker build 仍按上游顺序同步准备归档并返回准备错误；本轮不修正相关业务异常状态。Podman 镜像构建继续使用原 SDK。

日志显示、日志重连、ANSI 解析、导出和后台空闲流释放的前期改动全部撤销。对应既有缺陷按用户范围规则记录，不作为本轮修复任务。

## 原生依赖与打包

锁定 crate 的实际要求为 Rust 至少 1.92、GTK 4.22、GLib/GIO 2.88、libadwaita 1.9。Alpine 3.24 提供这些 aarch64 软件包。APKBUILD 明确其实际依赖，上游较低的 Meson 声明和缺少 rust-version 的现状保持不变。

使用系统 GtkSourceView、GTK4 VTE、Adwaita 图标、Dconf、桌面 schemas、CA 证书与 SSH 客户端。Alpine 默认 docker-engine 没有 procps-ng，而 Pods 的既有进程查询要求其 ps 的 CPU 百分比字段，因此原生包声明 procps-ng。中文包只把已有 zh_Hans catalog 映射到标准 zh_CN locale，不改旧翻译内容。

文件选择由 `src/utils.rs` 的既有 ashpd OpenFileRequest/SaveFileRequest 直接调用桌面portal。原生包显式声明 `xdg-desktop-portal` 前端；用户规格已提供1.21.2及KDE6.6.6后端。保留上游文件选择实现和桌面自身的后端，实际KDE文件选择与取消留给设备验收。

复用上游 Meson 安装二进制、desktop、图标、GSettings、GResource 和 metainfo。Cargo 缓存、离线准备和编译并发放在 APKBUILD；Meson 仅增加锁定 Cargo.lock 的参数。两架构采用原生编译，不增加通用交叉编译 helper。原发行 workflow 和 vendor 脚本保持上游状态。

## 验证边界

完整系统 QEMU 已证明沙箱能够运行独立客体内核及 Unix socket。较早宽范围候选还完成过真实 Docker 和 Registry 流程；这些结果属于已经撤回部分代码的历史候选，不能作为当前补丁验收结论。

源C/r1的测试覆盖已改共享控件、socket、请求并发、归档传输，以及用户按M-STARTUP批准后的完整启动。其导航只用加载态列表，ARM64正式程序使用真实空列表，尚未覆盖非空列表、详情和大部分操作表单。纠正轮补充这些UI状态，检查实际子控件边界、焦点和滚动可达性；依据失败证据决定最小修改。原生构建、安装和QEMU的准确版本、范围及制品哈希见[验证报告](verification.md)。Pixel 2的GPU、触屏、中文preedit、Plasma Keyboard协议、295%缩放、Portal、休眠、RSS/PSS和功耗仍需实机记录。

## 来源

- [上游固定基线](https://github.com/marhkb/pods/tree/349be73c8bc2a3340e2b6187df1c0c676b63be82)与用户设备规格。
- [Docker Linux 权限说明](https://docs.docker.com/engine/install/linux-postinstall/)、[Engine API](https://docs.docker.com/reference/api/engine/)。
- [Alpine 3.24 aarch64 main](https://dl-cdn.alpinelinux.org/alpine/v3.24/main/aarch64/)、[community](https://dl-cdn.alpinelinux.org/alpine/v3.24/community/aarch64/)的实际软件包。
- 上游反馈 [#891](https://github.com/marhkb/pods/issues/891)、[#740](https://github.com/marhkb/pods/issues/740)、[#915](https://github.com/marhkb/pods/issues/915)用于定位连接可读性、本机入口和 KDE 集成场景；不据此扩大修复范围。
