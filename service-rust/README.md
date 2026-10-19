# Sun-Panel Rust 维护文档

当前版采用模块化单体：一个 Rust 进程提供 API、Vue 静态前端和上传图片。使用 Axum、Tokio、SQLx 与 SQLite WAL，默认两个工作线程、两个数据库连接，无需 Redis。打开即可编辑同一面板，无登录和注册；导航只有一个地址。功能概览与上游对比见 [根 README](../README.md)，版本更新见 [CHANGELOG](../CHANGELOG.md)。

下列命令均从仓库根目录执行。默认仅供可信网络访问；本服务没有内置用户认证，远程访问需由网关或反向代理控制。

## 本地开发

工具链固定为 Rust 1.99.1、Node.js 24.22.0、pnpm 9.4.0，具体依赖由 Cargo.lock 和 pnpm-lock.yaml 锁定。前端组件使用 shadcn-vue 与 Reka UI，主题采用 Tailwind 4 的语义色变量，已移除 Naive UI 与旧运行时样式注入。

```sh
pnpm install --frozen-lockfile
pnpm run build
cargo run --manifest-path service-rust/Cargo.toml --locked
```

访问 `http://127.0.0.1:3002`，健康检查为 `/api/health`。默认数据目录是仓库根目录的 `./data`，前端目录是 `./dist`。开发前端时运行 `pnpm dev`，Vite 在 `http://127.0.0.1:1002` 将 `/api` 和 `/uploads` 代理到 Rust 服务。

前端使用 Tailwind 4 和现代 CSS，浏览器兼容性以实际验证为准。首页按视口高度计算上下留白；仅内容实际溢出时滚动，浏览器支持 scrollbar-gutter 时保留稳定滚动条空间。

## Docker

仓库只保留根 Dockerfile 和 docker-compose.yml，1.0.0 本地镜像名为 `sun-panel-rust:1.0.0`。

```sh
docker compose up --build -d
docker compose logs sun-panel-rust
docker compose ps
```

默认发布到 `127.0.0.1:3002`，可通过 SUN_PANEL_PORT 改宿主端口。局域网访问需修改端口绑定或接入反向代理。数据在 `sun-panel-rust-data` 命名卷，删除容器不会删除该卷。`docker compose down -v` 会删除卷。

运行镜像为非 root distroless，UID/GID 为 `10001:10001`，根文件系统只读，数据目录可写，使用程序内置 `--healthcheck`。图片存储、数据库与配置数据都位于数据卷。

只构建镜像时使用 `docker build -t sun-panel-rust:1.0.0 .`。这会构建当前 Docker builder 的目标架构；发布工作流中配置多架构构建，不代表本机已经验证两个架构。绑定宿主机目录代替命名卷时，应预先保证该目录对 UID/GID `10001:10001` 可写。

## 配置

| 环境变量 | 默认值 | 说明 |
|---|---|---|
| SUN_PANEL_BIND | 127.0.0.1:3002 | Docker 为 0.0.0.0:3002 |
| SUN_PANEL_DATA_DIR | ./data | 数据根目录 |
| SUN_PANEL_DATABASE | 数据目录/database.db | SQLite 文件 |
| SUN_PANEL_UPLOADS | 数据目录/uploads | 图片目录 |
| SUN_PANEL_WEB_DIR | ./dist | 构建后的前端 |
| SUN_PANEL_WORKER_THREADS | 2 | 1–64 |
| SUN_PANEL_DB_CONNECTIONS | 2 | 1–16 |
| SUN_PANEL_MONITOR_INTERVAL | 5 | 共享采样缓存秒数，1–300 |
| SUN_PANEL_MAX_UPLOAD_BYTES | 8388608 | 上传限制，1024–67108864 |
| SUN_PANEL_ALLOW_PRIVATE_FAVICON | false | 允许抓取内网图标 |
| RUST_LOG | sun_panel_rust=info,tower_http=warn | 日志过滤 |

内网图标抓取可设置 SUN_PANEL_ALLOW_PRIVATE_FAVICON=true，回环、链路本地和保留地址仍禁止，DNS 与重定向都会校验。上传接受受限的栅格图片，不接受主动 SVG、HTML 和脚本。

## 导航与配置

首页通过 `GET` 或 `POST /api/panel/navigation` 聚合读取分组和应用。响应的 `data.list` 保持分组及其 `items` 结构，顶层 `revision` 与列表从同一只读事务取得。应用在每个分组内按 `sort,id` 排列。

点击右上角“编辑面板”后，应用可以直接拖到同组、其他分组或空分组的任意位置，松手自动保存。搜索期间暂停拖动，清空搜索即可恢复。监控卡片在监控区域内直接拖动并自动保存，不参与应用分组。应用和监控共用更多操作按钮；右上角状态标记显示自动保存、保存中、已保存或结果待核对，旁边的帮助按钮按需展开操作说明，不占用导航正文高度。分组自身的顺序仍在“分组管理”中通过拖动或上下移动调整，并保存或取消。

应用移动统一使用 `POST /api/panel/navigation/move`，提交读取快照时的导航版本：

```json
{ "itemId": 12, "targetGroupId": 3, "targetIndex": 1, "revision": 42 }
```

`targetIndex` 是从原列表移除该应用后，插入目标列表的 0 起始位置，允许插到列表末尾；空分组只能使用 0。服务端在 `BEGIN IMMEDIATE` 事务内检查版本、应用与分组归属及索引，同时保存源组顺序、目标组顺序和应用所属分组。成功返回 `{ "code": 0, "data": { "revision": 45 }, "revision": 45 }`；版本由受影响记录的数据库触发器递增，不能假设一次移动只增加 1。原位置移动不写入，也不增加版本。非法输入或数据库错误整体回滚。

快照过期时返回 HTTP 200、业务 `code: 1409` 和 `currentRevision`，客户端载入最新排列，不使用旧列表覆盖其他页面的修改。写入超时或连接中断时先只读核对；结果仍不明确会暂停后续拖动，并提供“核对结果”入口，不自动重放移动。旧应用 `/api/panel/itemIcon/saveSort` 已删除；分组管理仍使用 `/api/panel/itemIconGroup/saveSort`。

应用和分组删除后进入持久化回收站，保留原 ID、内容和排序，重启后仍可恢复，不自动过期。删除接口可传 `revision` 校验快照，成功返回 `data.trash: [{ kind, id, deletionId }]`，顶层和 `data.revision` 返回新版本；分组排序同样接受可选 `revision` 并返回新版本。旧客户端未传版本时仍保持原协议。

回收站 API 均使用 `POST`：

| 地址 | 请求 | 行为 |
|---|---|---|
| `/api/panel/navigation/trash/list` | `{}` | `data.list` 返回 kind、id、title、deletedAt、deletionId，分组附带 itemCount，应用附带 groupId、groupTitle、requiresTargetGroup；data.count 为可操作条目数 |
| `/api/panel/navigation/trash/restore` | `{ kind, id, deletionId?, revision?, targetGroupId?, targetIndex? }` | 恢复条目；原分组缺失或已删除的应用需指定一个现存分组；可按目标组内 0 起始索引恢复位置，恢复及重排在同一事务中完成 |
| `/api/panel/navigation/trash/purge` | `{ kind, id, deletionId?, revision? }` | 永久删除指定条目；无法撤销 |

列表、恢复和永久删除均返回顶层及 `data.revision`。恢复或永久删除可传版本及删除批次，旧页面或旧撤销动作遇到版本/批次变化返回 `code: 1409`，不覆盖后续修改。分组的删除批次只包含删除该分组时仍存在的应用；恢复或永久删除分组仅处理该批次，之前单独删除的应用保留在回收站。历史软删除数据没有可靠批次记录，分组和应用分别恢复，避免意外复活先前删除的应用。回收站内的图标继续计入图片引用，关联条目永久删除后才允许删除图片；恢复时仍检查被外部或旧版本删除的托管图片。

系统应用中的“回收站”支持恢复应用、恢复分组及其同批应用、逐条永久删除；原分组不可用时可选择新的目标分组。记录保留至手动永久删除。当前页面会话还保留最近 20 步导航撤销，包括应用拖动、分组排序和删除；首页工具栏及分组管理提供撤销按钮，编辑面板时支持 Cmd/Ctrl+Z，输入框仍使用原生文字撤销。撤销按导航版本核对，外部导航修改会使历史失效；回收站内容则跨刷新和服务重启持久保存。

导航导出 `GET` 或 `POST /api/panel/navigation/export` 返回 `appName: "Sun-Panel-Navigation"`、`schemaVersion: 1` 文档。导入只接受此格式，先向 `/api/panel/navigation/import/preview` 提交 document 与 mode，再向 `/import` 提交同一内容和预览返回的 revision。支持 merge、append、replace，替换需要在页面输入确认短语；预览过期时拒绝写入。

导航 JSON 包含分组、图标引用和应用地址，不包含图片实体、外观和小组件。完整实例迁移使用下方备份恢复。

风格保存使用防抖和串行队列，按 revision 乐观并发校验。不同字段可合并，同字段冲突保留草稿并由用户载入服务端版本。模块按名称串行保存。配置冲突返回 HTTP 200、业务 code 1409/currentRevision，客户端不盲重发旧快照。

前端读取默认 10 秒、普通写入 15 秒、导入/预览/上传 35 秒绝对截止。支持导航和上传主动取消、编辑器读取取消及监控后台暂停。写入中断不能确定服务端是否提交：配置会读回核对，导入须重新预览。取消上传后服务端可能已保存文件，可到图片管理核对。

默认页脚为空，可自行设置 HTML。自带壁纸使用固定 /default-wallpaper.webp 路径，空配置或图片加载失败时回退默认图片，可在风格设置点击“使用默认壁纸”。编辑和系统应用按钮位于页面右上角，操作按钮显式使用浅色文字。卡片操作菜单只在编辑模式显示。偏好和系统应用菜单具有独立的明暗主题悬停样式。

## 完整备份恢复

先停止服务。快照包含一致 SQLite 副本（包括 WAL 提交记录）、图片文件和空目录、版本/时间/大小与 SHA256 清单。部署环境变量、反向代理和镜像由部署配置管理。

```sh
SUN_PANEL_DATA_DIR=./data ./service-rust/target/release/sun-panel-rust backup ./instance-backups/manual-001
```

恢复到新的空目录：

```sh
SUN_PANEL_DATA_DIR=./restored-data ./service-rust/target/release/sun-panel-rust restore ./instance-backups/manual-001
```

备份目标必须是不存在的新目录。恢复目标必须无数据库、WAL/SHM/journal，uploads 不存在或为空。服务和维护命令共用 OS 文件锁；校验失败不发布，普通发布错误回滚。上传和备份树不接受符号链接。分离自定义目录不承诺断电或强杀下跨文件原子恢复。

Docker 可将快照写入数据卷，再复制到宿主机：

```sh
docker compose stop sun-panel-rust
docker compose run --rm --no-deps --entrypoint /app/sun-panel-rust sun-panel-rust backup /app/data/backups/manual-001
mkdir -p ./instance-backups
docker compose cp sun-panel-rust:/app/data/backups/manual-001 ./instance-backups/manual-001
docker compose start sun-panel-rust
```

上例要求 Compose 服务容器已经创建，且 `manual-001` 备份目录尚不存在。备份成功后复制出容器数据卷；不要仅把快照保留在同一块数据盘。恢复应先在新的空目录或命名卷中执行验证，再切换。维护时不要让外部程序修改数据目录。

## 数据格式边界

当前版只接受当前格式导航文档及 Rust 完整备份。旧 Go 数据库自动升级、旧 .sun-panel.json、MD5 兼容、内外网双地址输入、migrate-legacy 命令及 tools 迁移容器已移除。

已应用到现有 Rust 数据库的 SQL 迁移文件保持原始内容和校验值。它们是数据库版本历史，不能作为“旧代码”删除；第六份迁移记录回收站删除批次，第七份迁移支持独立图标风格。历史数据行不会在升级时自动删除。升级前完整备份，回退时使用旧版本镜像和对应旧备份，避免让旧程序直接打开新版本数据库。

## 验证与构建

```sh
cargo fmt --manifest-path service-rust/Cargo.toml --all --check
cargo clippy --manifest-path service-rust/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path service-rust/Cargo.toml --locked
cargo build --manifest-path service-rust/Cargo.toml --release --locked
pnpm run lint
pnpm test
pnpm run build
SUN_PANEL_TEST_IMAGE=sun-panel-rust:1.0.0 pnpm run test:container
```

Rust 集成测试使用临时数据库，容器回归创建独立测试实例；运行容器回归前需先构建对应镜像。`SUN_PANEL_TEST_IMAGE` 可指定镜像名，未指定时以脚本中的默认值为准。CI 配置原生 amd64/arm64 容器回归；配置存在不代表远端本轮已运行。覆盖率测量方式及结果应注明工具、执行命令和统计范围，不能用测试数量代替覆盖率。

覆盖率从仓库根目录执行：

```sh
pnpm run test:coverage
rustup component add llvm-tools-preview --toolchain 1.99.1
cargo install cargo-llvm-cov --version 0.9.1 --locked
./service-rust/scripts/coverage.sh
```

前端命令统计测试直接导入的 TS/构建模块，不代表全部 Vue 界面覆盖率。Rust 报告输出到 `service-rust/target/coverage/`；`html/index.html`、LCOV 和 JSON 保留原始计数，`summary.json` 统计排除测试源码后的生产可执行行。本次结果、未覆盖范围和最终镜像验证见 [1.0.0 验证记录](../doc/verification-1.0.0.md)。

静态资源在构建时生成 Brotli/Gzip，运行时直接提供。哈希资源长期缓存，HTML/custom/service worker 重新验证。监控共享聚合请求和采样缓存，页面后台停止轮询；监控展示服务运行环境，不默认等同于完整宿主 NAS 或容器配额。

完整功能及裁剪依赖见 [doc/features.json](../doc/features.json)。性能脚本 `service-rust/scripts/benchmark.py` 只读测量已运行服务的现有导航数据，不会生成测试应用或比较 Go 后端；使用 `python3 service-rust/scripts/benchmark.py --help` 查看参数。测量时记录机器、架构、数据规模与命令，不把历史结果当作当前性能或跨语言提升的承诺。

## shadcn-vue 界面适配

基础组件位于 `src/components/ui`，registry 配置为根 `components.json`。按钮、弹出帮助、状态标记、表单控件和菜单优先使用 shadcn-vue / Reka UI。项目共享组件位于 `src/components/common`，负责受控弹窗、透明颜色选择、文件选择/拖放、图片预览及指针菜单等业务适配。应用与监控统一使用 `CardActionsButton` 展示更多操作。所有业务页面直接使用这些组件，不提供旧 UI 库名称的兼容层。

颜色选择支持 HEX/RGB/RGBA 和透明度；监控圆环使用 SVG，继续区分未知、失败和过期。上传复用带截止时间的请求层，保留进度、取消与卸载清理。反馈统一使用 Sonner 与 AlertDialog，确认队列按 ID 结算，弹窗关闭后恢复触发位置的焦点。

界面使用中文，主题支持明暗和跟随系统；主题偏好仅保存在当前浏览器。UI 组件源码随项目维护，更新时应审核生成代码与本地适配。

## 发布流程

`CHANGELOG.md` 保存版本索引，`doc/releases/<版本>.md` 保存该次 GitHub Release 正文；不再使用原项目的 `UPDATELOG.md`。准备下一个版本时，更新包版本、Rust 包版本、锁文件、Compose 默认镜像标签及对应发布文档，再通过本地检查。

发布标签采用 `v1.0.0` 格式。`publish-rust.yml` 在标签推送后调用验证工作流，校验标签与项目版本、发布说明一致，通过后读取 `doc/releases/1.0.0.md` 创建 GitHub Release。推送 `main` 仅更新源码，不等于发布新版本。

若需同时推送 Docker Hub 镜像，在仓库 Actions 中配置：

| 类型 | 名称 | 内容 |
|---|---|---|
| Variable | `DOCKER_IMAGE_NAME` | 实际镜像仓库，如 `你的账号/sun-panel-rust` |
| Secret | `DOCKERHUB_USERNAME` | Docker Hub 账号 |
| Secret | `DOCKERHUB_TOKEN` | 具有推送权限的访问令牌 |

未配置镜像仓库时只发布 GitHub Release。已配置仓库和凭据时，工作流构建并推送 `linux/amd64`、`linux/arm64` 镜像，以版本号和 `latest` 标记。远端发布是否成功需以 Actions 和 Release 实际结果确认；本地 `sun-panel-rust:1.0.0` 构建不执行公网推送。
