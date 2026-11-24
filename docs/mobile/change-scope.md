# 最小适配变更追踪

比较基线为 `349be73c8bc2a3340e2b6187df1c0c676b63be82`。本表记录相对基线保留的修改及其移动需求；[scope-map.json](scope-map.json) 给出全部代码、构建与测试文件的对应关系。需求定义与禁止范围见已同步至 Google Drive 的[设计](design.md)。

## 保留的改动

| 需求 | 改动位置 | 保留的具体内容 |
| --- | --- | --- |
| M-LOCAL | `engine/socket.rs`、`engine/mod.rs` | 选择本机 Docker 地址；在既有识别流程前保留 Unix socket 的实际访问错误。引擎 API 与版本处理不变 |
| M-LOCAL | `view/connection_creation_dialog.rs` 与 Blueprint | 动态显示并选择发现的本地 Docker socket；连接名、已有连接和原连接流程保持上游逻辑 |
| M-LOCAL、M-LAYOUT | `view/connection.rs`、`connections_sidebar.rs`、`window.rs`、`view/mod.rs` | 用既有 Adw.AlertDialog 显示可读的持久连接错误；不修改权限、用户组或 daemon |
| M-LAYOUT | `view/client_view.blp` | 双栏展开阈值从668sp提高到720sp；原边界小于实际子控件最小宽，导致右侧按钮裁切。折叠导航、选择与返回逻辑不变 |
| M-LAYOUT | `view/image_row.blp`、`volume_row.blp`、`container_volume_row.blp` | 仅将名称/标签与状态计数的内部横排改为已有的 Adw.WrapBox；长名称在手机上取得完整可用宽，短内容和桌面仍可并排；操作与计数模型不变 |
| M-LAYOUT | `view/container_create_opts_dialog.blp` | 创建容器的三页 ToggleGroup 按标题所需宽度分配空间，取消等宽限制；360/366宽的 Integration 标题完整显示。标题、页面绑定和字段逻辑不变 |
| M-LAYOUT、M-INPUT | `view/images_panel.blp` | 空镜像页按钮区最小高度从1改为106，容纳原竖排两个44px按钮及18px间距；保留500sp方向断点、按钮动作和StatusPage原滚动路径 |
| M-LAYOUT、M-INPUT | 连接对话框 Blueprint | 为帮助内容增加滚动和换行；显示目标平台服务及权限说明；设置 URL 输入语义 |
| M-LAYOUT、M-INPUT | `widget/source_view_search_widget.blp`、`text_search_entry` | 窄栏收起搜索选项至 Popover，宽栏恢复原开关；复用现有搜索属性 |
| M-INPUT | `widget/suggestion_entry_row.blp` | 建议弹层按内容高度收缩并滚动；建议获取、过滤和选择逻辑保持不变 |
| M-INPUT、M-LAYOUT | `view/container_terminal*`、`data/resources/style.css` | 窄屏辅助键及粘贴入口，点击区域至少 44×44；新控件使用原 exec 输入通道，不改终端 commit 或会话结束逻辑 |
| M-MEMORY | `rt.rs`、`engine/api/containers.rs` | I/O worker 上限 2、同时统计请求上限 4；保留原缓存、公式与事件处理 |
| M-MEMORY | `engine/archive.rs`、`engine/api/container.rs`、`engine/api/images.rs`、`model/container_copy_to_action.rs` | 完整 tar 的内存缓冲改为缓存目录中的匿名文件，按流上传；保留同步准备顺序及原失败、取消、完成处理 |
| M-NATIVE | `packaging/alpine`、`build-aux/prepare-alpine.py`、`src/meson.build`、README | 原生 APK、固定提交与源码校验、目标平台依赖、离线构建；Meson 只增加 `--locked` |
| M-LOCALE | `po/zh_Hans.po`、APKBUILD | 只翻译本轮新增字符串；APK 将已有 catalog 映射到 zh_CN |
| M-STARTUP | `data/com.github.marhkb.Pods.gschema.xml.in` | 用户于2026-11-24单独批准：只把 images.sort-attribute 的 choice/default 从 name 改为 id，共两行 |
| M-VERIFY、M-STARTUP | native workflow、`ci-alpine.sh`、`ui_tests.rs` 与子模块、`main.rs`、两个 engine harness、QEMU helpers | 验证适配控件、权限、传输和原生安装；扩充真实组件的非空列表、详情、表单、导航、输入和独立弹窗场景。隔离 Docker 中另测终端输入与缩放；`main.rs` 只增加测试模块声明 |

`Cargo.toml` 的新增直接依赖为已经存在于上游锁文件的 Hyper 0.14 和 tempfile，分别用于保持 Podman 的流上传类型和匿名临时文件。Tokio 的 net、tokio-util 的 io feature 服务于上述 socket 和文件传输。锁文件中 335 个包的名称、版本、来源和 checksum 与基线相同。

## 已撤回的范围外内容

先前候选中的 API 协商、端口地址、pull ID、build EOF、统计计算、stop 状态、连接取消与上传结果修复均已撤回。日志缓冲、解析、重连、导出及后台流生命周期修改也已撤回。通用构建 helper、上游发行流程调整、最低版本同步、旧翻译补全与欢迎文案改写不再保留。

批准启动修复后，以下 14 个文件仍与基线逐字节一致；schema 的两行例外单独列在上表：

```text
.github/workflows/release.yml
build-aux/dist-vendor.sh
meson.build
meson_options.txt
src/engine/dto/container_stats.rs
src/engine/dto/port.rs
src/engine/opts/container_create_opts.rs
src/model/connection_manager.rs
src/model/container_list.rs
src/model/image_build_action.rs
src/model/image_push_action.rs
src/view/container_log_page.rs
src/view/container_log_page.blp
src/view/welcome_page.blp
```

其他同时包含合法适配的文件按代码块回退：例如终端只保留辅助输入入口，上传只替换存储和传输载体。`src/log_buffer.rs`、`build-aux/cargo-build.py` 及其测试已删除。仓库内广泛 Docker/Registry 业务测试已缩减为适配路径与只读连接测试。

## 验证与审批边界

直接影响手机竖屏、触屏、导航、弹窗和输入的既有 UI 问题，依据最初需求纳入适配。是否修改由实际失败决定；未复现的候选保留原实现。完成更多页面的检查不要求更多页面都产生补丁，测试范围也不限定为已有补丁所触及的控件。

验证记录分别见 [Docker 与资源路径](testing-docker.md)、[QEMU 与控件](testing-qemu.md)和[Pixel 2 待验项目](device-acceptance.md)。先前宽范围候选的通过结果属于历史调查，当前工作树单独验证。

镜像排序 schema 的[两行补丁](proposals/startup-schema.patch)在范围回撤时曾撤销。用户于2026-11-24明确批准后，已按 M-STARTUP 单独应用并进入完整启动与导航验证。该授权不包含[独立清单](upstream-issues.md)中的其他上游问题。
