# 本机 Docker 与资源开销验证

这些测试只验证本次移动适配直接修改的连接、权限、请求并发和上传内存路径。它们调用 Pods 的实际 Rust engine 代码。完整窗口、Wayland、触屏和键盘验收见[设备验收表](device-acceptance.md)；上游已有业务问题见[独立问题清单](upstream-issues.md)。

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

## 当前范围的客体实测

2026-11-24，在范围收拢后的工作树上编译两个测试目标，使用完整系统 QEMU x86_64、Alpine 3.24、内核 6.18.60-0-virt，以 UID/GID 1000 的 tester 运行：

| 本轮检查 | 结果 |
| --- | --- |
| `engine_contract --include-ignored --test-threads=1` | 12 passed，0 failed，0 ignored；含九个本轮连接/资源测试与三个既有逻辑测试 |
| 实际 Unix socket 权限拒绝 | 通过，包含在上行；普通用户连接权限为零的自建 socket 得到 PermissionDenied |
| `docker_local_connection_smoke --exact --ignored` | 1 passed；真实 Docker 29.9.3；读取到 0 个容器、0 个镜像、0 个卷 |

完整串口记录为工作区 `build-env/qemu-x86/scoped-docker-boot.log`，独立结果文件位于 `build-env/qemu-x86/scoped-docker-results/scoped-docker/`。两个测试退出码均为零；客体正常同步、只读重新挂载并关机。这批是当前范围的后端验证，ARM64 应用和 Pixel 2 实机结果仍待补充。

## 结果记录

保存测试提交、完整输出、执行用户、架构、libc、GTK/libadwaita 与 Docker 版本。契约测试说明上述传输和权限路径的行为；只读 smoke 说明指定 daemon 的连接与读访问。Pixel 2 上的完整操作、触控、中文 preedit、295% 缩放、休眠恢复和 RSS/PSS 仍需实机记录。

较早宽范围候选曾运行容器生命周期、镜像构建、双向复制和真实 Registry 测试。其中包含已经撤回的上游 bug 修复，因此这批历史输出只作为问题调查证据；当前补丁使用这里列出的 scoped 测试重新验收。
