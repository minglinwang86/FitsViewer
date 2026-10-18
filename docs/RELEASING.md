# Release preparation / 发行准备

Candidate package: `0.1.6`; Git/Cargo/npm tag: `v0.1.6`. This release prepares Full Linux/Windows distributions and native macOS Apple Silicon/Intel portable apps. Other profiles are not published. These instructions describe the maintainer's release procedure.

1. Run the build, strict metadata check and isolated wheel validation in [BUILDING.md](BUILDING.md).
2. Review both READMEs, license notices, support limits, changelog and [version scope](NEXT_RELEASE.md).
3. Prepare a clean source archive, wheel and checksums:

```bash
python scripts/prepare_release.py
```

The output is `release/0.1.6/`. The source archive contains an allowlisted source tree, synthetic examples and license texts. It excludes credentials, runtime state, local deployment notes, observation files, historical reports, build tools and binaries. It is a GitHub source release, not a Python sdist; do not use `pip install` on this tarball. The wheel is the installation artifact.

Inspect the prepared source and wheel, including their embedded binaries. Use a private marker file kept outside the repository to check local usernames, paths, connection details and secrets. Third-party copyright and license attribution must remain intact.

```bash
python scripts/verify_privacy.py release/0.1.6/*.whl release/0.1.6/*-source.tar.gz --markers /path/to/private-markers.json
```

The source archive normalizes owner metadata; release builds remap compiler source paths. Do not upload local logs, unreviewed screenshots, runtime records or unreviewed validation JSON. Reviewed interface screenshots with public-data credits under docs/images are included in source releases. Public validation summaries contain generic test descriptions and synthetic inputs.

With the reviewed commit checked out and authenticated Git/GitHub CLI, the publication sequence is:

```bash
git push origin HEAD:main
git tag -a v0.1.6 -m 'FitsViewer 0.1.6'
git push origin v0.1.6
gh release create v0.1.6 --repo minglinwang86/FitsViewer --verify-tag --draft --title 'FitsViewer 0.1.6' --notes-file docs/releases/v0.1.6.md release/0.1.6/*
```

Inspect remote branch state before pushing. Upload only the manifest-listed wheels, Windows Setup/portable assets, macOS portable ZIPs, source tarball and `SHA256SUMS`. Pass repeated `--wheel` and `--asset` arguments to prepare_release.py when assembling platforms. Verify the uploaded files' checksums and the public download links after publishing. Repository visibility is managed separately in GitHub settings.

For macOS, run the native workflow before publication. Its locked Linux wheel supplies
only platform-independent resources; preserve the reviewed asset ID/hash while it is
used by the workflow. If a draft release already holds those assets, upload the remaining
reviewed files and update that draft instead of creating a duplicate release. macOS
validation evidence and signing limitations belong in the public validation summary.

The native macOS workflow needs an exact reviewed resource wheel before building.
Upload that wheel to the draft, update `config/macos-full-assets.json` with the
actual release/asset identifiers, byte count and wheel/WASM hashes, then dispatch
the native workflow from the reviewed commit. Its two macOS runners must finish
their actual platform checks before the assets join the release manifest. Prepare
the final source archive and checksums after the resource lock is committed.
Publish the existing reviewed draft with `gh release edit v0.1.6 --draft=false`;
do not recreate it or silently change the resource wheel while CI is using it.

PyPI/TestPyPI publishing is a later, separate scope. Prefer Trusted Publishing after the repository and workflow are configured. Before providing a Python sdist, implement and validate an independently buildable source package. No PyPI upload is performed by the candidate CI workflow.

中文：先构建、验证并扫描归档中的个人路径、连接信息及凭据，再生成干净发行目录。只上传清单中的安装包、源码包与校验文件，发布后核对下载内容。候选源码包不是 pip 可安装的 sdist，用户安装 wheel。
