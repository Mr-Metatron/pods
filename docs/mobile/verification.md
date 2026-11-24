# 范围收拢阶段验证记录（源 A）

日期：2026-11-24。产品源码固定为 [f427564e41347ae8bbe4a3a7f8e01eccadc18aa0](https://github.com/Mr-Metatron/pods/commit/f427564e41347ae8bbe4a3a7f8e01eccadc18aa0)，上游比较基线为 `349be73c8bc2a3340e2b6187df1c0c676b63be82`。本报告和固定源码的 APKBUILD 放在后续文档与打包入口提交中；产品代码以该固定提交为准。

下列记录对应批准启动修复之前的源 A。用户随后已明确批准[两行 schema 补丁](proposals/startup-schema.patch)，当前分支已应用，并增加真实主窗口启动与既有导航验证。包含修复的新候选采用 APK pkgrel=1，使其可以从源 A 的 r0 升级；最终结果将在匹配新源码的构建与测试完成后更新。

## 范围审计

[设计](design.md)已在 Google Drive 更新并读回，包含 In Scope、Out of Scope 和直接阻塞问题的审批规则。[变更追踪](change-scope.md)说明每组保留改动的移动需求，[scope-map.json](scope-map.json)覆盖所有保留的代码、构建与测试文件。

已逐字节确认 15 个撤回文件与上游基线相同；其余混合文件按代码块恢复原业务行为。Cargo.lock 中 335 个包的名称、版本、来源和 checksum 均未改变。此前的通用业务修复、日志重写、发行流程修复、旧文案补全和广泛业务测试已撤回；问题仅保留在[独立清单](upstream-issues.md)。

## 已执行的验证

完整系统 QEMU 使用 TCG 和独立客体内核，运行环境为 Alpine 3.24.4。x86_64 GUI 客体分配 2 个虚拟 CPU、2 GiB RAM，测试以普通 UID 1000 执行。以下结果属于收拢后的代码。

| 项目 | 环境与范围 | 结果 |
| --- | --- | --- |
| 本地连接及传输契约 | x86_64、Linux 6.18.60；真实 Unix socket、引擎识别、错误区分、统计并发、Docker/Podman 上传内容 | 12 passed，0 failed，0 ignored；包含九个本轮测试与三个原有基础测试 |
| 实际权限拒绝 | 普通用户连接权限为零的自建 socket | 保留 PermissionDenied；包含在上述 12 项中 |
| Docker 本机连接 | 真实 Docker 29.9.3；ping、info、容器/镜像/卷列表 | 1 passed；只读，未创建或修改资源 |
| 英文 GTK 控件 | x86_64、GdkX11Display、Xvfb | 1 项组合场景通过，23.36 秒 |
| 中文 GTK 控件 | 同一二进制、LANG=zh_CN.UTF-8、LANGUAGE 未设置 | 1 项组合场景通过，21.43 秒；已有 catalog 在 zh_CN 会话正确加载 |
| Wayland GTK 控件 | 同一二进制、GdkWaylandDisplay；嵌套 Weston、虚拟键鼠 seat | 1 项组合场景通过，21.43 秒；客体整轮退出码为零 |
| ARM64 客体基础能力 | aarch64 Alpine 官方内核与 rootfs、QEMU virt | 启动、Unix socket 绑定及发送通过；未据此声明 ARM64 Pods 应用验收完成 |
| 修改范围内的静态检查 | 变更 Rust 的格式、Blueprint、脚本语法、补丁空白及未应用建议补丁检查 | 通过；未新增全项目质量门槛 |

控件场景覆盖搜索的窄屏选项与桌面恢复、终端辅助栏及五组输入字节、建议选择与高度收缩、连接表单和可滚动帮助。检查宽度包括 360、366、651 和 1024 像素，并包含 366×300 的受限高度。终端六个按钮的点击区域均至少为 44×44；新增栏的最小宽度为 338。触屏粘贴、真实 exec 会话和中文 preedit 仍按设备验收表执行。

Wayland 的测试客户端明确为 GdkWaylandDisplay。Weston 的 X11 backend 运行在客体 Xvfb 中，使用 XTest 为 Weston 提供键鼠输入；应用通过 Wayland 连接该 compositor。早期 headless Weston 没有 seat，无法展示需输入事件的弹层，已将可复现输入 helper 纳入仓库。产品 Popover 行为保持不变。尺寸检查包含实际内容边框与 padding，单独记录 Wayland 的透明阴影尺寸。

最终 GUI 客体完成显示及辅助进程清理、文件同步、只读重新挂载和关机；没有此前调试轮次的晚到磁盘 I/O 错误。测试直接构造本轮改动的控件，使用原始 `images.sort-attribute='name'` schema，未激活完整主窗口。

## 构建与原生 APK

源提交对应的 [Native Alpine CI](https://github.com/Mr-Metatron/pods/actions/runs/45342088782) 已启动 aarch64 与 x86_64 原生构建。报告编写时仍在运行，尚未把该轮 APK 构建或安装记为通过。较早候选的 ARM64 APK 和测试包含现已撤回的改动，只作为历史调查材料。

固定的 [APKBUILD](../../packaging/alpine/APKBUILD) 使用上述源码提交的 GitHub 归档及实际 SHA-512，不依赖 Flatpak。设置 abuild 与签名密钥后，使用方式为：

```sh
cd packaging/alpine
abuild -r
```

已完整下载 1,275,183 字节的固定源码归档，核对 gzip CRC、唯一顶层目录，以及全部 546 个文件与源提交的 Git blob ID。Alpine `abuild -F -d verify unpack` 通过，解包目录与 recipe 的 `builddir` 一致；该步骤仅验证来源与解包，未执行包编译。归档 SHA-512 为：

```text
4987f48784c2489c2a9ca276c152114e96c540af621bea520761407df646536f9c6ae5b1149af6b16ae1499a1c54c8c43c422ee19a8a9f4a7e3d9eb16b37aedb
```

完整环境与签名说明见[原生打包说明](../../packaging/alpine/README.md)。

## 证据与复现

复现命令分别位于 [Docker 验证](testing-docker.md)和 [QEMU 控件验证](testing-qemu.md)。此次工作区原始输出如下：

| 记录 | 工作区相对路径 |
| --- | --- |
| 当前后端整轮串口 | `build-env/qemu-x86/scoped-docker-boot.log` |
| 契约与只读 Docker 输出 | `build-env/qemu-x86/scoped-docker-results/scoped-docker/` |
| 当前 GUI 三组串口 | `build-env/qemu-x86/scoped-verification/boot.log` |
| 当前 GUI 日志、81 张截图与哈希 | `build-env/qemu-x86/scoped-verification/` |
| ARM64 内核与 socket | `build-env/qemu-arm64/boot.log` |

测试二进制 SHA-256：

```text
pods-ui         0b9315882c3c747f5a2d2ac50c58600aca225637f5840b8ef1e37fdb42e80ec5
engine_contract e769d0aa3cb756fb486244814109cd41d4c0468ffca56b38799d29d73fb55260
engine_live     e3df4b904f7af7e3c2655e34f4f0e0e88847d13e22eaf5d2bd7d51ed05799fa1
```

## Pixel 2 待验项目

Pixel 2 的 295% 缩放、Adreno/Freedreno、Plasma Keyboard 中文输入、触屏、旋转、Portal、休眠恢复、RSS/PSS 与电量需要实机验证，具体步骤见[设备验收表](device-acceptance.md)。

本轮仅对 worker 数、统计并发和归档存储路径声明实现及测试结果；没有手机峰值内存或续航改善的测量结论。上游既有缺陷按用户要求继续保留，不作为本任务的修复或质量验收门槛。
