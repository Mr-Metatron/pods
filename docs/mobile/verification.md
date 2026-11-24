# Linux Mobile 适配验证记录

日期：2026-11-24。上游比较基线为 `349be73c8bc2a3340e2b6187df1c0c676b63be82`，本轮从 `541159251393f7ddfce55c254495f516239fa525` 继续纠正移动 UI 覆盖不足的问题。产品、构建模板与测试源码已固定为 `27802b3ef04e69b582355a62eba1461dc8cb90bb`。两架构原生r2包、中英文GTK测试，以及同一ARM64制品的完整系统QEMU验收均已通过；Pixel 2实机项目继续单独列为待验。

[r1 固定版本的验证报告](https://github.com/Mr-Metatron/pods/blob/541159251393f7ddfce55c254495f516239fa525/docs/mobile/verification.md)保留其准确范围：完整启动、加载态导航、共享控件，以及真实 Docker 空列表。那些结果没有覆盖本轮新增的非空详情与表单场景；本轮逐项补充实际组件验证。

## 设计与范围

修订设计先于本轮产品修改同步到[Google Drive](https://drive.google.com/file/d/1aBDSfs5XH-tiBO4DZcCdLd3k4OHcC7sG)，修改时间为2026-11-24 08:34:19 UTC，已完整读回并与本地文件一致。归档包含 In Scope、Out of Scope、架构、修改范围与验收矩阵，SHA-256为 `297bb4298534d0b0523dc847c0904f49c6bb106673c34883fbb3d62dc283f369`。

原生制品逐图审查又确认空镜像页的操作区在横屏下稳定裁切。对此先追加最小高度修正与空态按钮滚动验收方案，再于2026-11-24 19:17:04（台北时间，11:17:04 UTC）更新同一Drive设计并完整读回，SHA-256为 `5b9d7da9ebfc6ce258c40212cb913723d93a9824f87892f8534b7acce54545c4`；随后才补测试并实施该处修改。

直接影响手机竖屏、触屏、导航、弹窗和输入的既有 UI 问题按原始需求处理。每个补丁须有实际失败依据；通过检查的页面保留原实现。[变更追踪](change-scope.md)和[完整文件映射](scope-map.json)记录需求关联。用户单独批准的启动 schema 两行修改继续保留；无关上游问题只进入[独立清单](upstream-issues.md)。

本轮检查确认 Cargo.lock 中335个包的名称、版本、来源与校验值均保持基线不变。上游的发行流程、通用构建声明、端口转换、stats公式、连接取消、容器状态、build/push结果、日志、搜索状态以及创建选项回填实现保持原样。新增 GTK 场景仅在 `#[cfg(test)]` 构建中编译。

## 最小产品改动

相对r1，本轮产品修改仅涉及下表六个 Blueprint 文件，没有增加应用框架或改动引擎、模型与请求语义。原生包另外显式声明既有文件选择路径所需的 `xdg-desktop-portal` 前端；目标规格已有KDE后端，保留桌面自身的后端选择。

| 场景与修改前证据 | 最小调整 | 验证要点 |
| --- | --- | --- |
| ClientView 在668sp展开双栏，实际子控件最小宽为英文700px、中文695px，右侧按钮裁切 | 仅将该双栏阈值提高到720sp | 普通与选择状态、阈值两侧、旧边界、手机横竖屏与桌面，检查实际子控件和返回路径 |
| 镜像标签与卷名被同行状态计数挤成约100px窄列 | `image_row.blp`、`volume_row.blp` 内部使用已有 Adw.WrapBox | 360/366宽的普通列表中，镜像文字本身取得230/236px，卷名取得249/255px；选择状态分别为243/249px和262/268px。短内容与宽屏保持并排，菜单和计数绑定不变 |
| 容器详情的挂载卷名在338px行中只有108px | `container_volume_row.blp` 局部用同一 WrapBox，保留菜单位置 | 360/366宽时卷名取得243/249px；366×300、滚动与菜单检查通过 |
| 创建容器的等宽分页使 Integration 标题只有84px并被省略 | `container_create_opts_dialog.blp` 只关闭 ToggleGroup 的 homogeneous | 开发阶段同实例属性对照中，英文标题从84px增至106px；最终两架构中英文原生测试均确认标题无省略、三按钮可达。原28px按钮高度、页面与字段逻辑不变 |
| 空镜像页在651×366下把拉取/构建按钮裁成1px横线 | `images_panel.blp` 只将操作区最小高度从1改为106 | 同binary红测复现bin 1px、child minimum 44px；单行资源修改后英文/中文X11及Wayland均通过三列表×五视口的原有滚动和完整按钮边界检查 |

端口摘要仍完整换行，展开的IP字段、删除/展开操作和焦点可达；没有把它改成省略文本。环境变量/标签双输入行、操作完成/错误弹窗、进程搜索和保存连接筛选均已有可用路径，实测通过后保留原实现。

## 覆盖与结果的口径

UI 测试使用真实 GObject 模型、原有 refresh/inspect 入口以及实际 GTK/libadwaita 组件。隔离 Unix HTTP 夹具提供只读数据，保留请求记录；没有向产品增加测试专用模型接口。视口为360×651、366×651、651×366、366×300和1024×768，并补充导航边界。366×300检查可用高度缩短后的几何和焦点。

列表、详情、共享控件按相关场景覆盖上述五种视口；多数表单检查366×651、366×300与1024×768，创建容器、提交和复制另查360宽，双输入行和操作状态弹窗另查横屏。实际逐场景尺寸保留在测试源码与截图文件名中。原生颜色窗口使用独立输出专测，不能用父窗口尺寸推定其边界。

取消动作另抽样一次创建容器表单，在真实 `PdsWindow` 中点击共享的 `win.close` 动作，验证只关闭当前弹窗并保留主窗口。其他表单检查几何与相关控件；没有逐一执行真实主窗口的取消动作。普通布局夹具未安装主窗口的 `win.close` 动作，其截图中的取消按钮状态与这项真实动作检查分别记录。

| 页面与状态 | 检查范围 |
| --- | --- |
| 主窗口与导航 | 干净配置完整启动、欢迎页、既有侧栏选择/返回、宽窄切换和双栏边界 |
| 容器、镜像、卷 | 空与非空列表、长名称、选择模式、底部动作、列表/网格、搜索、菜单及三类详情 |
| 容器详情与辅助页 | Running/Exited/Dead、长字段、滚动到底部、重命名、取消删除、Inspect、已加载日志、进程表滚动/选择/搜索 |
| 创建与操作表单 | 连接、容器三页、提交、双向复制、镜像拉取/构建/标签/推送、卷创建、清理、设备/挂载/标签输入及完成/失败/取消状态 |
| 输入与补充页面 | 输入焦点在可见滚动区内、长内容、低高度、建议弹层、连接帮助、全局搜索结果、保存连接、引擎信息、操作历史 |
| 独立颜色窗口 | 从实际颜色按钮打开独立 GTK toplevel，在普通输出与真实366×651、366×300 Wayland输出检查；不以父窗口尺寸代替输出尺寸 |
| 连接中的终端 | 独立真实 Docker 容器中的输入、粘贴、复制、辅助键、中断与行列同步 |

全局搜索把已有查询替换成无匹配查询后，资源行数为零而仍显示初始 `search` 占位；在桌面宽度也复现。生产搜索逻辑保留。测试截图实际状态，并另以明确标记的模板夹具检查 `no-results` 页几何；这不计作查询到空态的切换通过。Dockerfile路径和内存单位回填缺陷同样独立记录，表单几何测试不作为其业务正确性证明。

## 开发阶段证据

| 记录 | 实际结果与边界 |
| --- | --- |
| `mobile-ui-baseline1` | 修改前长镜像标签和卷名的窄列截图；该轮日志夹具的until=0处理错误保留原始失败，随后只修测试响应 |
| `mobile-ui-baseline2` | 158张表单截图；测试客体缺少桌面schema导致中断。APK原已声明依赖，只补齐客体 |
| `mobile-ui-baseline3` | 表单、详情、日志、Inspect及进程页子阶段通过；全局搜索旧缺陷和组合测试超时均保留，整轮没有记作通过 |
| `mobile-ui-auxiliary4` | 20.14秒失败，明确记录全局搜索0行而占位错误；该生产行为保留 |
| `mobile-ui-toggle4` | 同实例仅切换homogeneous作对照，英文20.73秒、中文16.14秒通过；22张截图，随后只改一项模板属性 |
| `mobile-ui-live4` | 真实Docker终端在X11与Wayland分别18.01秒、17.29秒通过，14张截图及实际终端文本 |
| `mobile-ui-color-desktop4/5` | 测试没有先滚动至颜色按钮，观察不到chooser；未据此更改产品。修正实际滚动/点击与窗口观察后另行验证 |
| `mobile-ui-full5` | 90.61秒结束，131张截图；辅助页面与共享控件通过。新分页检查和宽窄切换的帧时序检查未通过，整轮保持失败记录 |
| `mobile-ui-color-desktop6` / `mobile-ui-color-phone6` | 分别8.05秒、7.53秒通过，合计4张截图。独立颜色窗口在366×651的真实Wayland输出上自然分配366×308内容区，确认/取消按钮可达，取消保持原颜色；色盘末列与自定义编辑器另补专测 |
| `mobile-ui-full6` | 600秒达到执行器时限，保留379张截图；表单、辅助页面、列表与详情三个阶段全部完成，组合用例仍按超时失败记录。最后共享控件尚未执行，随后只改进测试等待并重跑完整用例 |
| `mobile-ui-full7` | 479.96秒失败，保留379张截图；真实主窗口取消、表单、辅助页和共享控件通过。三个720选择态的删除按钮可达断言通过；详情分组在日志页面检查后遇到5秒绘制等待超时，保留失败并增加精确调用与绘制阶段诊断 |
| `mobile-ui-color-desktop7` | 7.19秒失败，色盘右端五个色块通过；测试对自定义色块误用了普通Widget激活入口。随后改为触发该原生控件已有的Space键处理器，产品未修改 |
| `mobile-ui-pages8` | 独立页面组完整通过，463.80秒、140张截图。包含日志返回、Exited/Dead详情与最终Top搜索；没有触发慢帧或帧超时诊断。此组单独执行，最终组合结果仍由匹配源码的原生CI及ARM64客体给出 |
| `mobile-ui-color-desktop8` / `mobile-ui-color-phone8` / `mobile-ui-color-phone-short8` | 桌面7.75秒通过；两组手机输出分别8.06秒和7.52秒失败，原因均为自定义编辑器的输入区右缘超出原生横向滚动视口2px。实际色值可见，原始截图与矩形保留；随后把焦点可见性和完整输入区的滚动可达性分别检查 |
| `mobile-ui-color-desktop9` / `mobile-ui-color-phone9` / `mobile-ui-color-phone-short9` | 桌面9.19秒、366×651 Wayland 9.12秒、366×300 Wayland 8.03秒全部通过，各6张截图。色盘末列可达；默认输入首尾光标可见，沿原有横向滚动条移动后完整输入区通过严格边界检查，取消保持原色；颜色产品代码未修改 |
| `mobile-ui-empty-before` | 24.79秒按预期失败、23张截图；新增断言确认651×366时按钮自身44px而父bin仅1px。该失败在单行高度修正前封存，360/366×651的操作区此前均取得106px |
| `mobile-ui-empty-after` | 与before同一Rust binary，只替换单行高度修正后的界面资源；EN X11 34.77秒、ZH X11 32.43秒、Wayland35.81秒全部通过，各37张截图。651×366下bin106px容纳横排44px，366×300滚底后完整容纳竖排106px；容器和卷空态保持原布局并通过 |
| `mobile-ui-cta-empty` | 新增共用滚动裁切检查后，EN X11 38.14秒通过、42张截图；覆盖真实欢迎页的五视口按钮，以及三列表的五视口空态。新增过滤Show All和停止终端Start检查进入完整组合验证 |
| `mobile-ui-cta-full` | 同一冻结构建的完整EN X11组合730.98秒通过、448张截图、574次只读夹具请求。表单、辅助页、空态、列表/详情及共享控件全部完成；欢迎页、三类过滤Show All和停止终端Start的滚动检查通过。退出码为零，客体只读重新挂载后正常关机。该开发构建的相关源码哈希与最终固定源码一致，原生APK另行验证 |

开发客体为x86_64、Alpine3.24.4、Linux6.18.60-virt、2个虚拟CPU、2GiB RAM、QEMU TCG，无KVM和网络接口。GTK4.22.4、GLib2.88.4、libadwaita1.9.2、VTE0.84.0；GUI以UID1000运行。Wayland使用有输入seat的nested Weston和软件渲染，应用实际后端为GdkWaylandDisplay。

测试执行器保留实时日志、默认每进程600秒超时和准确退出状态。主循环等待有截止时间；缩放后等待GTK实际after-paint帧，再检查窗口和子控件的精确分配。没有放宽裁切断言来消除失败。每次开发执行保存二进制、安装资源和实际编译源码快照的哈希；未重新编译的短专测明确引用原快照。

第8次测试构建按每帧完成回调请求下一帧，保留两次after-paint与5秒等待条件，并记录慢等待的调用位置、帧数、窗口状态和单次GLib事件处理耗时。开发完整组与页面专测将外层进程预算显式设为1200秒；CI保持600秒，最终ARM64客体单独记录其预算。页面专测和先前完整组的运行上下文不同，未将单独通过用作超时根因的唯一证据。

原生颜色编辑器保留GTK的横向滚动布局。手机输出的默认输入区矩形为x247、宽111，右缘358超过356宽的滚动区2px；色值首尾光标位于x247和x297，均可见。沿既有滚动条移动后，完整输入区位于x201、宽111并严格落在视口内。此处没有把原始边缘裁切改写成默认整框自动适配，也没有更改几何容差。

## 真实 Docker 终端

开发阶段的真实daemon为Docker29.9.3，API1.56，独立socket是 `/run/pods-ui-terminal.sock`，权限为0660、所有者root、组1000。辅助脚本离线导入客体对应根文件系统，只创建一个专用、无网络的容器；退出时清理该资源并停止daemon。

两后端均验证了VTE commit到shell、触控粘贴入口、选择复制、五个辅助键的精确输入字节，以及Ctrl+C终止前台sleep。五种视口下，容器 `stty size` 与VTE实际行列完全一致：30×39、30×40、14×72、10×40、40×113，分别对应上述五个常规视口。原有terminal commit和会话结束逻辑没有修改。复现命令与证据边界见[Docker记录](testing-docker.md)和[QEMU说明](testing-qemu.md)。

## 原生中间构建 E 与空态覆盖缺口

源码 `182aad8d00fdcf7a4faa73c034cb4ce3f3ef3ffd` 的[原生CI](https://github.com/Mr-Metatron/pods/actions/runs/45377624764)两架构job均成功。x86_64制品 `14914971131` 的ZIP、APK、harness及ELF已核对；12项契约通过，中英文完整UI分别163.30秒和161.42秒通过，各408张截图，Docker28.0.4只读连接通过。aarch64制品为 `14912924949`，其job状态成功；未将它用于最终ARM64客体验收。

逐图复核 `empty-images-651x366.png` 后，发现英文及中文的拉取/构建按钮被裁为约1px。源码仍是上游 `images_panel.blp` 的 `height-request: 1`；三条44/106px子控件超出1px容器的告警与稳定截图一致。旧空态断言只查工具栏、横向边界，缺少按钮纵向裁切和滚动可达性。E的自动测试成功保留其准确断言范围；该覆盖缺口随后用下述S的新增断言和原生制品重新验证。

## 最终原生构建 S

最终产品、构建模板与测试源码固定为 S：`27802b3ef04e69b582355a62eba1461dc8cb90bb`。[Native Alpine CI 45385035150](https://github.com/Mr-Metatron/pods/actions/runs/45385035150) 的 attempt 1 已成功结束，x86_64 与 aarch64 两个 job 的构建、安装、GTK 测试和制品上传步骤均为 success。交付包版本为 `3.3.0_git20261124-r2`。

| 架构 | Job ID | Artifact ID | ZIP 大小（bytes） | ZIP SHA-256 |
| --- | ---: | ---: | ---: | --- |
| x86_64 | 138413635516 | 14917267018 | 46,915,253 | `ed25c5dfcf4e5636958dc4f17f815b84f210b29042249805a9994c92d45ac26d` |
| aarch64 | 138413635964 | 14916304845 | 46,369,699 | `7e016bc94d877ac4b3d81d2011557b04a64ee84048659580fa53bd888e2a3fe6` |

两份下载记录的 ZIP 大小、SHA-256 与 GitHub artifact 元数据一致，artifact 的 `head_sha` 均为 S。记录已核对主包、语言包及 `pods-ui`、`engine_contract`、`engine_live` 三个测试程序的校验值。ARM 主程序与测试程序均为 aarch64 ELF，使用 `/lib/ld-musl-aarch64.so.1`；x86_64 对应 `/lib/ld-musl-x86_64.so.1`。

证据：交付ZIP中的 `evidence/review/native-ci-final-status.json`、`evidence/native-aarch64/download-verification.json` 和 `evidence/native-x86_64/download-verification.json`。

### 普通用户测试与两语言界面

两个架构均在 Alpine 3.24 原生容器中执行。`engine_contract` 和 GUI 测试使用普通用户 `uid=1000(builder)`、`gid=1000(builder)`，补充组为 `300(abuild)`；GUI 环境为 Xvfb/X11、Cairo 渲染及独立 D-Bus 会话。每种语言执行一个完整 GUI 聚合测试。

| 架构 | engine_contract | 英文 GUI | 中文 GUI | 截图数量 |
| --- | --- | --- | --- | --- |
| x86_64 | 12 passed，0 failed，0.16 秒 | 1 passed，171.86 秒 | 1 passed，172.29 秒 | EN 448 张、zh_CN 448 张，合计 896 张 |
| aarch64 | 12 passed，0 failed，0.16 秒 | 1 passed，181.38 秒 | 1 passed，182.74 秒 | EN 448 张、zh_CN 448 张，合计 896 张 |

四份完整 GUI 日志均完成表单、辅助页、空态、非空导航／详情及共享控件阶段，并包含新增空态按钮和已有操作按钮的滚动可达性检查。原空镜像页的 `exceeds AdwBreakpointBin height` 告警在四份最终日志中的数量均为零。两个架构都确认在 `LANG=zh_CN.UTF-8`、未设置 `LANGUAGE` 时取得“正则表达式”译文；英文对应“Regular Expression”。

日志证据位于 `evidence/native-aarch64/logs/` 和 `evidence/native-x86_64/logs/`，对应 `test-user.txt`、`engine-contract.txt`、`ui-en.txt`、`ui-zh_CN.txt`、`locale-en.txt`、`locale-zh_CN.txt`。对应制品根目录下的 `screenshots/en/` 和 `screenshots/zh_CN/` 均实际包含 448 个 PNG。

### x86_64 真实 Docker 只读检查

x86_64 的 `docker_local_connection_smoke` 通过，耗时 0.01 秒。它显式连接 `unix:///var/run/docker.sock`，完成 Docker 引擎识别、ping、info 及容器／镜像／卷列表读取。日志记录 daemon 为 Docker 28.0.4、API 1.48、x86_64，内核为 `7.0.0-1014-aws`；读取时有 1 个容器、7 个镜像、0 个卷。

这项检查以普通用户 `uid=1001(tester)`、`gid=1001(tester)` 运行，补充组为 `116(engine-test)`。CI 在一次性测试容器内将 tester 加入 socket 对应 GID 的组，沿用原 socket 权限。该测试的操作范围是读取已有 daemon 状态。

CI 中 aarch64 的 `Check the local Docker connection` 步骤为 **skipped**；真实ARM Docker的独立客体结果见下节。

证据：交付ZIP中的 `evidence/native-x86_64/docker-connection.log` 和 `docker-version.txt`；执行路径为固定源码 S 的 `build-aux/ci-alpine.sh` 与 `tests/engine_live.rs`。

### 最终 ARM APK 与公钥

| 文件 | 大小（bytes） | SHA-256 |
| --- | ---: | --- |
| `pods-3.3.0_git20261124-r2.apk` | 4,402,897 | `741b1ed33619fd9a92c699014293ddb368b5a7b4c66ff0f9ad81f1c2f3a8045c` |
| `pods-lang-3.3.0_git20261124-r2.apk` | 148,029 | `1b49193fa29b9dff3d7bd9e7bef54ce57c9b0983bf708ffc5cbcd30440a94da9` |

上述文件在交付ZIP的 `packages/` 中。对应公钥文件为 `keys/builder-6b0575c9.rsa.pub`：文件 SHA-256 为 `f5c786fe7cdc6c24754ead60b2782ef1663d7e914851dd8a2313e7a2e5483a98`，公钥 SPKI DER SHA-256 为 `88f52b264c1a91236a615c1306478ab613033c8b1208e11130ed8b3ba042b67f`。两种摘要的计算对象分别为公钥文件字节与其 SPKI DER 编码。

包大小和公钥摘要来自上述 ARM 下载核验记录；管理员核对公钥后安装APK，GUI继续以普通用户运行。

### 固定源码 APKBUILD 的归档验证

本次 CI 从 S 的检出内容生成带校验值的构建配方，制品内配方的 `_commit` 为 S，source 名称为 `pods-3.3.0_git20261124-27802b3ef04e.tar.gz`。供直接使用的固定 `packaging/alpine/APKBUILD` 则指向 [GitHub 的 S 源码归档](https://github.com/Mr-Metatron/pods/archive/27802b3ef04e69b582355a62eba1461dc8cb90bb.tar.gz)，其单独验证范围为下载、SHA-512 校验、解包和 Git 内容／模式比对。

| 验证项 | 结果 |
| --- | --- |
| GitHub 归档文件 | `pods-27802b3ef04e69b582355a62eba1461dc8cb90bb.tar.gz`，1,323,682 bytes |
| 归档 SHA-256 | `8791bbec66eb2ae5d28d3fba63cbbe242f9b3206a149977360a1797fe200bc4f` |
| 归档 SHA-512 | `f4245ae5e05e8b4f0bbcad417dbe6f3d63f67d717053594372dafdb0bb14893dafbee5962f3315a61eec528f6df87af0218c9afca2b7ea16f61702f3b7c77341` |
| 固定 APKBUILD SHA-256 | `f0e4b3317755a1ebdc305d0e61f2da3f386279146cdec22aa4369b90e1937f51` |
| Git 归档内容比对 | S 的 554 个 tracked files 全部匹配，blob ID 与文件模式一致 |
| abuild 校验及解包 | 退出码 0；解包后 554 个文件再次与 S 匹配，blob ID 与文件模式一致 |
| 配方解析版本／目录 | `3.3.0_git20261124-r2`；`src/pods-27802b3ef04e69b582355a62eba1461dc8cb90bb` |

证据：交付ZIP中的 `evidence/review/pinned-source-verification.json` 和 `evidence/review/abuild-verify-unpack.log`。

## ARM64 最终安装与运行

使用上述artifact `14916304845` 的APK、公钥和三个测试程序，在新的ARM64完整系统QEMU中执行验收。客体为Alpine3.24.4、Linux6.18.60-0-virt、4096字节页，2个虚拟CPU、1536MiB RAM、TCG软件执行且无网络接口。GTK4.22.4、GLib2.88.4、libadwaita1.9.2与原生构建版本一致；GUI和测试以UID1000运行。

签名安装成功，版本为 `3.3.0_git20261124-r2`，安装后的schema默认值为 `images.sort-attribute='id'`。正式 `/usr/bin/pods` 的SHA-256为 `44105efa4230c92b2be42a19600766252314f81d7f9d76e5a2f696cbd0560af7`，与原生APK内的文件完全一致。GTK检查调用同次编译的harness，加载实际安装资源。

| 验收阶段 | 实际结果 |
| --- | --- |
| 12项契约，含普通用户实际权限拒绝 | 12 passed，0 failed，0.50秒 |
| 真实本机Docker只读连接 | 1 passed，0.32秒；aarch64 Docker29.9.3，读取时容器、镜像、卷均为零 |
| 完整Wayland GTK聚合组 | 1 passed，840.89秒；448张截图、697次只读夹具请求，包含所有新增按钮滚动检查 |
| 真实运行中终端，X11 | 1 passed，17.50秒，7张截图 |
| 真实运行中终端，Wayland | 1 passed，17.32秒，7张截图 |

Wayland客户端使用nested Weston、软件渲染及虚拟输入seat。完整组预先设定的进程预算为900秒；两帧绘制等待、单次5秒门槛和几何断言保持原样。页面组结束后出现较长等待，整组仍在该预算内成功结束；没有将QEMU耗时换算为手机性能。

真实终端使用独立Docker daemon和 `/run/pods-ui-terminal.sock`（0660、root:1000），离线导入匹配架构的Alpine测试镜像，只创建一个无网络容器。两后端均验证真实shell输入、剪贴板、五个辅助键的精确字节及Ctrl+C中断。五种视口下的VTE与容器 `stty size` 一致，结果与前述开发记录中的行列矩阵相同。脚本退出时清理自建容器并停止daemon。

四个客体阶段、guest和QEMU退出码均为0，根文件系统重新挂载为只读后正常关机。离线导出513个输出文件，逐文件哈希核对通过；462张PNG全部可解析。导出器的UID/GID保留警告仅涉及宿主所有权元数据，原始日志保留，文件内容验收通过。

证据随交付ZIP保存在 `evidence/arm64-qemu/`，包括安装日志、独立测试日志、448张Wayland界面截图及14张真实终端截图、串口、输入哈希和阶段退出状态。`summary.json` 的SHA-256为 `5520f231e9312fc652c31ee2e11f86ca69d52565ad2995ee08f032734ef3dd8d`。五组未编辑的布局前后对照另存于 `evidence/history/comparisons/`，每组注明架构、页面状态和滚动位置差异。

最终交付提交只更新这些结果文档及指向S的固定APKBUILD，产品、构建模板与测试代码保持S不变。原始失败记录、中间候选E、开发调试构建和最终原生制品分别保留来源。

## Pixel 2 实机待验

实际目标仍为用户提供的walleye、4GiB RAM、aarch64 musl、4KiB页、Plasma Mobile/KWin Wayland及本机Docker29.9.2。295%缩放、Adreno/Freedreno、真实触屏、Plasma Keyboard显示/隐藏与中文preedit、旋转、Portal、休眠恢复、手机RSS/PSS和耗电需要实机记录。

原生包与模拟机验证覆盖可以在当前环境执行的源码、依赖、布局、焦点、导航、Wayland协议路径和隔离daemon输入。设备验收步骤见[Pixel 2表](device-acceptance.md)；QEMU的软件渲染耗时不用于推断手机性能。
