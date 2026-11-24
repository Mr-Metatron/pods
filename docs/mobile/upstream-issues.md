# 上游既有问题与独立验证记录

基线为 [Pods 349be73c8bc2a3340e2b6187df1c0c676b63be82](https://github.com/marhkb/pods/tree/349be73c8bc2a3340e2b6187df1c0c676b63be82)。以下问题在移动适配审查中发现。本轮保留上游业务行为；此前试做的通用修复及对应验收断言已撤回。下述完整启动阻塞已于2026-11-24获得用户对两行最小补丁的明确批准；其余条目只记录问题，不扩大开发范围。

证据分为源码确认、受控接口/组件验证、真实 Docker 客体验证。下文早期业务问题的真实客体复现使用 x86_64 Alpine 3.24、Docker 29.9.3；最终适配补丁还完成了 ARM64 客体验证，具体范围见[Docker 验证](testing-docker.md)。Pixel 2 的 Docker 29.9.2 和完整手机会话尚未完成验收。此前含临时修复的测试结果单独标为历史候选。

## 完整启动阻塞：镜像排序默认值与枚举不一致

**状态：用户于2026-11-24明确批准，已仅应用两行补丁；源 C 的 x86_64 和 ARM64 QEMU 实际应用启动与既有导航在 X11 英文、X11 中文和 Wayland 英文三组均通过，两架构原生 APK 构建与安装也通过。** 原问题阻止完整 Window/ClientView 初始化，直接影响手机端到端 UI 验收。授权记录见设计中的 M-STARTUP；具体范围与证据见 [QEMU 验证](testing-qemu.md)。

[上游 schema](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/data/com.github.marhkb.Pods.gschema.xml.in#L151-L161) 的 `images.sort-attribute` choices/default 使用 `name`，而 [ImagesPanel::SortAttribute](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/view/images_panel.rs#L68-L78) 的枚举 nick 为 `id`、`tag`、`containers`。默认 GSettings 绑定因此被 GType 拒绝。

复现条件是使用原始 schema、干净配置（`GSETTINGS_BACKEND=memory`）和有效 GTK 显示会话，创建完整应用窗口。2026-11-24 05:29 UTC 的 QEMU x86_64 GTK 测试实际输出如下，随后进程 `Aborted`：

```text
GLib-GIO-WARNING **: 05:29:15.345: Unable to look up enum nick ‘name’ via GType
GLib-GIO-ERROR **: 05:29:15.348: The schema default value for key 'sort-attribute' in schema 'com.github.marhkb.Pods.view.panels.images' was rejected by the binding mapping function.
```

工作区证据 `build-env/upstream-findings/schema-default-observed.log` 保存了当时输出的原文摘录。最初的循环 boot.log 已被后续测试覆盖，该摘录是保留的观察记录。范围回撤提交 f427564e 中，schema 恢复为上游的 `name`；该阶段只验证独立控件。用户批准后，当前 schema 才应用下述两行修复，随后用实际主窗口与既有导航验证。

已批准补丁只改镜像 schema 中两行，原始可审阅文件为 [startup-schema.patch](proposals/startup-schema.patch)：

```diff
-        <choice value='name'/>
+        <choice value='id'/>
-      <default>'name'</default>
+      <default>'id'</default>
```

其他 schema 的 `name` 保持原值。独立 keyfile 实验在旧设置含 `name` 的情况下读到旧 schema 为 `'name'`、候选 schema 为 `'id'`，记录位于 `build-env/schema-migration/result.log`；因此候选不附加迁移代码。源 C 的测试从安装的 schema 读到 `id` 后直接激活真实应用，没有覆盖设置或添加迁移代码。

## 端口绑定丢失用户填写的地址

源码确认：[Docker 创建选项](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/engine/opts/container_create_opts.rs#L42-L54) 把每个端口映射的 `host_ip` 设为 `None`；[Podman 转换](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/engine/dto/port.rs#L40-L51) 同样忽略该字段。界面和模型已有 IP 输入。

重现输入为一条 `127.0.0.1:HOST_PORT -> CONTAINER_PORT/tcp` 绑定。序列化后的创建请求没有对应 HostIp，因此 daemon 使用自身的默认绑定地址，用户填写的回环限制没有传递。临时修复曾通过捕获请求与真实 Docker inspect 检查 `127.0.0.1`；该验证属于历史候选。两个转换文件当前均恢复基线。

## Docker pull 的摘要与本地镜像 ID

源码确认：[Docker pull](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/engine/api/images.rs) 从 `Digest: ...` 状态取得 `Finished.image_id`；[ImagePullAction::finish](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/model/image_pull_action.rs#L111-L140) 用该值查找以本地镜像 ID 索引的对象。Registry manifest digest 和本地 config ID 可不同，因此动作的镜像对象关联可能无法建立。

受控重现是让 pull 返回一个 manifest digest，再让同镜像的 inspect 返回不同的 config ID。真实隔离 Registry 的一个样本为 manifest `sha256:dcc82e683f94db42f445baf37614322f0ed8d3ee3b65de3d4f0d33dc871c5d36`、本地 ID `sha256:001820cf5f089af3cdd5184521ebe6e25172dbd566f38ec84e082be8cabeb328`。曾对临时修复验证 pull 完成值与 inspect 一致；当前已恢复上游的摘要处理。

## Docker build 的流结束与准备失败

源码确认：[Images::build](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/engine/api/images.rs) 仅在拿到 `aux.id` 后产生 Finished。以 HTTP 200 返回一条普通 `stream` 消息后直接 EOF，会结束流而没有 Finished 或 Error；消费动作缺少相应终态。临时受控 Unix fixture 曾覆盖这一条件，通用修复和断言现已撤回。

另一条准备失败路径位于 [ImageBuildAction](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/model/image_build_action.rs#L75-L82)：`images.build(opts)` 同步报错时只记录日志并返回空流，未设置动作失败。重现条件为不可读或不存在的构建目录。本次内存改动仍同步生成归档并立即返回准备错误，保留该上层处理方式。

## Docker stats 的数值与操作名称

源码确认：[ContainerStats 转换](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/engine/dto/container_stats.rs) 仅匹配小写 `read`/`write`。输入 `Read`/`Write` 的统计会得到零累计；正 CPU 增量配合零系统增量会算出无限值，正内存使用量配合零内存上限也会得到无限百分比。

这些输入曾进入临时单元 fixture，修复目前已撤回。另据 [stats_stream](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/engine/api/containers.rs) 的缓存更新，停止后不再返回的容器可能保留旧 CPU 快照，重新出现时跨停止期取差。当前只增加四个请求的并发上限，数值公式、大小写匹配和缓存行为保持上游。

## Docker stop 事件后的状态

源码确认：[Docker 事件处理](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/model/container_list.rs#L355-L380) 将 `kill` 与 `stop` 都映射为 Stopping，将 `die` 映射为 Exited。[Docker 官方事件示例](https://docs.docker.com/reference/cli/docker/system/events/) 给出的停止序列为 kill、die、stop。按该序列向模型投递后，末尾 stop 会把 Exited 改回 Stopping。

这里的结论来自源码和已公开事件顺序。历史真实 daemon 测试验证过停止/启动 API 与事件接收，未对原始 GTK 模型的最终状态作实机断言。事件映射文件已恢复基线。

## 取消连接与文件复制的异步状态

[ConnectionManager](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/model/connection_manager.rs#L211-L217) 先设置 `creating_new_connection=true`，再对可取消任务的结果使用 `await?`。任务被取消并返回 None 时，会在复位标志前返回。重现条件为连接创建未完成时取消。

[ContainerCopyToAction](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/model/container_copy_to_action.rs) 最终回调检查可取消 future 外层结果的 `is_ok()`。上传返回 `Ok(Err(...))` 时，外层依然成功，动作可能显示 Finished。复制准备的多个异步阶段之间也存在取消已发生、但排队回调继续发起下一阶段的窗口。

[Action::set_state](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/model/action.rs#L73-L84) 已阻止终态再次改变，所以这里的取消问题是后续上传副作用，Cancelled 标签不会被 Finished 覆盖。这些结论来自控制流审查；本轮保留原状态处理，只把 tar 存储改为临时文件。

## 日志与空闲后台流

[src/view/container_log_page.rs](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/view/container_log_page.rs) 的 follow 从最后时间戳取整秒作为 since，并按相同完整时间戳的条目数跳过回放。若同一秒内已有不同纳秒的两条记录，回放覆盖这一秒，而跳过数只统计最后一种时间戳，可能重复显示旧记录。处理单位又是 transport chunk，一个 chunk 含多行时只建立一条时间戳映射。以上为源码确认的边界条件。

日志显示缓冲和向前获取的历史队列没有持续容量上限。GtkSourceBuffer 的独立组件样例还观察到默认启用 undo，删除文本后可以撤销恢复；仅删可见文本不能说明这些存储已经释放。此次容量限制、游标、解析与导出重写全部撤回，未对手机 RSS/PSS 作改善声明。

导出路径打开文件后直接 unwrap，读取或写入失败停止流后，`on_finish` 仍显示成功提示。PlainTextPerform 每个 chunk 新建 parser，且不保留内部换行；ANSI、UTF-8 和多行文本跨 chunk 时需单独测试。此处记录源码条件，不宣称已在 Pixel 2 上重现。

[src/rt.rs](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/rt.rs) 的 Pipe producer 在下一项到达后才通过 send 发现接收者已关闭；如果上游一直空闲，关闭页面后该等待仍可持续。本次保留原流生命周期，仅限制 runtime 的 I/O worker 数量。

## SDK API 版本兼容性观察

锁定的 Bollard 0.21.1 默认 API 版本为 1.53。独立 Unix 请求记录显示，调用临时增加的协商后，客户端状态改变，但实际 target 仍为 `/version`、`/containers/json?all=true&size=false` 等未带版本前缀的路径。该 crate 的 `src/uri.rs` 先拼接版本 URL，再以绝对 path 调用 `url.join(path)`，与观察一致。

Docker 29.9.3 的历史真实客体接受了这些请求。旧 daemon 的兼容下限尚未验证。本轮已撤回 API 协商，不增加 SDK workaround，也不升级锁定依赖。

## Alpine 宿主进程列表依赖

真实 Docker 29.9.3 客体最初只有 BusyBox `ps`。Pods 既有 Top 请求传入 `-o user,pid,ppid,pcpu,etime,tty,time,args`，daemon 返回 HTTP 500，错误包含 `ps: bad -o argument 'pcpu'`。安装 procps-ng 后，同一真实客体的进程列表检查通过。Alpine docker-engine 包当时的直接依赖中没有 procps-ng。

这项按目标平台依赖处理：APKBUILD 声明 procps-ng，保持原 API 和错误语义。曾添加的产品错误重写已经撤回。它不扩大上游业务修复范围。

## 历史候选证据的位置

工作区 `build-env/qemu-x86/results-pass-docker/qemu-x86/docker-live.log` 保留较早 Docker 全操作测试的输出；`build-env/qemu-x86/registry-results/qemu-x86/` 保留真实 Distribution 2.8.3 的匿名与 BasicAuth 推送/拉取记录，两个测试分别用时 3.03 秒与 3.71 秒并通过。它们所测候选含已撤回的通用修复，当前 scoped 验收另行记录。

相关广泛测试脚本已移至工作区 `build-env/archived-*` 调查记录，不再作为仓库 CI 的移动适配门槛。本文件的基线源码链接可独立审阅；工作区日志用于追溯此次调查，未将模拟机结果写成 Pixel 2 实机结果。

## 空的已保存视图触发 GtkStack 警告

源 C 完整启动验证在创建主窗口及独立 ClientView 时观察到 `Gtk-WARNING: Child name '' not found in GtkStack`。源码依据为[上游 schema 的 last-used-view 默认值](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/data/com.github.marhkb.Pods.gschema.xml.in#L24-L28) `''`，以及 [ClientView 中的直接 GSettings 绑定](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/view/client_view.rs#L130-L136)。页面列表没有该名称。

使用干净内存设置时，X11 英文、X11 中文和 Wayland 英文三组均出现此警告，随后实际主窗口、Images 侧栏导航与返回断言全部通过。工作区证据为 `build-env/qemu-x86/startup-verification/boot.log` 及三份独立 GTK 日志。上述 schema key 和 ClientView 绑定与基线相同，本轮未修改它们。

## 双栏断点边界的既有最小宽度

[上游 ClientView](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/view/client_view.blp#L11-L32) 在 `min-width: 668sp` 时展开双栏。本轮按 GTK 当前设置换算为 668 像素，并测试 667 与 668 像素两侧。在精确 668 像素宽度，英文测试输出 `AdwNavigationSplitView ... exceeds AdwBreakpointBin width: requested 700 px, 668 px available`；中文为请求 695 像素。测试使用较长的中英文连接名称，未连接 daemon，Images 面板处于原有加载状态。

断点切换、Images 页面选择与窄屏返回动作均通过。边界截图 `build-env/qemu-x86/startup-verification/mobile-ui-wayland/navigation-668x366.png` 可见右端关闭按钮超出截图范围；366 与 651 像素使用单栏，1024 像素桌面双栏截图显示全部窗口按钮。当前导航测试只断言 ClientView 外框、侧栏返回及页面映射，没有把边界下所有子控件的可见范围纳入通过条件。

`client_view.blp`、`client_view.rs` 和 `images_panel.blp` 对比基线均无改动。这是当前 GTK/libadwaita 环境下观察到的既有边界行为；本轮不调整其断点或面板布局。Pixel 2 的实际窗口装饰与显示缩放仍需实机检查。
