# FitsViewer 0.1.7 implementation plan / 修改与验收计划

Requested on 2026-10-27. Development starts from the published 0.1.6 source plus
its documentation updates. This plan is not a claim of completed validation.

1. Add direct Low/High selection and dragging on the physical-value histogram,
   with matching numeric controls and shared Minimal/PRO behavior. Display limits
   must remain finite and ordered; changing them must not change source pixels.
2. Add **Exit all FitsViewer windows**. The user explicitly chose application exit,
   rather than merely closing documents. Close owned viewers and jobs gracefully;
   never terminate the Jupyter server or unrelated kernels/processes.
3. Add a remembered **Open new images in the same window** preference. Reuse an
   existing compatible desktop viewer when enabled, with safe fallback if it has
   stopped; support the browser/Jupyter host without mixing window ownership.
4. Investigate large-image stalls. Native startup currently reads the complete
   array before creating a window; reuse bounded metadata/preview/tile access.
   Add compact 16-bit display tiles in Minimal mode, preserving invalid pixels and
   physical-value mapping. The user explicitly requires PRO to keep original-bit-depth
   tiles and normal image quality; mode changes must invalidate the display cache.
   Original-pixel readout and scientific operations retain original data in both modes.
5. Reproduce downloaded-template opening failures using an actual file when
   available, and test compression, empty-primary/extension images, formats,
   artifact lifetimes and host validation. Do not guess the reported root cause.

Check bilingual controls, histogram interactions, multiple viewer instances,
window reuse/exit cleanup, big-file resource budgets and downloaded FITS loading.
Keep numerical/unit tests distinct from GUI and real-data evidence. Record any
unavailable environment or missing reproducer. Updating the development version
does not authorize replacing published 0.1.6 assets or publishing an untested 0.1.7.
