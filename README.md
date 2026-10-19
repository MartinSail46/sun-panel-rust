# Sun-Panel Rust

面向个人服务器、NAS 和浏览器首页的单用户导航面板。基于 [Sun-Panel 公开 v1.8.1 标签源码](https://github.com/hslr-s/sun-panel/tree/v1.8.1)继续开发，以 Rust 重写后端，前端采用 Vue 3 与 shadcn-vue。

一个进程提供网页、API 和上传图片，使用 SQLite 保存数据，无需外部数据库或 Redis。打开即可编辑同一个面板，每个应用只维护一个访问地址。

> 当前版本：**1.0.0**。本地镜像名：`sun-panel-rust:1.0.0`。本项目没有登录与权限隔离，应部署在可信网络中；需要远程访问时，在反向代理或访问网关中配置认证。

## 功能

- **导航与分组**：新增、编辑、搜索应用；支持当前页、新标签和网页小窗口。进入编辑模式后，可将应用直接拖到同组、其他分组或空分组，松手自动保存。
- **图标与外观**：文字、图片、Iconify 图标、网站图标抓取；支持每项独立卡片风格、壁纸、布局、明暗主题和自定义 HTML 页脚。
- **回收与撤销**：应用和分组删除后进入持久化回收站；当前页面支持最近 20 步导航撤销。永久删除需单独确认。
- **时钟与监控**：时钟、日期、网页搜索，以及 CPU、内存、磁盘监控；页面进入后台后暂停轮询。
- **数据维护**：导航 JSON 导入预览、合并、追加与替换；离线命令可备份和恢复数据库及上传图片。
- **保存保护**：导航移动使用事务与版本检查；配置保存处理并发冲突，写入超时后先核对结果。

完整盘点及功能边界见 [功能清单](doc/features.json)，操作与部署细节见 [维护文档](service-rust/README.md)。

## 与原项目的功能对比

下表对比本仓库可检查的上游 **公开 v1.8.1 标签源码**，不涵盖上游后续闭源版本或 PRO 功能，也不作为性能对比。

| 项目 | 原项目公开源码 | Sun-Panel Rust 1.0.0 |
|---|---|---|
| 后端 | Go、Gin、GORM | Rust、Axum、SQLx |
| 数据存储 | SQLite / MySQL 配置，含可选 Redis 支持 | SQLite WAL，无需外部服务 |
| 前端组件 | Vue 3、Naive UI | Vue 3、shadcn-vue / Reka UI、Tailwind CSS 4 |
| 账号与访问 | 登录、多账号、管理员与公开访问模式 | 单用户工作区，免登录；访问认证由部署方提供 |
| 应用地址 | 内网与外网双地址、网络切换 | 每个应用一个地址 |
| 分组与排序 | 分组管理、拖动排序 | 保留分组管理；应用可直接跨组拖动并自动保存 |
| 图标与卡片 | 文字、图片、Iconify、网站图标；多种卡片样式 | 保留；支持每项独立卡片风格与图片引用保护 |
| 应用打开方式 | 当前页、新标签、网页小窗口 | 保留；小窗口提供地址与新标签入口 |
| 搜索、时钟、监控 | 应用 / 网页搜索、时钟与系统监控 | 保留；监控聚合采样、后台暂停和失效状态提示 |
| 壁纸与布局 | 自定义壁纸、模糊、遮罩、标题、布局与页脚 | 保留；默认页脚为空，内置默认壁纸及加载失败回退 |
| 界面偏好 | 中文 / 英文及主题设置 | 中文界面，亮色 / 暗色 / 跟随系统 |
| 删除与恢复 | 删除导航，无回收站页面 | 持久化回收站、删除批次校验、会话内撤销 |
| 导航导入导出 | 旧版 `.sun-panel.json` 导入导出 | 新版 `Sun-Panel-Navigation` 格式，导入前预览与版本校验 |
| 完整实例备份 | 本次对比源码未提供同等离线备份恢复命令 | 数据库一致快照、上传图片、SHA256 清单与空目标恢复 |
| 自定义 CSS / JS | 支持自定义静态文件 | 保留 `/custom/index.css` 与 `/custom/index.js` 加载入口 |
| Docker 部署 | Go 镜像与部署入口 | Rust 多阶段构建、非 root 运行、内置健康检查 |

旧 Go 后端、登录注册、账号管理、公告、验证码、演示测试页和旧数据迁移助手已移除。既有 Rust 数据库需要的 SQL 迁移历史仍保留。**旧 Go 数据库和旧版导航导出文件不能直接导入本版。**

## Docker 快速启动

在仓库根目录执行：

```sh
docker compose up --build -d
docker compose ps
```

打开 [http://127.0.0.1:3002](http://127.0.0.1:3002)。默认仅发布本机端口，数据保存在 Compose 的 `sun-panel-rust-data` 命名卷中。修改宿主端口：

```sh
SUN_PANEL_PORT=33008 docker compose up --build -d
```

查看日志和停止服务：

```sh
docker compose logs --tail=100 sun-panel-rust
docker compose down
```

`down` 保留数据卷；`down -v` 会删除卷。需要局域网访问时，修改 `docker-compose.yml` 中端口前的 `127.0.0.1`，并按实际访问范围配置网关认证。

单独构建指定名称的本地镜像：

```sh
docker build -t sun-panel-rust:1.0.0 .
```

以上镜像名用于本地构建，不代表 Docker Hub 中已存在同名公共镜像。镜像内置 `/api/health` 健康检查；CPU 和磁盘等监控反映服务运行环境，容器中不保证等同于完整宿主机数据。

## 配置与数据

| 环境变量 | 原生运行默认值 | 用途 |
|---|---|---|
| `SUN_PANEL_BIND` | `127.0.0.1:3002` | 监听地址；镜像内为 `0.0.0.0:3002` |
| `SUN_PANEL_DATA_DIR` | `./data` | 数据目录；镜像内为 `/app/data` |
| `SUN_PANEL_WORKER_THREADS` | `2` | Tokio 工作线程数 |
| `SUN_PANEL_DB_CONNECTIONS` | `2` | SQLite 连接数 |
| `SUN_PANEL_MONITOR_INTERVAL` | `5` | 监控共享缓存秒数 |
| `SUN_PANEL_ALLOW_PRIVATE_FAVICON` | `false` | 是否允许抓取私有网络网站图标 |
| `RUST_LOG` | `sun_panel_rust=info,tower_http=warn` | 日志级别 |

数据库、上传目录、上传大小等完整参数见 [配置说明](service-rust/README.md#配置)。不要让多个服务进程共用同一数据目录。

导航 JSON 只包含导航和图片引用。迁移整个实例或升级前，应停止服务并执行完整备份；不要把运行中的 `database.db` 单文件复制当作完整备份。命令和 Docker 示例见 [备份恢复](service-rust/README.md#完整备份恢复)。

## 开发与验证

使用 Rust `1.99.1`、Node.js `24.22.0` 和 pnpm `9.4.0`，以仓库工具链文件与锁文件为准。

```sh
pnpm install --frozen-lockfile
pnpm run build
cargo run --manifest-path service-rust/Cargo.toml --locked
```

前端热更新运行 `pnpm dev`，访问 [http://127.0.0.1:1002](http://127.0.0.1:1002)，API 代理到本机 `3002` 端口。

```sh
pnpm run lint
pnpm test
pnpm run build
cargo fmt --manifest-path service-rust/Cargo.toml --all --check
cargo clippy --manifest-path service-rust/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path service-rust/Cargo.toml --locked
```

容器回归使用独立临时数据，详见 [验证与构建](service-rust/README.md#验证与构建)。本次实测结果与覆盖率范围见 [1.0.0 验证记录](doc/verification-1.0.0.md)。测试、覆盖率和各架构镜像应以当前执行结果为准；本项目不承诺未经测量的跨语言性能提升。

## 版本与许可

更新记录见 [CHANGELOG.md](CHANGELOG.md)，本次 Release 正文见 [1.0.0 发布说明](doc/releases/1.0.0.md)。发布使用 `v1.0.0` 格式标签，标签版本必须与 `package.json`、`service-rust/Cargo.toml` 及对应发布说明一致。标签推送后工作流先验证，再创建 GitHub Release；配置镜像仓库变量与凭据后才推送多架构镜像，详情见 [发布流程](service-rust/README.md#发布流程)。

本项目在 [Sun-Panel](https://github.com/hslr-s/sun-panel) 的公开源码基础上修改，保留原作者红烧猎人的版权声明，遵循 [MIT License](LICENSE)。引入和修改的 shadcn-vue 组件许可见 [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt)。
