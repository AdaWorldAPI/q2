# Vendored JavaScript resources

This directory holds JS payloads that Quarto's render pipeline embeds
into the binary via `include_bytes!` and ships as Project-scoped
artifacts when the relevant feature is active.

Adding a new resource here means a `BootstrapJsStage`-style stage that
detects a triggering condition and registers a `js:<feature>` artifact —
see `crates/quarto-core/src/stage/stages/bootstrap_js.rs` for the
prototype.

## `bootstrap/`

Bootstrap 5 JS runtime, used when a Bootstrap-backed theme is active.

- **`bootstrap.bundle.min.js`** — Bootstrap 5.3.1, the *bundled* build
  with Popper inlined. Fetched from
  <https://cdn.jsdelivr.net/npm/bootstrap@5.3.1/dist/js/bootstrap.bundle.min.js>.
  Size: 80,668 bytes.

  We deliberately ship the **bundle** (not `bootstrap.min.js`) so that
  popovers, tooltips, and auto-positioned dropdowns work without an
  extra Popper script. Quarto 1 ships the same bundled bytes but
  mislabels the file as `bootstrap.min.js`; we use the correct name.

### Version contract

The Bootstrap JS version here **must match** the Bootstrap SCSS version
under `resources/scss/bootstrap/` (see that directory's README). When
bumping Bootstrap, update both in the same commit. Mismatched JS/CSS
versions can produce subtle component bugs (e.g. JS expects a class
that the CSS no longer ships).
