# AGENTS.md
KView 是一款面向开发者和高级用户的现代化跨平台桌面文件查看与轻量处理工具。结合极速预览能力与对代码/文本/媒体/文档的深入理解，无需启动大型庞杂应用即可快速打开、理解和处理本地及远程文件。

## 主要技术栈
- 桌面底层：Tauri 2 (Rust 2021)
- 前端核心：Vue 3 (Composition API / `<script setup>`) + TypeScript + Vite
- UI 与样式：Nuxt UI + Tailwind CSS + Lucide Icons
- 状态管理：Pinia
- 核心查看器与解析器：Monaco Editor (代码与文本)、ArtPlayer (音频与视频)、PDF.js (PDF 预览)、docx-preview (Word 预览)、SheetJS/xlsx (表格)、markdown-it (Markdown 安全渲染)、@lingo-reader/mobi-parser & rbook (电子书)
- 原生系统能力 (Rust)：office_oxide (Office 文档解析)、zip / tar / flate2 (压缩包解析)、msi / pelite (安装包元数据提取)、hpvcd (动态照片解析)、windows-sys (Windows 系统快捷键与资源管理器交互)
- 工具链与包管理：VitePlus (`vp`)、pnpm、Cargo

## 仓库代码结构
- `src/`: 前端 Vue 3 核心源码
  - `components/`: 应用外壳、工作区及通用交互组件 (AppShell, Workspace, ViewerFooter, SettingsModal 等)
  - `viewers/`: 各类文件格式专用 Viewer 组件与按需懒加载注册表 (TextViewer, HexViewer, ImageViewer, PdfViewer, DocxViewer, XlsxViewer, EbookViewer 等)
  - `services/`: 前端核心服务，封装 Tauri IPC 通信 (desktop-api)、文件与资源载入 (document-loader)、桌面生命周期
  - `state/`: Pinia 响应式状态管理 (workspace, document, library, settings, updates)
  - `domain/`: 格式识别与领域模型定义 (formats)
  - `types/`: 全局 TypeScript 类型契约
  - `utils/`: 辅助工具方法 (binary, url 等)
- `src-tauri/`: Tauri 2 / Rust 后端源码与桌面配置
  - `src/`: Rust 系统模块，负责单实例管理、文件读写、空格预览监听、文件格式解析与更新器
  - `Cargo.toml`: Rust 依赖与构建配置
  - `tauri.conf.json`: Tauri 应用窗口与能力配置
- `shared/`: 前后端共享元数据与配置 (`formats.json`)
- `scripts/`: 发布与工程维护脚本 (`release.mjs`)
- `docs/`: 性能基线与更新验证文档

## 相关命令
- `vp install`: 安装项目与前端依赖
- `vp dev`: 启动前端 Vite 开发服务器
- `vpr tauri:dev`: 启动桌面应用开发环境并关联前端
- `vp check`: 执行格式检查、Lint 与 TypeScript 类型校验
- `vp build`: 构建前端静态产物
- `vpr tauri:build`: 构建生成跨平台桌面端生产分发包
- `vpr release:validate`: 校验发布规范与版本一致性

## 系统约定

### 基本原则

- 外部输入必须在系统边界完成校验和规范化。
- 内部 Service 默认接收到的数据已经满足约定和不变量，不重复校验。
- 优先 fail fast，不通过默认值、fallback 或吞异常掩盖错误。
- 一个模块只负责一个明确职责，避免把异常处理扩散到多个层级。
- 不为假想场景增加防御性代码。

### 系统边界 system boundary

以下数据视为不可信输入，应在进入系统时校验：

- 外部文件路径、拖拽文件、命令行启动参数及系统打开方式输入；
- 远程公开 URL 地址、剪贴板数据及用户输入；
- Tauri IPC 前后端通信交互参数（Tauri Command 入参与前端 invoke 返回值）；
- 本地文件原始二进制、编码及不受信任的第三方外部文件流；
- 外部归档、Office/PDF 文档、安装包等文件解析器的解析产物。

Tauri Command 与前端 Service 优先使用强类型结构与校验（Rust serde 反序列化 / TypeScript 类型契约）完成结构和合法性验证。

进入业务 Service 后，默认输入已经合法。

例如：

外部文件 / 用户操作输入
    ↓
系统边界校验与规范化 (Rust / IPC / Service)
    ↓
业务Service / Pinia Store
    ↓
视图展示 (Viewer / Component)

不要出现：

UI 组件校验
    ↓
Store 再校验
    ↓
Service 再校验
    ↓
IPC 调用再校验

### 边界处理 edge case

只有以下情况允许新增边界处理：

1. design 明确定义；
2. 已有测试要求；
3. 外部系统契约要求；
4. 已知真实线上问题。

除此之外，不主动增加推测性的：

- `if` 分支；
- fallback；
- 默认值修复；
- 自动纠错；
- 容错转换；
- 重试逻辑。

### 立即失败 fail fast

发现违反契约或不变量的情况时，应立即失败。

不要：

- `try...catch` / `catch` 后吞掉异常返回空对象；
- 返回 `null` / `undefined` 掩盖真实错误；
- 在 Rust 中使用 `.unwrap()` 或静默忽略 `Err`；
- 使用默认值继续执行；
- 记录日志后静默忽略异常。

只捕获当前层能够明确处理的异常。

无法处理的异常应继续向上传递，Rust 统一转换为 `CommandError`，前端统一由错误状态或 `ErrorState` 展现。

### 回退 fallback

默认禁止静默回退。

允许回退时，必须有明确依据，例如：

- design 明确定义降级策略；
- 第三方服务契约要求；
- 已知生产故障的恢复方案。

回退必须是可观察的，至少应有日志或指标，不允许静默发生。