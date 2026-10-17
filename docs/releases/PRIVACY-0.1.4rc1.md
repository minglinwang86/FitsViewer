# Release privacy review / 发行物隐私检查

The public source archive and Linux wheel were checked before upload, including archive members, FITS examples, frontend assets and native/WASM binaries. Private marker checks supplement generic credential, private-network and personal-profile path rules. No matching private marker or credential was found in the released artifacts.

- Examples use generic paths and documentation hostnames. Runtime configuration is created on the installing user's own computer.
- Compiler source locations are remapped; the application discovers installed resources at runtime.
- Source archives normalize owner metadata. Only synthetic FITS examples are included.
- Local logs, authentication state, screenshots, runtime files, deployment notes and detailed internal reports are excluded.
- Dependency copyright notices and license authors' attribution remain intact.

Reproduce with `scripts/verify_privacy.py`; keep any private marker list outside the repository. This is a targeted prepublication check, not a guarantee that arbitrary future changes contain no private information.

中文：公开源码包与 Linux wheel 已检查目录、示例及二进制内容；未发现匹配的私人路径、连接标记或凭据。示例均使用通用提示，归档不包含本地日志、截图、认证状态、运行记录或观测数据。第三方许可证中的原作者署名保留。
