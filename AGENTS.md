# Working in FitsViewer

Read `docs/AI_DEVELOPMENT_GUIDE.md` and `docs/ARCHITECTURE.md` before a substantial change.
They map source ownership, module features, scientific conventions and validation scope.

- Preserve unrelated/uncommitted work. Read the affected implementations and lifecycle.
- Use the shared core/viewer and module registry; do not create stripped source forks.
- Keep external scientific packages external; do not embed Python, TeX or science data.
- A source-only request excludes Cargo check/test, npm builds, wheels and installers.
  Dry plans and stdlib/AST checks are available without compiling the application.
- Do not treat static checks or historical screenshots as current GUI/science acceptance.
- Record work and remaining validation. Do not publish or restart user services without scope.
