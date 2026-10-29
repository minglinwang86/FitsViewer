# FitsViewer Mini for phones / 手机极简版

This target compiles the shared core/viewer with only the registry's `base`
module and the `offline` feature. It includes no PRO workspace, native server,
Python interpreter or scientific Python packages. Files selected on the device
are read in its local WebAssembly viewer.

手机目标按模块注册表只构建 `base`，启用 `offline` 本地读取；不包含 PRO、后端服务器、
Python 或科学软件栈。选择的文件在设备本地处理。

| Artifact / 产物 | Use / 用途 |
| --- | --- |
| Android debug-signed APK | Sideload test build. This is not a Play Store release; CI debug signing does not promise upgrade compatibility with a later package. / 安卓侧载测试包，非应用商店发行版，后续签名不同可能需要卸载旧测试包。 |
| iOS `UNSIGNED.app.zip` | Native device build for subsequent valid Apple signing and provisioning. It cannot be installed as supplied. / iPhone 原生未签名构建，需合法 Apple 签名及配置后才能安装，不能直接安装此归档。 |
| Offline web/PWA files | Host the generated site using HTTPS, then open it in Safari or Chrome; supported browsers can add it to the Home Screen. / 将站点部署到 HTTPS 后，在 Safari 或 Chrome 打开并添加到主屏幕。 |

The iPhone simulator build is separate from the unsigned device build. Simulator
startup is not a claim that the device package has been signed or tested on a
physical iPhone. See the workflow's actual evidence before distributing a build.

iPhone 模拟器版本与设备版本分别构建。模拟器启动不代表真机安装或所有文件提供者已经
验证；发包前须查看对应 CI 的实际结果。

Use the system file picker to open FITS, supported gzip/FITS compression and basic
spectra. The toolbar's `−`, `+` and `Fit` controls provide touch-friendly zoom;
pinch zoom is not currently supported. Native file associations are not registered.
The icon belongs to the application.

使用系统文件选择器打开图像或基础光谱，通过 `−`、`+`、`Fit` 缩放；当前不支持双指
捏合。只设置应用图标，不为关联 FITS 文件设置图标。

The file and gzip-expanded input limit is 64 MiB. This is not a total decoded-array
or application-memory guarantee; internally compressed images and device limits
still matter. Original files are not rewritten. Scientific values are for reference
and require independent validation before scientific publication.

文件及 gzip 展开输入上限为 64 MiB；此值不代表所有内部压缩图像的总解码内存或程序
内存上限，设备资源仍可能限制打开能力。程序不改写原文件。坐标、流量、星等等数值仅
建议作为参考，论文数据需要独立验证与误差评估。

Builds run through `.github/workflows/mobile-mini.yml`. `mobile/build.py` has a dry
plan by default; compilation requires `--execute`. Native wrappers consume the
same hash-verified Mini web artifact, with its font and original license texts.
