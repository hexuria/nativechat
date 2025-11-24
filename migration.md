# GPUI Components → GPUI Migration Blueprint

## 1. Snapshot of Current `gpui_component*` Usage
- `src/main.rs`, `src/root.rs`: bootstrapping via `gpui_component::Root` (dialog/sheet/notification layers) and the `init` helper.
- `src/theme.rs`, `src/state.rs`: rely on `ThemeRegistry`, `Theme`, and `ActiveTheme` for dynamic theme loading/toggling.
- `src/assets.rs`: falls back to `gpui_component_assets::Assets` whenever local embeds miss.
- UI surfaces (`src/components/**`): import layout helpers (`h_flex`, `v_flex`, `StyledExt`), theme access (`ActiveTheme`), inputs (`Input`, `InputState`), buttons, icons, popovers, tooltips, scrollbars, avatars, sidebar primitives, etc.

## 2. Preparation
1. Clone Zed’s monorepo somewhere outside `nativechat` (we only need it as a dependency source):
   ```bash
   git clone https://github.com/zed-industries/zed.git ~/Code/zed
   ```
2. Identify the crates we want to vendor: `crates/ui`, `crates/ui_input`, `crates/ui_macros`, plus any transitive helpers those require (watch for `ui_resources`, `theme`, etc. when compiling).

## 3. Wiring the Crates Into `nativechat`
1. Add git/path dependencies in `Cargo.toml` (example using git, pin to a commit we trust):
   ```toml
   [dependencies]
   gpui = "0.2.2"
   ui = { package = "ui", git = "https://github.com/zed-industries/zed", rev = "<commit>" }
   ui_input = { git = "https://github.com/zed-industries/zed", rev = "<commit>" }
   ui_macros = { git = "https://github.com/zed-industries/zed", rev = "<commit>" }
   ```
   Alternatively, copy the crates under `nativechat/vendor/` and reference them via `path = "vendor/ui"` so we can tweak them locally.
2. Ensure the Zed crates depend on the same `gpui` version we already use. If not, add a `[patch.crates-io] gpui = { version = "0.2.2" }` section pointing to our crate to avoid duplicate symbols.
3. Remove `gpui-component` and `gpui-component-assets` once replacements compile, but not before—keep them until every module no longer imports them.

## 4. Replacing Imports (Suggested Order)
1. **Core Shell**: Swap `gpui_component::Root` usage with the equivalent constructs from Zed’s `ui` crate (it exposes `Root`, overlay renderers, and theme helpers). Update `main.rs`, `root.rs` accordingly.
2. **Theme System**: Replace `ThemeRegistry`, `Theme`, `ActiveTheme`, and `Theme::global_mut` calls with the Zed equivalents (look at `crates/ui/src/styles` and `component_prelude`). Make sure the theme watcher still reloads our custom themes.
3. **Assets**: Mirror Zed’s asset loading strategy or keep our `RustEmbed` fallback, but route default lookups to whatever asset bundle the Zed crates expect.
4. **Primitives & Components**: For each module in `src/components/**`, replace `use gpui_component::{...}` with imports from the vendored `ui` crate. If certain widgets (e.g., `SidebarMenuItem`, `ButtonVariants`) do not exist upstream, copy their implementations from `gpui-component` into `src/components/` and adjust namespaces.
5. **State Helpers**: Update `state.rs` to call the new theme toggling APIs. Confirm `ActiveTheme` (or its replacement) exposes `.background`, `.foreground`, etc.

## 5. Validation Pass
1. Remove all `gpui_component` / `gpui_component_assets` imports and run `rg "gpui_component" src` to confirm zero matches.
2. Run `cargo check` and `cargo fmt`/`cargo clippy` to ensure the code compiles and follows style.
3. Exercise the app to verify overlays (dialogs/sheets/notifications) and theming still behave—Zed’s `Root` helpers must replace the old ones feature-for-feature.

## 6. Cleanup & Hardening
- Once stable, delete unused assets/themes from the legacy components package.
- Consider pinning the Zed git dependency to a fork/commit so upstream breaking changes do not cascade into NativeChat unexpectedly.
- Document any local tweaks inside the vendored crates so future upgrades are repeatable.
