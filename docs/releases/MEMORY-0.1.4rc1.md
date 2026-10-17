# Memory validation / 内存验证

Candidate: 0.1.4rc1. Date: 2026-10-17. Synthetic data and isolated Jupyter/browser processes.

Ordinary FITS and gzip FITS were tested independently. Each run uses six 2048x2048 float32 images (16 MiB raw pixels per image), eight simultaneous open/close cycles, twelve single-file iterations, eight closes during startup and one warm-up. Closed-state measurements include garbage collection and a short wait. The browser is driven through CDP without Network, Debugger or Performance instrumentation.

| File type | Verified loaded-document teardown | Surviving WASM memories | Cycle 8 open PSS / MiB | Cycle 8 closed PSS / MiB | Cycle 1 to 8 closed change / MiB |
|---|---:|---:|---:|---:|---:|
| fits | 61/61 | 0 | 985.72 | 429.21 | +50.49 |
| fits.gz | 61/61 | 0 | 976.79 | 410.46 | +41.61 |

All tested loaded WebGL contexts were explicitly released. Closed documents left no viewer iframes, no surviving WebAssembly.Memory instances and no JavaScript exceptions. No persistent FITS pixel/WASM resource leak was found in these bounded tests. Remaining process-memory growth is not fully attributed; this is not a promise of zero growth for every input, browser or long-running session.

The Rust WebRunner has an owned lifecycle and an exported stop function. The parent document's disposed signal performs teardown before DOM detachment. Teardown destroys rendering callbacks, drops application/image ownership, releases the WebGL context, stops popup polling and disconnects document signals. Closing during asynchronous startup is handled as well. Additional instrumented lifecycle tests before the privacy rebuild found zero surviving viewer widgets, contents and iframes; the lifecycle implementation is unchanged.

Browser/framework caches, compiled code and allocator reserves can retain process memory after document objects are released. Earlier instrumented runs retained more process memory than the lightweight CDP run, and an ordinary text-editor control also grew. Process PSS alone must not be interpreted as retained FITS data.

Reproduce after installing the candidate in a separate environment:

```bash
python scripts/verify_memory.py --python /path/to/env/bin/python --wheel /path/to/candidate.whl --cycles 8 --lightweight-cdp --uncompressed
python scripts/verify_memory.py --python /path/to/env/bin/python --wheel /path/to/candidate.whl --cycles 8 --lightweight-cdp
```

Raw measurements: [CSV](MEMORY-0.1.4rc1.csv). Maximum-size images, Safari, other graphics drivers and long-running sessions remain unvalidated.

中文：普通 FITS 与 gzip FITS 各自完成 61 次正常关闭与 8 次加载中关闭；WASM 存活数均为 0，已加载页的图形上下文全部释放。已补齐主动释放及窗口销毁顺序。整体进程内存仍可能保留；不能据此保证所有环境长期运行绝无增长。

Measured wheel SHA256: `0e0083fec5e858cf9e7c5f0c93f6fb04ddbfd5aabd42f1ec055fdf69de858978`.
