# Interface locales / 界面语言

`registry.json` is the canonical language registry. Each registered UTF-8 JSON file
contains the same complete English message keys, including the browser shell.
`en.json` is the canonical key set: its values equal its keys. The maintained locales
are English, simplified Chinese, Italian, Spanish, German and French.

Rust `i18n.rs` embeds these same files for native and WASM. It parses the small
registry once and each non-English dictionary only when selected. English returns
the input directly. Unknown text, paths, numerical values and scientific identifiers
return the original borrowed `&str`; no translated value enters scientific state.
The existing `t`, `set`, `supported`, `language_label` and iterable `LANGUAGES` APIs
remain available. Registry order does not select a dictionary: its file name does.

The browser imports `index.js`. Its synchronous translator is ready for immediate
English fallback; `ready` loads the registry and initially selected language. Only
the selected dictionary is fetched and cached. Missing resources fall back to the
English key, and an earlier async response cannot replace a later language choice.
The shell updates its document language and startup text after a switch. Shared web
asset staging already copies this directory recursively for every build profile.

## Add or update a language

1. Copy `en.json` to the new locale file and translate every value. Never change the
   keys. Keep named placeholders such as `{target}`, numbers, bounds and units.
   Preserve leading/trailing spaces used to concatenate messages. The migrated
   Chinese typography intentionally retains its existing spacing.
2. Add its stable code, native name, self-label and file name to `registry.json`.
   Codes are exact preferences values, not translated labels or locale guesses.
3. Add one named `include_str!` entry to `SOURCES` in `crates/viewer/src/i18n.rs`.
   No new crate or build generator is needed. Keep scientific IDs and stored enum
   values unchanged; translate only their displayed descriptions.
4. Extend the expected registry codes in `qa/test_i18n.py`, then run that script,
   `node qa/locale_shell.mjs`, and the Rust i18n/preferences/ribbon tests on the
   authorized Linux environment. Ribbon tests iterate the registry automatically
   and include narrow windows, both modes and themes; they are not screenshots.
5. Rebuild native/WASM and shared web assets together before a release. Check real
   screenshots separately, especially long translations and scientific warnings.

Prefer concise standard UI words on buttons while retaining the complete scientific
meaning in explanations. Do not translate FITS column IDs, file paths, service IDs,
palette names stored in preferences, coordinate values or formulas. Identical words
such as `FITS`, `WCS`, `Gaia`, `Sigma`, or language cognates are valid; untranslated
English sentences are not substitutes for completed translations.
