# Standalone SSH viewing / 普通 SSH 查看

`fver` starts or reuses an independent FitsViewer HTTP service. It does not require
Jupyter or X11 forwarding. Run it **on the remote machine** with one explicit file:

```bash
fver image.fits --public-url http://127.0.0.1:18877
fver image.fits.gz --hdu 1 --public-url http://127.0.0.1:18877
```

The remote service listens on loopback port `8877` by default. `--public-url`
describes the address your browser can actually reach; it does not create a
forward. On the local computer, first provide an SSH local forward, for example:

```bash
ssh -N -T -L 127.0.0.1:18877:127.0.0.1:8877 user@server.example.com
```

An existing SSH connection with that forwarding rule can carry the traffic. Plain
SSH without a forwarding rule cannot acquire a new local port or control your
browser from the remote process. The command prints a clickable URL immediately;
open it on your local computer. The URL has a one-use ticket and expires after
60 seconds. If it expires, run `fver` again for a fresh URL.

`fver` 在远端启动独立服务，先输出可点击地址，不等待大文件传输。首次需要本机打开
该地址；已有窗口可接收同一 target 的后续请求。已存在的端口转发可以复用，普通 SSH
进程本身不能控制本机浏览器。SSH 内使用回环 HTTP；HTTPS 需要用户已有、正确配置的
受控网关与认证，`--public-url` 不会安装 TLS 或创建公网访问入口。

## Choose a destination / 选择窗口目标

```bash
fver image.fits --target science --public-url http://127.0.0.1:18877
fver other.fits --target science --public-url http://127.0.0.1:18877
fver image.fits --route browser --target science --public-url http://127.0.0.1:18877
```

The default target is `default`. Target names contain 1–64 letters, digits,
underscores or hyphens. Separate targets prevent two workflows from sharing a
delivery destination. `auto` tries a paired, online helper when available, and
otherwise uses the browser route. A failed native attempt falls back to the
browser route for the same request. The CLI's request ID can be inspected:

```bash
fver status REQUEST_ID --target science
fver cancel REQUEST_ID --target science
```

“Queued”, “claimed” and “first frame” are separate states. Starting a child process
or claiming a request does not establish that an image was drawn. Cancellation
invalidates delivery and releases that request's owners; closing an unrelated
window must not release its files or tasks.

## Optional local helper / 可选本机助手

Install the Full package on both machines for native windows. On the **remote**
machine, issue a short-lived pairing code:

```bash
fver pair --target science --public-url http://127.0.0.1:18877
```

On the **local** computer, start the helper and paste that code at its hidden
prompt. Do not put the code in shell arguments, public logs or screenshots:

```bash
fver helper --url http://127.0.0.1:18877 --target science --mode native
```

The helper accepts a direct local loopback connection. It launches the fixed
FitsViewer executable with a bounded stdin protocol; it does not execute commands
received from the remote service. Keep it running for subsequent `fver` requests.
Ctrl+C closes only its owned native children and any tunnel it created. Browser
tabs manage their own lifetime.

The macOS portable includes a `./fver` wrapper beside `./fitsviewer`. It reuses the
same helper Python sources and needs an external Python 3.10+; choose one with
`FITSVIEWER_PYTHON=/path/to/python ./fver ...`. No interpreter is bundled. Copying
only the `.app` without the adjacent wrapper retains the application; keep the
portable directory together to use its terminal wrappers.

Alternatively, let the helper create its own SSH forward through a trusted system
SSH host alias:

```bash
fver helper --ssh my-astro-server --remote-port 8877 --target science --mode native
```

System SSH handles login, host keys and forwarding policy. This option creates an
owned tunnel; it does not modify an unrelated running SSH process. A helper
without native desktop support can use `--mode browser`. Pair again after a server
instance changes. An unpaired machine still has the basic clickable-URL route.

## Small and large images / 大小图传输

| Data | Transfer |
| --- | --- |
| Selected HDU's complete decoded packet ≤ `20_000_000` bytes | Transfer the full typed packet once; ordinary pan, zoom and display changes then use local data. |
| Larger supported two-dimensional image | Transfer a bounded preview and visible numeric tiles; local stretch/brightness changes reuse cached values. |
| Precise cursor value | Read the original sample through a separate pixel request; the preview is not used as the measurement. |
| Scientific operations | Run against original files in the selected external Python environment, subject to that operation's normal bounds. |

The threshold includes metadata and spectral coordinate/error arrays, not just raw
pixels. Compressed disk size is not the threshold. Ordinary FITS can use seeked
tiles; gzip needs bounded disk preparation. Tiled FITS compression currently needs
a bounded decoded cache. Very large spectra, unsupported compression or a format
preparation limit return an explicit error. Native direct-file opening retains its
normal reader limits; the progressive route concerns HTTP-backed viewing.

The client estimates a 40 MiB tile cache per active remote image, admits four tile
fetches at a time and requests at most twenty visible tiles. The host bounds decoded
preparation to 192 MiB, gzip spool to 1 GiB and admitted image operations to sixteen.
These are component budgets, not a total browser/RSS/GPU memory guarantee. Cached
preview statistics are labelled as sampled. A cold zoom or distant pan may need
network access; cached display adjustments do not wait for it.

## Data and access boundaries / 数据与访问边界

- A browser grant covers files explicitly registered for that request, not the
  containing directory or the server's control credential. Environment changes,
  arbitrary uploads and arbitrary URL downloads are separate capabilities.
- Pairing, browser bootstrap, helper delivery and artifact links use distinct,
  expiring credentials. Treat a fresh URL as private while its ticket is valid.
- File identity and revision are checked. Replacing or modifying a source requires
  a fresh request; a stale callback must not change the current document.
- BSCALE/BZERO, BLANK and nonfinite masks remain scientific data conventions.
  Display scaling/rotation and low-resolution previews never rewrite the source.
- FitsViewer's coordinates, source identifications, fluxes and derived values are
  reference results. Validate them independently before scientific publication.

See [commands](COMMANDS.md), [API](API.md), [architecture](ARCHITECTURE.md) and
[resource ownership](RESOURCE_LIFECYCLE.md) for implementation contracts. Platform
acceptance and limitations belong in the release validation record; this guide
does not by itself establish a successful helper/browser or scientific test.
