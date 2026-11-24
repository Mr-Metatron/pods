# Linux Mobile 适配验证记录

日期：2026-11-24。产品源码固定为 [20ed3a2f6ced56710d8c3588c238b61e66d90f99](https://github.com/Mr-Metatron/pods/commit/20ed3a2f6ced56710d8c3588c238b61e66d90f99)（下称源 C），上游比较基线为 `349be73c8bc2a3340e2b6187df1c0c676b63be82`。本报告与固定源码的 APKBUILD 放在后续文档提交中；该提交不再修改产品或测试代码。

源 C 包含用户明确批准的[两行启动 schema 修复](proposals/startup-schema.patch)，原生包版本为 `3.3.0_git20261124-r1`。两架构原生构建与安装均已成功，x86_64 和 ARM64 完整系统 QEMU 的英文、中文、Wayland 三组完整主窗口及导航检查全部通过。ARM64 正式安装程序还连接了真实隔离 Docker，显示实际加载完成的空容器列表并正常退出。

## 范围审计

[设计](design.md)已在 Google Drive 存档并读回，包含 In Scope、Out of Scope，以及 M-STARTUP 的明确批准边界。[变更追踪](change-scope.md)与 [scope-map.json](scope-map.json)将保留的代码、构建和测试文件映射到移动需求。

范围收拢后，14 个撤回文件与上游基线逐字节相同；schema 的 choice/default 从 `name` 改为 `id` 是单独批准的两行例外。Cargo.lock 中 335 个包的名称、版本、来源和 checksum 保持不变。其他通用业务修复、日志重写、发行流程修改、旧文案补全与广泛业务测试已撤回，发现的问题保留在[独立清单](upstream-issues.md)。

原有 668sp 折叠导航、360 宽窗口下限、列表底部动作、Adw.Dialog、WrapBox、容器操作和 exec 会话继续使用上游实现。新改动集中在本地 socket 与可读错误提示、窄屏搜索、连接帮助、终端辅助键、建议列表高度、原生打包和直接资源预算。

## 完整启动、导航与移动控件

x86_64 完整系统 QEMU 使用 TCG、独立 Linux 6.18.60 客体内核和 Alpine 3.24.4，分配 2 个虚拟 CPU、2 GiB RAM。测试以 UID 1000、GTK 4.22.4、libadwaita 1.9.2、VTE 0.84.0 运行，启用 `G_DEBUG=fatal-criticals`。三组使用相同的源 C 测试二进制及安装资源。

| 后端与语言 | 结果 | 用时 | 截图 |
| --- | --- | --- | --- |
| X11 / Xvfb，`C.UTF-8` | 1 项组合场景通过 | 38.64 秒 | 38 张 |
| X11 / Xvfb，`zh_CN.UTF-8`，未设置 LANGUAGE | 1 项组合场景通过 | 35.20 秒 | 38 张 |
| GdkWaylandDisplay / nested Weston，虚拟键鼠 seat | 1 项组合场景通过 | 35.01 秒 | 38 张 |

测试读取实际安装的 `images.sort-attribute='id'`，使用全新配置激活真实应用主窗口，检查 366×651、651×366、366×300、360×651、1024×768。随后构造原有 ClientView，通过既有侧栏信号进入 Images，再通过既有返回动作回到窄屏侧栏。导航 fixture 使用隔离 client 模型和临时 Unix listener，Images 保持原有 loading 状态；真实 daemon 连接另行测试。

受影响控件覆盖窄屏搜索选项与桌面恢复、终端辅助栏及五组输入字节、建议选择和高度收缩、连接表单及可滚动帮助。终端六个按钮的点击区域均至少 44×44，新增栏最小宽度为 338。366×300 用于验证可用高度缩短后的布局，实际软键盘和中文 preedit 按实机表执行。

三组都验证了原有导航断点下方及边界的折叠状态、选中页面、映射状态与返回路径。原有 split view 在 668 像素边界要求 695/700 像素宽，截图显示右端关闭按钮超出范围；该上游问题已独立记录，未把边界所有控件可见性记为通过。手机 366/651 宽的折叠布局及 1024 宽桌面布局通过。

Wayland 客户端实际使用 GdkWaylandDisplay；Weston 的 X11 backend 在客体 Xvfb 中运行，XTest 事件为 Weston 提供输入 seat。帮助弹层按实际 GTK 内容边框与 padding 测量，透明阴影单独记录。产品 Popover 行为保持原样。中文进程的 gettext 断言通过；上游未翻译的欢迎文案继续显示英文。

整轮输出 `QEMU_UI_TEST_EXIT=0`，显示进程清理、文件同步、只读重新挂载及关机完成，无晚到文件系统 I/O 错误。114 张 PNG 均成功解码，代表性手机、桌面、受限高度及弹层截图已查看。命令、尺寸和证据细节见 [QEMU 验证](testing-qemu.md)。

## 原生 APK 与 ARM64

源 C 的 [Native Alpine CI](https://github.com/Mr-Metatron/pods/actions/runs/45345743799) 在原生 x86_64 与 aarch64 runner 上均为 completed/success；构建、安装、GTK 验证和制品上传步骤均通过。x86_64 job 的真实 Docker 连接检查也通过，aarch64 job 按设计跳过 runner daemon 检查，其真实 Docker 检查另在 ARM64 完整系统 QEMU 执行。

最终制品为 `pods-alpine-aarch64`（artifact `14899187208`）和 `pods-alpine-x86_64`（artifact `14898843991`），对应源码均为 C。两份 ZIP 的 API digest、ZIP 完整性、两个 APK 与三个测试二进制的内部 SHA-256 均核对一致；正式二进制及 harness 的 ELF 架构和 musl 解释器正确。此前源 A 的 APK 或包含已撤回修改的更早候选不用于最终交付。

| 原生构建 | APK 安装与元数据检查 | 普通用户契约测试 | 英文完整启动/导航/控件 | 中文完整启动/导航/控件 |
| --- | --- | --- | --- | --- |
| aarch64 | 通过 | 12 passed | 1 passed，17.86 秒 | 1 passed，17.74 秒 |
| x86_64 | 通过 | 12 passed | 1 passed，18.14 秒 | 1 passed，18.04 秒 |

每架构保留 76 张截图。构建及 GTK 测试用户为 UID 1000；x86_64 的独立 Docker smoke 用户为 UID 1001，仅在测试容器内获得 socket 组访问权。该 smoke 对真实 Docker 28.0.4 通过，读取到 1 个容器、7 个镜像和 0 个卷，全程只读。原生测试运行于 musl 1.2.6、GTK 4.22.4、GLib 2.88.4、libadwaita 1.9.2 和 VTE 0.84.0。

ARM64 安装包的准确校验值如下；其正式 ELF 使用 `/lib/ld-musl-aarch64.so.1`。

| 文件 | 字节数 | SHA-256 |
| --- | --- | --- |
| `pods-3.3.0_git20261124-r1.apk` | 4,402,910 | `9657f58789e7f649316867e24ad3e49011e313bbef54d932a738ab4d66e1bd99` |
| `pods-lang-3.3.0_git20261124-r1.apk` | 148,034 | `b69de7919d969e3b4783c22d2d98c84b9e24805c5be46661870be768d360e718` |

随包公钥为 `builder-6b053817.rsa.pub`，文件 SHA-256 为 `9b7b08a3cdb33f9a6d6f38cfe790c0106c6a7c8720d322e9b73b187c3218d429`。其 SPKI DER SHA-256 指纹为 `e00d5d6116b7cc8febecdf5a5e9ea55742e2f4051d029feebe5af10b468521a8`。每次 CI job 的签名密钥独立，安装时使用该次、该架构的匹配公钥。

ARM64 完整系统 QEMU 使用这两个 r1 APK 和同制品中的三个测试二进制。客体内完成签名安装、GLib schema 与图标触发器，实际安装版本为 `3.3.0_git20261124-r1`，从安装的 schema 读到排序默认值 `'id'`。Linux 为 6.18.60-0-virt，页大小实际为 4096 字节。

| ARM64 完整系统客体检查 | 结果 |
| --- | --- |
| 普通 UID 1000 的 Unix socket / 传输契约 | 12 passed，0 failed，0 ignored，0.47 秒；含实际 PermissionDenied |
| 真实 Docker 29.9.3 的只读连接 | 1 passed，0.29 秒；容器、镜像、卷均为零 |
| X11 英文完整启动、导航与控件 | 1 passed，37.70 秒，38 张截图 |
| X11 中文完整启动、导航与控件 | 1 passed，34.16 秒，38 张截图 |
| GdkWaylandDisplay 完整启动、导航与控件 | 1 passed，33.14 秒，38 张截图 |
| 正式 `/usr/bin/pods` 连接隔离 daemon | 已加载真实空列表，桌面和竖屏截图人工复核通过，原 app.quit 正常退出 |

ARM64 整轮 `QEMU_ARM64_SOURCE_C_GUEST_EXIT=0`，同步、只读重新挂载后正常关机。第一轮安装已成功，但 scratch 脚本误把 `apk info -v` 的描述输出当作版本，导致版本断言退出。首次日志保存在 `candidate-C-attempt1-fixture-version-query/`；只修正该测试脚本的版本读取和离线查询后，使用干净磁盘与相同 C 制品重跑。产品、APK 和仓库测试代码没有变化。

正式程序验证使用普通 UID 1000、独立 XDG 配置及 keyfile backend，只预设合法的本地连接 UUID。安装的排序默认值在运行前后均为 `'id'`。截图显示连接名 `QEMU local Docker`、Docker `v29.9.3` 与 `No Containers Available`，没有加载指示器或连接错误。程序前后读取的三类资源清单均为空。桌面实际 X11 外窗为 1024×768；竖屏请求 366×651 后实际外窗为 370×651，图注保留实测值。精确的 360/366 内容视口由上述 GTK harness 单独验证。

这项正式程序检查的后端为 X11 / Xvfb，Wayland 来自独立 harness。正式程序 SHA-256 为 `44105efa4230c92b2be42a19600766252314f81d7f9d76e5a2f696cbd0560af7`，与 APK 解出的二进制相同。它验证保存的本地连接、真实列表加载及窄屏呈现，没有执行容器或镜像写操作。

[APKBUILD](../../packaging/alpine/APKBUILD)已固定到源 C 的 GitHub 归档和实际 SHA-512。归档完整下载，大小 1,279,912 字节；gzip/tar 完整，548 个源文件的 Git blob ID 全部与源 C 一致。Alpine `abuild -F -d verify unpack` 已通过，解包目录匹配 recipe 的 `builddir`。SHA-512 为：

```text
88a571a743f5353ee4e56c23e6738099b2dfa3682b114a3b2c34fc5d9781953c06a7d96556e52b20a8e427fb6fdc1b7497af3d6341f8a870d71b723505fbd181
```

配置 abuild 与签名密钥后可直接构建：

```sh
cd packaging/alpine
abuild -r
```

包使用系统 GTK、libadwaita、GtkSourceView、VTE 和图标资源，安装 desktop、GSettings、GResource 与中文 catalog，不依赖 Flatpak。`pods-lang` 提供 zh_CN 到上游 zh_Hans 的应用级映射；procps-ng 满足原有本地 Docker 进程列表的 ps 参数要求。完整依赖、编译内存设置和签名流程见[打包说明](../../packaging/alpine/README.md)。

## 本机连接与权限

源 C 保留范围收拢后的 engine 实现，只增加已批准 schema 及对应启动测试。当前范围的契约测试覆盖真实 Unix socket、引擎识别、访问错误、Docker stats 最多四个并发请求，以及 Docker/Podman 接收的上传 tar 内容；共有 12 项，包含九个适配测试与三个既有基础测试。

真实权限测试由普通用户连接权限为零的自建 socket，保留 `PermissionDenied` 和原始错误来源；两架构原生及 ARM64 客体均通过。真实 Docker smoke 仅执行识别、ping、info 和容器/镜像/卷列表读取。完整记录和复现命令见 [Docker 验证](testing-docker.md)。

GUI 继续以普通用户运行。应用显示实际 socket 和持久错误说明，不执行授权命令。系统 Docker 的组授权由管理员管理，加入相应组后需重新登录桌面；rootless 服务保留用户自己的运行时 socket。

## 证据与历史边界

| 记录 | 工作区相对路径 |
| --- | --- |
| 源 C x86_64 GUI 串口、三组日志、114 张截图及资源哈希 | `build-env/qemu-x86/startup-verification/` |
| 源 C 原生 ARM64 APK、签名公钥、日志、76 张截图及 ELF | `build-env/native-scoped-C-arm/` |
| 源 C 原生 x86_64 APK、日志、76 张截图及 Docker smoke | `build-env/native-scoped-C-x86/` |
| 源 C ARM64 客体安装、12 项契约、Docker、三组 GUI 与正式程序证据 | `build-env/qemu-arm64/candidate-C-evidence/` |
| 首次 ARM64 版本查询 fixture 失败的独立记录 | `build-env/qemu-arm64/candidate-C-attempt1-fixture-version-query/` |
| 源 C 固定归档校验与解包 | `build-env/pinned-source/20ed3a2f6ced56710d8c3588c238b61e66d90f99-verification.json` 及同目录日志 |
| 源 A x86_64 后端与只读 Docker | `build-env/qemu-x86/scoped-docker-results/scoped-docker/` |
| 源 A x86_64 独立控件三组记录 | `build-env/qemu-x86/scoped-verification/` |
| 源 A ARM64 环境预检 | `build-env/qemu-arm64/preflight-A-evidence/` |

源 C 本地 x86_64 GTK 测试二进制 SHA-256 为：

```text
7bf8bc18fc560e02fb7822232b59628a52ac52239db098ed0af74d145727d492
```

源 A `f427564e41347ae8bbe4a3a7f8e01eccadc18aa0` 的 [Native Alpine CI](https://github.com/Mr-Metatron/pods/actions/runs/45342088782)两架构已通过，QEMU 后端在 x86_64 与 ARM64 均通过 12 项契约和真实 Docker 29.9.3 的只读 smoke。源 A 使用原 schema，GUI 仅测试独立控件，未完成完整应用启动。A 的原生包版本为 r0；本次交付使用包含已批准启动修复的 C / r1。

更早宽范围候选的容器生命周期、Registry 和全业务结果属于历史调查，不纳入本次最小适配的验收结论。

## Pixel 2 待验项目

Pixel 2 的 295% 缩放、Adreno/Freedreno、Plasma Keyboard 中文输入与候选窗、触屏、旋转、Portal、休眠恢复、实际 Docker 29.9.2 工作负载、RSS/PSS 及电量仍需实机验证。具体操作见[设备验收表](device-acceptance.md)。

本轮对 I/O worker 上限 2、stats 并发上限 4、归档匿名磁盘文件及流式传输记录实现与测试结果。尚未测量手机峰值内存或续航；日志窗口和其他既有资源问题继续保留在独立清单，不扩大本次修复范围。
