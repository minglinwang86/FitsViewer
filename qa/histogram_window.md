# Focused Jupyter histogram browser regression

Run `node qa/histogram_window.cjs CONFIG_FILE` with Node.js, Playwright and its
Chromium browser available. The caller starts an isolated candidate Jupyter server
and supplies two synthetic FITS files below that server's root. The test creates
its own browser context and named Lab workspace; it does not use existing tabs or
kernels, restart services or run native/scientific suites.

The private JSON input contains:

| Field | Meaning |
| --- | --- |
| `base_url` | Authenticated Jupyter service base URL, ending with `/` |
| `token` | The service's existing user authentication token |
| `workspace` | A unique Lab workspace name for this run |
| `output` | Owned directory for screenshots and the result JSON |
| `image1`, `image2` | Server-root-relative paths to synthetic FITS files |
| `candidate_web` | Candidate `web` directory containing the generated `pkg` assets |
| `override_assets` | Default `false`: verify assets actually served by the installed candidate. `true` replaces JS/WASM only in this private QA context for source-level testing. |

The script verifies the actual JS/WASM response bytes against `candidate_web`.
It checks the initial popup, actual pointer move/resize, Jupyter document-tab
hiding/restoration, explicit `display:none` restoration, and Log counts `true`
and `false` across X/close, reopen and page reload. Snapshots include popup
geometry, inferred image/spectrum context, app identity, document generation,
`loaded`, `first_frame_ready`, and stored preference values.

For Windows CI, create the JSON in the runner's temporary directory from the
already-started candidate service's runtime values, then call:

```powershell
node qa/histogram_window.cjs $qaConfigPath
```

Use `override_assets=false` for packaged-candidate acceptance. X/close/reopen
geometry checks are mandatory, alongside the Log counts persistence checks.
The expected installed web directory can be obtained using the candidate Python:

```python
from fitsviewer.runtime import resources
print(resources()[2])
```

Collect only the result JSON and PNG screenshots as CI artifacts. Do not collect
the private input JSON, server runtime records or authentication logs. Remove the
private input after the run. The caller remains responsible for stopping only
its own temporary Jupyter and any test-owned kernels.
