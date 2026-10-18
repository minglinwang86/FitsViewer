# Prerelease preparation / 预发布准备

Candidate package: `0.1.5rc1`; Git/Cargo/npm tag: `v0.1.5-rc.1`. This release prepares Full Linux and Windows distributions. Other profiles are not published. These instructions describe the maintainer's release procedure.

1. Run the build, strict metadata check and isolated wheel validation in [BUILDING.md](BUILDING.md).
2. Review both READMEs, license notices, support limits, changelog and [version scope](NEXT_RELEASE.md).
3. Prepare a clean source archive, wheel and checksums:

```bash
python scripts/prepare_release.py
```

The output is `release/0.1.5rc1/`. The source archive contains an allowlisted source tree, synthetic examples and license texts. It excludes credentials, runtime state, local deployment notes, observation files, historical reports, build tools and binaries. It is a GitHub source release, not a Python sdist; do not use `pip install` on this tarball. The wheel is the installation artifact.

Inspect the prepared source and wheel, including their embedded binaries. Use a private marker file kept outside the repository to check local usernames, paths, connection details and secrets. Third-party copyright and license attribution must remain intact.

```bash
python scripts/verify_privacy.py release/0.1.5rc1/*.whl release/0.1.5rc1/*-source.tar.gz --markers /path/to/private-markers.json
```

The source archive normalizes owner metadata; release builds remap compiler source paths. Do not upload local logs, unreviewed screenshots, runtime records or unreviewed validation JSON. Reviewed interface screenshots with public-data credits under docs/images are included in source releases. Public validation summaries contain generic test descriptions and synthetic inputs.

With the reviewed commit checked out and authenticated Git/GitHub CLI, the publication sequence is:

```bash
git push origin HEAD:main
git tag -a v0.1.5-rc.1 -m 'FitsViewer 0.1.5rc1'
git push origin v0.1.5-rc.1
gh release create v0.1.5-rc.1 --repo minglinwang86/FitsViewer --verify-tag --prerelease --title 'FitsViewer 0.1.5rc1' --notes-file docs/releases/v0.1.5-rc.1.md release/0.1.5rc1/*
```

Inspect remote branch state before pushing. Upload only the manifest-listed wheels, Windows Setup/portable assets, source tarball and `SHA256SUMS`. Pass repeated `--wheel` and `--asset` arguments to prepare_release.py when assembling both platforms. Verify the uploaded files' checksums and the public download links after publishing. Repository visibility is managed separately in GitHub settings.

PyPI/TestPyPI publishing is a later, separate scope. Prefer Trusted Publishing after the repository and workflow are configured. Before providing a Python sdist, implement and validate an independently buildable source package. No PyPI upload is performed by the candidate CI workflow.

中文：先构建、验证并扫描归档中的个人路径、连接信息及凭据，再生成干净发行目录。只上传清单中的安装包、源码包与校验文件，发布后核对下载内容。候选源码包不是 pip 可安装的 sdist，用户安装 wheel。
