# Next release tasks / 下一版本更新任务

Version: to be decided. An unchecked item is planned work, not an implemented feature.

版本号待定；未勾选项目表示待完成，不能据此视为当前版本已支持。

## FV-NEXT-001: Chinese paths / 中文路径支持

**Status: open. Priority: high. Reported: 2026-10-17.**

Windows users report garbled Chinese text when pasting a FITS path. Add Chinese path support across the shared desktop/browser viewer, and verify the Linux, JupyterLab and Notebook entry points before the next release.

Windows 用户反馈：粘贴中文路径时显示乱码。下一版本需要完善中文路径的显示、粘贴和打开，并覆盖 Linux、JupyterLab、Notebook 入口。

### Investigation / 调查结果

The checked source baseline is `db35565`. Windows runtime checks used the retained `0.1.4rc2` wheel; its published release has been withdrawn. These checks do not constitute a new release or a completed fix.

源码检查基线为 `db35565`。Windows 实测使用本地留存的 `0.1.4rc2` wheel；对应公开发行版已撤下。本记录属于问题调查，尚未修复，也未发布新的安装包。

The synthetic fixture is a 32 × 32 signed 16-bit image with pixel values 0–1023. Paths include Chinese directory/file names, spaces and full-width parentheses: `中文数据/观测 星场（测试）.fits` and the equivalent `.fits.gz` file.

| Entry / 入口 | Evidence / 证据 | Result / 结果 |
|---|---|---|
| Windows native command-line file argument / Windows 原生命令行参数 | Two real render/close runs, ordinary FITS and gzip FITS / 两次实际渲染并关闭 | Loaded with a texture, no error, exact Unicode path preserved / 图像加载成功，中文路径完整 |
| Windows native path field / Windows 原生路径输入框 | Real window inspection and Ctrl+C/Ctrl+V round trip / 实际窗口及复制粘贴检查 | Chinese glyphs render as boxes; the internal string remains correct and the test image loads / 中文显示为方框，内部字符串正确，测试图像可加载 |
| Windows browser backend API / Windows 浏览器后端接口 | HTTP requests with encoded Chinese paths / 中文路径编码请求 | Both FITS forms return HTTP 200; decoded path and every pixel match / 两类文件均返回 200，路径和全部像素匹配 |
| Quoted path at the backend API / 后端接口的带引号路径 | Same relative path enclosed in literal double quotes / 同一路径外加字面双引号 | HTTP 404. GUI input currently forwards the original string, so pasted quotes need dedicated handling / 返回 404；界面当前原样传递输入，需处理粘贴的外层引号 |
| Linux native viewer / Linux 原生窗口 | Source review / 源码检查 | Shares the egui UI and default fonts with Windows; Chinese display is at risk. Not runtime-verified in this investigation / 共用界面和默认字体，有同类显示风险；本轮未完成实机验证 |
| Browser viewer, JupyterLab and Notebook / 浏览器、JupyterLab、Notebook | Source review / 源码检查 | Canvas UI shares the default fonts. URL paths use percent encoding, `URLSearchParams` or Python `urlencode`; end-to-end Chinese-path and clipboard checks remain open / 画布共用默认字体；路径已有 URL 编码，但中文路径和剪贴板端到端验证待完成 |

The shared UI enables egui default fonts without registering a Chinese font fallback. This matches the observed missing glyphs. The inspected Windows file picker uses the Unicode `GetOpenFileNameW` API, and file loading uses Rust paths. The successful tests show that missing glyphs and actual path corruption must be investigated separately; they do not prove every clipboard source or input method works.

共享界面只启用了 egui 默认字体，未注册中文字体回退，与实测中文字形缺失一致。Windows 文件选择器使用 Unicode API，底层按 Rust 路径读取。已有测试说明应分别处理字形显示与路径损坏，不能据此认定所有剪贴板来源或输入法均已兼容。

The additional quoted-path GUI case, Linux GUI, browser rendering and Jupyter/Notebook Chinese-path runtime cases were not completed in this investigation. Earlier general Unicode filename checks covered file loading, not Chinese glyph rendering or external clipboard paste.

额外的带引号路径界面测试，以及 Linux 图形界面、浏览器渲染和 Jupyter/Notebook 中文路径实测仍待补充。历史 Unicode 文件名测试只覆盖文件读取，未验证中文字形与外部剪贴板粘贴。

### Required work and acceptance / 修复与验收要求

- [ ] Register a licensed Chinese font fallback for both native egui and WebAssembly. Verify Chinese path, file name, HDU label and error display; record the font license, package-size impact and browser loading behavior. / 为原生与 WASM 界面配置有许可的中文字体回退，验证路径、文件名、HDU 名称与错误信息显示，并检查包大小和加载行为。
- [ ] Preserve Unicode throughout clipboard input, arguments, file selection, drag/drop, filesystem access and URL transport. Inspect Windows clipboard Unicode handling; do not add an ANSI or locale-dependent conversion. / 检查剪贴板、命令行、选择文件、拖放、文件系统和 URL 全链路 Unicode 传递，避免引入 ANSI 或依赖本地代码页的转换。
- [ ] Handle a recognized pair of outer quotes in pasted Windows paths, including Explorer's **Copy as path** format. Preserve valid file-name characters and meaningful spaces; do not indiscriminately trim or normalize filesystem names. Report an invalid path legibly. / 处理资源管理器“复制文件地址”形式的外层引号，保留合法文件名字符及有意义的空格，避免任意裁剪或 Unicode 规范化；错误提示需可读。
- [ ] Test ordinary FITS and `.fits.gz` files with Chinese directories, Chinese file names, spaces and full-width parentheses. Compare the received path, loaded dimensions and pixel values, in addition to inspecting the rendered text. / 验证中文目录、文件名、空格、全角括号，除观察字形外，还需核对路径、尺寸和像素。
- [ ] On Windows, test external Ctrl+V and Shift+Insert, quoted/unquoted absolute paths, file selection, drag/drop, file association double-click and CLI opening. Include a mixed Chinese/English path and an invalid Chinese path. / Windows 验证外部粘贴、带引号与不带引号的绝对路径、选择文件、拖放、关联双击和命令行，包含中英文混合路径及无效中文路径。
- [ ] On Linux, test native desktop file opening and clipboard input under a UTF-8 locale, plus browser access over SSH. Verify X2Go clipboard behavior in an actual forwarded session before claiming X2Go compatibility. / Linux 验证 UTF-8 环境下的原生打开与粘贴，以及 SSH 浏览器入口；X2Go 剪贴板兼容需实际转发会话验证。
- [ ] In JupyterLab, double-click both Chinese-named FITS forms in the file browser. Test Notebook `fitsviewer.show(...)`, a Jupyter URL prefix and clipboard entry inside the viewer. Check the viewer's internal path and loaded image, not only the tab title. / JupyterLab 验证文件列表双击两类中文文件、Notebook 内嵌、URL 前缀和查看器内粘贴，并检查内部路径与图像。
- [ ] Save reproducible results for the next release and mark this task complete only after the required cases pass. / 保存可复现的验收结果，全部必要用例通过后再关闭任务。
