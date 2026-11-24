# Pods 原生 Linux Mobile 适配设计

日期：2026-11-24。上游及 fork 审查基线：`349be73c8bc2a3340e2b6187df1c0c676b63be82`。工作分支：`feat/linux-mobile-docker`。

本版按用户新增的严格范围限制修订，替代先前较宽的实施范围。开发前的原始设计已先于源码修改存档，历史内容保留在 Git 提交 `abb066f4a4e5ced9edb0d7e0ee4de9c063d5c4a7`。范围修订已先更新 Google Drive 存档，再执行缩减后的修改。2026-11-24 用户明确批准下述两行启动修复；本次先补存授权边界，再应用补丁。其他未获批准的上游问题继续只作记录。

## In Scope

本任务只适配 Linux Mobile / postmarketOS。每个保留的代码块必须对应下表中的需求；实施时继续复用上游 engine / model / view 分层和 libadwaita 组件。

| 需求标识 | 允许的最小改动 | 具体边界 |
| --- | --- | --- |
| M-LOCAL | 手机本机 Docker socket 发现与访问说明 | 保留已有引擎识别和 API 行为；仅选择本地 socket、区分实际访问错误、用可阅读的弹窗说明权限 |
| M-LAYOUT | 366/360 宽竖屏、651 宽横屏及桌面恢复 | 只改实测拥挤的共享搜索栏、连接帮助弹层和建议列表；保留已有导航、列表、表单和返回栈 |
| M-INPUT | 触屏及软键盘可达性 | 窄屏终端辅助键、44px 点击区域、URL 输入语义、建议列表可收缩；复用既有 exec、粘贴及 GTK/VTE 输入路径 |
| M-NATIVE | aarch64 musl / Alpine 原生安装 | APKBUILD、实际系统依赖、固定源码校验、已有 desktop/schema/resource 的安装、原生两架构构建；无 Flatpak 依赖 |
| M-LOCALE | 本轮新增控件及原生中文资源集成 | 翻译新增移动控件和权限提示；APK 内映射 zh_CN 到已有 zh_Hans catalog，不改旧文案或补齐整个项目翻译 |
| M-MEMORY | 4 GB 手机的直接资源预算 | 限制客户端 I/O worker 与同时 stats 请求数；归档从整包内存缓冲改为磁盘文件传输，保留原有完成、失败、取消及事件语义 |
| M-VERIFY | 适配本身的验证 | ARM64/Alpine 构建、实际修改控件的 GTK/Wayland 尺寸和输入测试、本地 socket/权限检查、所改传输路径验证，以及 Pixel 2 待验记录 |
| M-STARTUP | 用户单独批准的完整启动阻塞修复 | 仅将 images.sort-attribute 的 choice 与 default 从 name 改为 id，共两行；不扩展其他上游修复 |

本轮适配自身引入的回归需要修复，例如窄屏搜索在桌面宽度未恢复、触控栏在 366 宽溢出。这些修复应限定在新增适配代码内。

打包可以声明目标平台实际需要的依赖，例如 GTK 4.22、GLib 2.88、libadwaita 1.9、Rust 1.92 以上，以及 Alpine Docker Top 所需的 procps-ng；不升级上游 Cargo.lock 中的库版本。构建缓存和离线设置尽量放在 APKBUILD，避免扩展上游通用构建框架。

## Out of Scope

禁止修复、重构或改进与本次适配无直接关系的上游行为。发现的问题只写入独立的[上游问题清单](upstream-issues.md)，不把解决这些问题列为移动适配的质量门槛。

| 明确禁止的内容 | 处理方式 |
| --- | --- |
| 端口 host IP 丢失、pull 的镜像 ID、build EOF、stop 状态、百分比算式等既有业务问题 | 恢复上游实现，保留复现与影响说明 |
| 既有连接取消、上传错误/取消、终端 commit 回调或后台流生命周期问题 | 不借资源优化名义修复；保留原有逻辑 |
| 日志重连、解析、ANSI、导出和完整日志窗口重写 | 全部撤回，记录已有问题；本轮不改日志语义 |
| API 版本协商、SDK 补丁、依赖升级、通用最低版本同步 | 不实施；只记录目标平台实际构建要求 |
| 上游发行 workflow、vendor/checksum 修复、通用交叉编译 helper | 恢复上游；原生 APK 使用独立的必要打包路径 |
| 欢迎页旧文案、旧译文、代码风格清理和其他技术债 | 保持原样；只格式化本轮必要修改 |
| 覆盖全项目的业务回归、Registry/search、全量 lint 作为本任务验收门槛 | 从当前补丁和 CI gate 中移除；此前实验作为历史证据保留 |
| Compose、Swarm、独立网络编辑器或额外产品功能 | 不新增 |

上游 Bug 若直接阻止本次适配，必须先说明阻塞原因、影响范围和最小补丁，取得用户明确批准后才能修改。此前已经应用但未按此规则批准的修复先撤回。不会通过测试专用配置、临时包补丁或安装脚本隐藏产品阻塞。

## 运行基线与已有实现

目标来自用户提供的设备规格：Pixel 2 walleye、骁龙 835、4 GB RAM、aarch64 musl、4 KiB 页；postmarketOS / Alpine 3.24；systemd；Plasma Mobile / KWin Wayland，295% 缩放，主要内容尺寸为 366×651 与 651×366，Plasma Keyboard 中文输入。GTK 4.22.4、GLib 2.88.1、libadwaita 1.9.2、Docker 29.9.2 为提供的设备基线，仍需实机核对。

Pods 已有 Docker / Podman 后端、668sp 折叠导航、360 宽窗口下限、窄屏底部动作、Adw.Dialog、PreferencesPage、WrapBox 和终端 resize。保留这些已有实现。优先复用共享组件，不预设所有页面都需要改动。

## 已批准的启动阻塞例外

首次完整启动的真实 GTK 测试发现：镜像排序枚举使用 `id`，GSettings schema 却提供并默认使用 `name`，干净配置下发生 GLib-GIO 致命错误。该问题存在于上游基线，影响完整应用启动，也会阻止 Pixel 2 的端到端 UI 验收。

用户在 2026-11-24 明确回复“批准”，授权应用已展示的两行补丁：在 `data/com.github.marhkb.Pods.gschema.xml.in` 中，仅把 `images.sort-attribute` 的一个 choice 和 default 从 `name` 改为 `id`。授权范围不包含其他 schema、迁移代码、枚举或业务逻辑。已有独立实验表明旧 keyfile 中的 name 会按 GSettings 规则回落为 id；批准后需用完整应用启动与既有导航重新验证。具体复现和补丁原文保留在问题清单与 proposals/startup-schema.patch 中。

## 实施顺序与验收

先审查相对上游基线的完整 diff，给每项修改分配需求标识，再撤销范围外块。文档、测试与 CI 一并缩减，避免测试断言强迫修复上游问题。最后复查变更名单和原有行为保持情况。

| 验收层 | 本轮检查 | 状态边界 |
| --- | --- | --- |
| 原生构建 | Alpine x86_64 / aarch64 musl、APK 依赖、安装产物和源码校验 | 两架构分别记录，旧宽范围候选的结果不替代当前缩减补丁 |
| 变更控件 | 360/366 竖屏、651 横屏、366×300 键盘缩短高度、1024 桌面；搜索和终端辅助键 | 保留已通过的独立控件检查；批准的启动修复另增加完整窗口与既有导航检查 |
| 本地连接 | Docker socket 选择、不存在/文件/未监听/权限拒绝、普通用户组权限 | 不修复其他引擎业务语义 |
| 资源改动 | 同时 stats 请求上限、I/O worker 预算、归档传输内容及有界内存路径 | 不把整项目内存缺陷纳入修复 |
| 完整应用 | 启动、真实导航、已有 Docker 功能在手机可达 | 应用已批准的两行 schema 修复后验证完整启动与既有导航；遇到其他上游阻塞另行说明 |
| Pixel 2 | 295% 缩放、触屏、中文 preedit、键盘避让、旋转、Portal、休眠、GPU、RSS/PSS 与功耗 | 需要实机执行，QEMU 结果单独报告 |

QEMU system 已能在沙箱中启动独立的 ARM64 / x86_64 客体内核，并运行 Unix socket、Docker 和 GTK/Wayland。模拟机用于执行可运行的验证，不替代 Pixel 2 图形驱动、触屏、实际键盘和功耗测量。

## 交付

交付最小适配分支、原生 APKBUILD、逐项需求追踪、独立上游问题清单、当前补丁的验证记录，以及实机验收步骤。此前已推送的较宽候选以明确的后续回退提交收拢，不删除审查历史，不向上游发送 PR 或问题消息。
