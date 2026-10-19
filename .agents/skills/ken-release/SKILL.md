---
name: ken-release
description: 当用户手动触发，禁止自动执行！用户更新项目版本，更新README、CHANGELOG等文件，不要进行任何的 git 操作！

# deepseek
user-invocable: true # 可选，true 才能被 /name 触发

# vscode 指定模型
model: deepseek-flash-sd (customendpoint)
---

# 发布更新流程

用户更新项目版本，更新README、CHANGELOG等文件，不要进行任何的 git 操作！

## 工作流程

**优先检查是否有代码变更，如果没有变更，此SKILL的流程立即结束。**

1. 检查上一个版本号的代码commit记录，总结自上一个版本以来的所有变更。
2. 修改项目版本号。例如前端项目应该变更 `package.json` 中的版本号。
3. 检查是否存在 `CHANGELOG.md` 文件。如果存在就将新版本中所有变更总结到 `CHANGELOG.md` ，否则总结到 `README.md` 。

## 格式规范示例

在 `CHANGELOG.md` 或 `README.md` 文件中有一个二级标题（变更日志），用于描述变更内容。每一个变更项都必须以 `### vX.Y.Z (YYYY-MM-DD)` 格式开头。

```md
### v1.2.3 (2024-01-01)

- 通过检查 `package.json` 中的版本号变更，自动化生成提交信息。
- 版本号变更时自动打标签并推送标签到远端。
- 通过使用 Emoji 增强提交信息的可读性。

```
