# 本机 Docker 与资源开销验证

这些测试验证本次移动适配的连接、权限、请求并发、上传内存路径和终端辅助输入。它们调用 Pods 的实际 Rust engine 代码。原生窗口与Wayland验证见[QEMU记录](testing-qemu.md)，实际触屏和键盘验收见[设备验收表](device-acceptance.md)；上游已有业务问题见[独立问题清单](upstream-issues.md)。

## Unix socket 契约测试

安装原生构建依赖并生成 Meson 配置后，在仓库根目录运行：

```sh
cargo test --locked --offline --target-dir _build/src --test engine_contract
```

测试使用临时 Unix HTTP 服务，保留 SDK 的实际传输路径。范围如下：

| 检查 | 验证内容 |
| --- | --- |
| 引擎识别 | 原有 Docker/Podman 识别与能力开关保持可用；未知引擎返回错误 |
| 本地发现 | 明确的本地 DOCKER_HOST、运行时 rootless socket、系统 socket 的选择顺序 |
| 访问诊断 | 路径不存在、普通文件、正常监听 socket、已停止服务的 socket |
| Docker stats | 七个容器的快照全部返回，同时处理的请求最多四个 |
| 文件上传 | Docker 与 Podman 收到的 PUT tar 内容与输入文件逐字节相同 |

测试不要求改变上游的百分比计算、事件状态、镜像 ID 或失败处理。上传 fixture 只接收请求，不运行容器。宿主禁止 `socket(AF_UNIX)` 时，这些测试会明确失败；请在允许 Unix socket 的原生 Linux 或完整系统 QEMU 客体中运行，并单独记录宿主限制。

真实文件权限拒绝需要普通用户执行，且该进程没有绕过文件权限的 capability：

```sh
cargo test --locked --offline --target-dir _build/src --test engine_contract \
  denied_socket_retains_permission_denied_errno -- --ignored --nocapture
```

这个测试仅把自建临时 socket 的权限设为零，检查实际 `PermissionDenied` 错误及其来源，然后恢复权限清理。Docker socket 和用户组保持不变。也可在普通用户环境使用 `-- --include-ignored --test-threads=1` 一次执行全部 scoped contract。

## 真实 Docker 的只读连接检查

明确选择手机或测试机上的 Unix socket，然后以实际桌面用户运行：

```sh
PODS_TEST_DOCKER_HOST=unix:///var/run/docker.sock \
cargo test --locked --offline --target-dir _build/src --test engine_live \
  docker_local_connection_smoke -- --exact --ignored --nocapture
```

rootless 服务可把环境变量设为 `unix://$XDG_RUNTIME_DIR/docker.sock`。测试不会隐式选择 socket；它在三十秒内验证引擎识别、ping、info、容器列表、镜像列表和卷列表，输出 daemon 架构、版本、内核与对象数量。测试只读取状态，不拉取镜像、不创建或修改资源、不运行 prune，也不需要 Registry 凭据。

运行用户需要原有 socket 的访问权限。连接失败时保留错误全文，并在 UI 中检查对应的持久提示。系统 Docker 的组权限具有 root 等价控制能力；组变化后需重新登录桌面，Pods 继续以普通用户运行。

Alpine 的现有进程列表功能还需要宿主 `procps-ng` 的 `ps`，原生 APK 已声明该依赖。这个只读连接测试不调用进程列表；安装与完整应用功能验收按设备验收表记录。

## 隔离客体中的真实终端

`tests/qemu/guest-terminal-tests.sh` 在没有网络接口的完整系统客体内启动专用 Docker daemon，离线导入匹配架构的 Alpine 根文件系统，再创建一个专用容器。它只使用 `/run/pods-ui-terminal.sock`，不会选择宿主或用户现有 daemon。测试结束时移除自建资源并停止 daemon。具体准备与选择命令见[QEMU运行说明](testing-qemu.md#connected-terminal-check)。

移动纠正轮的x86_64开发构建在Docker29.9.3、普通UID1000下通过X11与原生Wayland两组检查，分别用时18.01秒和17.29秒。专用socket权限为 `0660 root:1000`。这些结果使用第4次开发构建的冻结源码快照；最终固定源码与ARM64制品结果另见[总报告](verification.md)。

测试经真实VTE commit把命令发送到容器shell，验证触控粘贴按钮和选区复制；五个辅助键在容器raw模式下由 `od` 核对精确字节，Ctrl+C另验证终止前台sleep。五种视口下，容器 `stty size` 与VTE实际行列一致：360×651为30×39、366×651为30×40、651×366为14×72、366×300为10×40、1024×768为40×113。两个后端各保留7张截图和实际终端文本，证据位于 `build-env/qemu-x86/mobile-ui-live4/`。

这项检查覆盖现有exec连接和本次触屏辅助入口。原有terminal commit与会话退出逻辑保持不变；真实手机的中文preedit、软键盘显示/隐藏和触屏事件交付仍需实机验证。

## 源 C / r1 的历史结果

该轮产品源码为 `20ed3a2f6ced56710d8c3588c238b61e66d90f99`，原生包版本 `3.3.0_git20261124-r1`。[Native Alpine CI](https://github.com/Mr-Metatron/pods/actions/runs/45345743799) 的原生 x86_64 与 aarch64 构建、安装和普通用户契约测试全部通过，每架构 12 passed。x86_64 的真实 Docker 28.0.4 只读 smoke 通过，读取到 1 个容器、7 个镜像、0 个卷；没有修改这些资源。

同一 aarch64 制品随后在独立 ARM64 完整系统 QEMU 中安装和运行，客体使用 Linux 6.18.60-0-virt、4096 字节页。UID 1000 的契约测试为 12 passed（0.47 秒），包括实际权限拒绝；真实隔离 Docker 29.9.3 的只读 smoke 为 1 passed（0.29 秒），读取到三类资源均为零。两个 harness 的 SHA-256 为：

```text
engine_contract 43e16bab5773a9fb191be3a16396847b40eefbefe6b1e67e19af39f28c2ec071
engine_live     2be6401bb562c42768d5ce77a48648db6ade7c945b7bd0f1fa57852cf20aa808
```

正式安装的 `/usr/bin/pods` 还在该客体中以 UID 1000 启动，加载预先保存的合法本地测试连接，显示 Docker v29.9.3 与实际空容器列表。桌面与竖屏截图已人工复核，原 app.quit 正常退出，运行前后容器、镜像、卷清单仍为空。此项使用 X11 / Xvfb；Wayland 的独立控件和导航验证见 [QEMU 记录](testing-qemu.md)。未执行容器生命周期、镜像拉取或 Registry 写操作。

原生证据位于工作区 `build-env/native-scoped-C-arm/` 与 `build-env/native-scoped-C-x86/`；ARM64 客体的串口、独立日志、正式程序截图与哈希位于 `build-env/qemu-arm64/candidate-C-evidence/`。客体所有阶段退出码为零并正常关机。首轮 scratch 包版本查询断言的误判与修正记录见[总报告](verification.md)，没有修改产品来通过检查。

## 源 A 的客体记录

2026-11-24，源 A `f427564e41347ae8bbe4a3a7f8e01eccadc18aa0` 的两个测试目标在完整系统 QEMU x86_64、Alpine 3.24、内核 6.18.60-0-virt 中，以 UID/GID 1000 的 tester 运行：

| 本轮检查 | 结果 |
| --- | --- |
| `engine_contract --include-ignored --test-threads=1` | 12 passed，0 failed，0 ignored；含九个本轮连接/资源测试与三个既有逻辑测试 |
| 实际 Unix socket 权限拒绝 | 通过，包含在上行；普通用户连接权限为零的自建 socket 得到 PermissionDenied |
| `docker_local_connection_smoke --exact --ignored` | 1 passed；真实 Docker 29.9.3；读取到 0 个容器、0 个镜像、0 个卷 |

完整串口记录为工作区 `build-env/qemu-x86/scoped-docker-boot.log`，独立结果文件位于 `build-env/qemu-x86/scoped-docker-results/scoped-docker/`。两个测试退出码均为零；客体正常同步、只读重新挂载并关机。二进制 SHA-256 分别为 `e769d0aa3cb756fb486244814109cd41d4c0468ffca56b38799d29d73fb55260`（engine_contract）和 `e3df4b904f7af7e3c2655e34f4f0e0e88847d13e22eaf5d2bd7d51ed05799fa1`（engine_live）。

源 A 的原生 aarch64 测试制品也在独立 ARM64 完整系统 QEMU 中通过相同的 12 项契约与 Docker 29.9.3 只读 smoke。记录在 `build-env/qemu-arm64/preflight-A-evidence/`；该轮只用于检查 ARM64 内核、普通用户权限和 daemon 环境，没有安装 A 的 APK 或测试完整 GUI。

A 至 C 的 `src/engine`、`src/model` 和 `src/rt.rs` 没有差异；r1交付使用 C 的匹配原生制品重新验证；当前纠正轮结果见[总报告](verification.md)。Pixel 2 实机结果单独记录。

## 结果记录

保存测试提交、完整输出、执行用户、架构、libc、GTK/libadwaita 与 Docker 版本。契约测试说明上述传输和权限路径的行为；只读 smoke 说明指定 daemon 的连接与读访问。Pixel 2 上的完整操作、触控、中文 preedit、295% 缩放、休眠恢复和 RSS/PSS 仍需实机记录。

较早宽范围候选曾运行容器生命周期、镜像构建、双向复制和真实 Registry 测试。其中包含已经撤回的上游 bug 修复，因此这批历史输出只作为问题调查证据；当前补丁使用这里列出的 scoped 测试重新验收。
