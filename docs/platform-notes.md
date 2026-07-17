# KView 平台差异与已知限制

## 支持基线

- Windows 10/11 x64：使用系统 WebView2；NSIS 默认按当前用户安装，无需管理员权限。
- macOS 12 及以上：使用系统 WKWebView；应用包、图标、文件关联和 LaunchAgent 开机启动已配置。
- KView 不内置浏览器运行时，最终渲染和媒体解码能力会随系统 WebView 版本变化。

## 已知差异

- Windows 文件打开由单实例参数转发处理；macOS 额外处理系统 `Opened` 文件事件。
- 托盘菜单在 Windows 与 macOS 的排列和系统图标风格可能不同，功能口径一致。
- 当前 M3 按 Windows 范围完成人工验收；macOS 安装、升级、卸载和文件关联留待需要发布 macOS 版本时复验。
- Windows 已分别验证极速和低内存模式的 `--background` 静默启动，均不会误弹主窗口，并能通过后续文件打开恢复或重建唯一窗口。
- Windows release 安装版中，极速模式保留 1 个隐藏 WebView 窗口；该 WebView2 进程组由 1 个浏览器进程、1 个 renderer 及 GPU/utility/crashpad 等系统子进程组成。低内存模式关闭窗口后不存在 KView 所属的 WebView2 子进程。
- Windows 默认交付 NSIS 当前用户安装包；覆盖安装会保留首次安装前的文件关联备份，卸载后清理 KView 文件类、备份值和开机启动项。

## MVP 限制

- 文本仅支持 UTF-8、UTF-8 BOM、UTF-16 LE/BE BOM；其他本地编码会显示明确错误。
- 超过 20 MiB 的文本只读展示前 2 MiB；图片安全上限为 100 MiB。
- SVG 会移除脚本、事件属性和 `foreignObject`，因此复杂 SVG 的外观可能变化。
- PDF、Office 和视频能力分别在后续里程碑交付；当前关联元数据为后续版本预留。
- 本地诊断日志只包含时间、错误类别、错误代码和耗时，不包含文件路径或文件内容。
