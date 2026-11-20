# affectUI 公开预览参考

参考内容保存在 `.local-cache/affectui/`，目录已被 Git 忽略。184 个公开 HTML 页面、779 份脚本与样式依赖及 8 份字体文件保留原有目录结构，总计约 13MB。

## 打开预览

- 文件夹首页：[Files](http://127.0.0.1:5198/screens/files)
- 项目内部：[Folder](http://127.0.0.1:5198/screens/folder)
- 页面目录：[Screens](http://127.0.0.1:5198/screens)

启动命令：

```bash
env PATH=/opt/homebrew/opt/node@22/bin:$PATH node .local-cache/affectui/serve.mjs
```

## 布局记录

- Files：文件夹网格使用名称与文件数量两行信息。卡片高度 59px，圆角 12px；网格间距 11px。进入文件夹后显示其中的文件。
- Folder：页首显示面包屑；文件夹标题与操作位于同一行。文件网格与标题间隔 20px，文件间距 12px。窄窗口下操作换行。
- 文件卡片：预览、文件名称和类型信息组成一个入口，名称与信息保持相同起点。
- 文件夹导航与页面内容共享当前文件夹名称。

Studio 继续使用现有 shadcn 组件、图标和语义颜色。

## 运行检查

`.local-cache/affectui/check-preview.mjs` 检查四个文件夹、进入 Q3 Campaign 后的五个文件、返回、搜索、网格与列表切换，以及亮暗主题切换。

`.local-cache/affectui/check-screens.mjs` 检查 15 个页面的完整预览，均渲染成功，脚本和样式请求没有失败。

检查结果分别保存在 `preview-check.json` 与 `screens-check.json`。
