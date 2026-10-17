# Contributing / 参与开发

Start with [BUILDING.md](docs/BUILDING.md) and the [API contracts](docs/API.md). Keep FITS numeric interpretation separate from display transforms. Preserve raw pixel types, apply BSCALE/BZERO once, and keep viewing read-only.

Use synthetic fixtures for reproductions. Parsing/display changes need a relevant numeric or interaction check; installation changes should pass the isolated wheel check. Include Python, JupyterLab, OS, architecture and browser versions in bug reports. Omit tokens, private paths and observational data unless authorized to share them.

Open an issue or pull request in this repository once published. Use its security reporting channel for vulnerabilities. Contributions are supplied under the project's MIT source license, with third-party notices preserved.

中文：先阅读构建说明和 API 合约，使用合成数据复现问题。保持只读查看、原始类型和 FITS 数值解释；运行修改对应的验证。提交问题时附环境和复现步骤，不附登录 token 或未经授权的科学数据。
