# 上游既有问题与独立验证记录

基线为 [Pods 349be73c8bc2a3340e2b6187df1c0c676b63be82](https://github.com/marhkb/pods/tree/349be73c8bc2a3340e2b6187df1c0c676b63be82)。以下问题在移动适配审查中发现。本轮保留上游业务行为；此前试做的通用修复及对应验收断言已撤回。除下述完整启动阻塞外，其余条目只记录问题，不扩大本轮审批或开发范围。

证据分为源码确认、受控接口/组件验证、真实 Docker 客体验证。真实客体使用 x86_64 Alpine 3.24、Docker 29.9.3；Pixel 2 的 Docker 29.9.2 和完整手机会话尚未完成验收。此前含临时修复的测试结果标为历史候选，当前补丁的结果另见[Docker 验证](testing-docker.md)。

## 完整启动阻塞：镜像排序默认值与枚举不一致

**状态：等待用户单独批准；建议补丁未应用。** 该项阻止完整 Window/ClientView 初始化，直接影响后续手机端到端 UI 验收。

[上游 schema](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/data/com.github.marhkb.Pods.gschema.xml.in#L151-L161) 的 `images.sort-attribute` choices/default 使用 `name`，而 [ImagesPanel::SortAttribute](https://github.com/marhkb/pods/blob/349be73c8bc2a3340e2b6187df1c0c676b63be82/src/view/images_panel.rs#L68-L78) 的枚举 nick 为 `id`、`tag`、`containers`。默认 GSettings 绑定因此被 GType 拒绝。

复现条件是使用原始 schema、干净配置（`GSETTINGS_BACKEND=memory`）和有效 GTK 显示会话，创建完整应用窗口。2026-11-24 05:29 UTC 的 QEMU x86_64 GTK 测试实际输出如下，随后进程 `Aborted`：

```text
GLib-GIO-WARNING **: 05:29:15.345: Unable to look up enum nick ‘name’ via GType
GLib-GIO-ERROR **: 05:29:15.348: The schema default value for key 'sort-attribute' in schema 'com.github.marhkb.Pods.view.panels.images' was rejected by the binding mapping function.
```

工作区证据 `build-env/upstream-findings/schema-default-observed.log` 保存了当时输出的原文摘录。最初的循环 boot.log 已被后续测试覆盖，该摘录是保留的观察记录。当前 schema 已恢复上游的 `name`；scoped UI 测试读取该值并仅构造本次改动的独立控件，不覆盖设置来绕过完整启动失败。

最小候选只改镜像 schema 中两行，完整可审阅文件为 [startup-schema.patch](proposals/startup-schema.patch)：

```diff
-        <choice value='name'/>
+        <choice value='id'/>
-      <default>'name'</default>
+      <default>'id'</default>
```

其他 schema 的 `name` 保持原值。独立 keyfile 实验在旧设置含 `name` 的情况下读到旧 schema 为 `'name'`、候选 schema 为 `'id'`，记录位于 `build-env/schema-migration/result.log`；因此候选不附加迁移代码。应用这两行仍需用户授权，之后重新执行完整启动与导航验收。

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
