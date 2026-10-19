# 1.0.0 发布前验证记录

执行日期：2026-10-19。以下是当前工作树在 macOS arm64 与本机 Docker Linux arm64 环境中的实际结果；远端 GitHub Actions 和 Linux amd64 未在本轮运行。

## 测试与覆盖率

| 检查 | 结果 | 范围 |
|---|---|---|
| 前端行为测试 | 105/105 通过 | 请求生命周期、并发保存、导航移动/撤销、回收站、上传、反馈、组件渲染等 |
| 前端行覆盖率 | 1069/1079，99.07% | 14 个直接导入的源码 TS 模块和 1 个构建模块 |
| 前端分支 / 函数覆盖率 | 93.58% / 98.39% | 与上一行相同的统计范围 |
| Rust 单元与集成测试 | 75/75 通过 | 导航、持久化回收站、文件、监控、备份恢复、传输、CLI 和网站图标 HTTP 流程 |
| Rust 生产代码行覆盖率 | 2754/2919，94.35% | `src/*.rs` 的可执行行，排除集成测试和内嵌测试模块 |
| 静态检查 | 通过 | ESLint、Vue 类型检查、Rustfmt、Clippy `-D warnings` |
| 生产构建 | 通过 | Vue 前端和 Docker 内 Rust release 构建 |
| 容器回归 | 通过 | Linux arm64 的最终 `sun-panel-rust:1.0.0` 镜像 |

前端统计**不是整个前端覆盖率**：另有 61 个 TS 模块和全部 112 个 Vue 源文件未纳入源码计数；部分组件、store 和 hook 通过 VM/SSR 回归执行，但没有对应的源码映射计数。浏览器真实布局、拖放和跨浏览器行为不由这个覆盖率数字证明。

Rust 使用 Rust 1.99.1、cargo-llvm-cov 0.9.1 与 llvm-tools-preview。原始 LLVM HTML/LCOV/JSON 报告保留，`summary.json` 另外移除内嵌 `#[cfg(test)] mod tests` 的行，避免把测试代码计入生产覆盖率。此处只报告行覆盖率，不将它解释为分支覆盖率。

Rust 仍有未覆盖的 CLI 服务启动/信号处理、错误转换和部分异常 I/O 路径。网站图标测试使用本地 HTTP fixture，验证重定向、相对地址、回退和拒绝路径；不代表已验证任意公网网站。覆盖率较低不作为删除代码的依据。

## 可重复执行

在仓库根目录：

```sh
pnpm run test:coverage
rustup component add llvm-tools-preview --toolchain 1.99.1
cargo install cargo-llvm-cov --version 0.9.1 --locked
./service-rust/scripts/coverage.sh
SUN_PANEL_TEST_IMAGE=sun-panel-rust:1.0.0 pnpm run test:container
```

Rust 报告写入 `service-rust/target/coverage/`，其中 `html/index.html` 是可视化报告，`summary.json` 是生产行汇总；生成目录已被 Git 忽略。前端命令直接输出模块覆盖率表，未导入的模块不会自动计入分母。CI 调用相同的覆盖率入口，远端结果以实际运行日志为准。

## 清理与文档

- 引用检查确认前端 189 个 TS/Vue/CSS 模块均可从应用入口到达，19 组 UI 组件均有业务导入。
- 删除无调用的请求 GET/default 导出、闲置请求参数和全局 localStorage 清空方法。
- 旧 Go 服务、登录/公告/验证码兼容代码、演示页面、旧 UI 配置及原项目宣传/捐赠图片保持删除状态；另删除 10 份过期阶段报告，保留可重复运行的性能脚本。
- Rust Clippy 无未使用代码警告；保留仍在使用的模块、图标名兼容以及已应用的数据库迁移历史。
- README、CHANGELOG 与 Release 正文的本地链接通过；功能清单的 127 条源码路径存在。
- 发布脚本接受 `v1.0.0`，拒绝与项目版本不一致的标签；工作流 YAML 与 Compose 配置解析通过。

## 最终镜像

- 标签：`sun-panel-rust:1.0.0`
- 架构：`linux/arm64`
- 镜像 ID：`sha256:9c002654c54885394d3e0e44ff8ba7327f11ffae711780381d7a81f45c27cbfa`
- 本机 Docker 显示占用约 72.5 MB、内容约 17.1 MB，运行用户为 `10001:10001`。
- 使用依赖缓存、受限构建上下文和多阶段构建；前端构建执行类型检查，运行层保留非 root 与内置健康检查。
- 回归验证版本号、静态压缩与缓存、导航事务与冲突、配置保存、导入导出、图片引用保护、进程重启后的数据/回收站持久化、恢复与永久删除。

本轮没有提交或推送 Git、创建远端 Release 或向镜像仓库推送镜像。多架构发布配置已准备，Linux amd64 需要由对应 CI 作业验证。
