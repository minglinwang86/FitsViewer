# Resource ownership / 资源生命周期

This describes the current source resource contract, including 0.1.6 additions.
[Historical 0.1.5rc1 validation](releases/VALIDATION-0.1.5rc1.md) applies to its
own artifacts, not a later source checkout. These bounds do not guarantee fixed RSS.
本文描述当前源码及 0.1.6 的新增资源约定；历史验收只适用于对应产物，上限不是整个进程 RSS。

## Documents, roles and jobs

Each viewer has a session identifier. New GUI scientific requests carry a
document generation and a separate PRO or Gaia role epoch. File replacement and
cached-tab selection release the old document; late replies cannot update the
new one. Closing the view releases all of that view's roles. Switching to Minimal
releases PRO while retaining the independent Gaia role and referenced data.
Folding, resizing or floating panels changes presentation only.

每个查看页有独立 session。文档 generation 和 PRO/Gaia epoch 分别阻止旧请求重新启动
任务、旧结果污染新图。换图与缓存标签切换共用文档释放；关闭查看页释放其全部角色。
收起、浮动、改变大小保留同一个控制器与任务。

Live leases expire after 180 seconds without a heartbeat, including when page
teardown cannot reach the server. The background reaper does not require another
viewer request. Closed-generation tombstones last up to 30 minutes; the session
table is capped at 4096. Legacy CLI requests can omit generation/epoch metadata.

Dependency probe/install jobs belong to the base View role through start, status
and cancellation. Another window releasing PRO or its own document cannot stop
them. An idle worker is retired only after active jobs and undelivered completed
results have been handled. Switching the external Python is refused while either
is present, to avoid losing a completed installation's log/result.

The worker permits 32 active/queued jobs. Completed futures are retired into
bounded JSON history: at most 32 results and 32 MiB total, with 128 lightweight
tombstones. An evicted result returns `state:consumed`; it is not resubmitted or
silently applied. Status and cancellation check the owner. Cancellation is
cooperative for external scientific routines; core-packet extraction additionally
polls cancellation and has a 60-second deadline.

## Temporary scientific results

Each host uses a unique marked subdirectory under its runtime cache. Python
initially owns new results. After registration the host sends internal
`artifact_detach`; Python forgets ownership without deleting the file. The host
then controls pins and deletion. Public clients cannot call internal detach or
worker-owner release operations.

At most 64 registered artifacts and 512 MiB are retained per host. A document
pins files it is using, including open cached tabs; a file can have several
viewers. Unpinned results expire after 15 seconds. Pinned derived images survive
PRO → Minimal and survive release of a different viewer. Missing/expired results
are reported explicitly. Uploaded bridge files acquire a view lease as well.

临时结果由唯一所有者删除，避免 Python 清理与宿主缓存互相争抢。关闭图层移除引用；
不同查看页的引用互不代删。产物不是永久保存文件，应通过下载/导出保存需要的结果。
宿主只删除自己的标记目录，不递归删除共享缓存父目录。

Persistent archive downloads have a separate lifetime. Python creates only an owned
temporary artifact. The Rust host validates its owner/document/PRO epoch and FITS
header, copies it to a unique temporary file beside the original, and commits with
a same-directory hard link that cannot overwrite an existing file. Release/cancel
marks the matching active copy before waiting for the serialized science request;
the copy rechecks the lease after registering, so an earlier close cannot escape
cancellation. The temporary guard is armed only after successful `create_new`.

The no-overwrite commit is the completion point. A late cancellation never removes
the saved path, which the user might already have replaced. A committed file is not
registered as a cache artifact. Unsupported/unwritable destinations return an
explicit temporary-result fallback. The UI waits for the original save result after
a cancel acknowledgement and discards incompatible late document replies.

## Processes and browser resources

The host serializes Python protocol calls, bounds requests to 8 MiB and responses
to 32 MiB, and limits a response wait to 120 seconds. Worker startup belongs to a
dedicated stable thread. Unix workers have an owned process group; Windows uses
an owned Job Object. Shutdown first requests graceful exit, then terminates and
waits for the owned child/tree. Linux additionally guards the known worker and
packet child against disappearance of their creating parent. This does not
promise control of arbitrary external services started by third-party packages.

On Windows the worker starts suspended, joins the host-owned Job Object, and
only then resumes. This also covers the environment launcher creating its real
Python child. Failure while attaching or resuming terminates and waits for the
new worker. Scientific cache paths use Windows extended paths so that a long
Unicode parent path plus a generated artifact filename remains usable.

Jupyter's proxy command passes `--supervisor-pid` together with `--jupyter`.
The host observes that Jupyter process through a pinned Windows process handle,
or a Linux PID plus its start time. When it exits, the host stops its worker tree
and drains HTTP requests before exiting. This covers proxy versions whose
Windows shutdown hook leaves a child running. Ordinary standalone viewers do
not inherit the lifetime of the short-lived CLI that opened them.

The creating-thread detail follows the Linux
[PR_SET_PDEATHSIG caveat](https://man7.org/linux/man-pages/man2/PR_SET_PDEATHSIG.2const.html).

The browser cancels its outstanding fetches and polling, destroys the WASM app
and WebGL context, and sends a best-effort view release before removing the
iframe. Expiring leases cover a lost release request. Browser/GPU allocators may
retain reserved memory after objects are freed; evaluate repeated cycles,
live objects, job/artifact counts and owned PIDs together.
