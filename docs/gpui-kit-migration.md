# GPUI Kit 0.6 migration plan

Status: **plan only**. No host or Clojure code has been changed yet.

Upstream: [GPUI Kit v0.6.0](https://github.com/longbridge/gpui-kit/releases/tag/v0.6.0) (2026-09-03). Docs: [gpui-kit.com](https://gpui-kit.com). Source: [longbridge/gpui-kit](https://github.com/longbridge/gpui-kit) tag `v0.6.0`.

This document is the course of action for moving clj-gpui off **gpui-component 0.5.1 + crates.io `gpui` 0.2.2** onto **GPUI Kit 0.6**. It is written so we can discuss the strategy before spending a compile cycle on the host.

## Recommendation (read this first)

**Migrate the native host in one jump to `gpui-kit` 0.6. Keep the Clojure `gpui.ui` surface stable. Treat new Kit widgets as a second, optional coverage pass.**

That is the best course of action for this repo, for three reasons:

1. **There is no incremental crate upgrade.** 0.6 does not sit on Zed's crates.io `gpui` 0.2.2. Kit publishes its own GPUI snapshot as `gpui-pre` 0.3.x (`package = "gpui-pre"`, imported as `gpui`). Mixing 0.5.1 widgets with 0.6 GPUI types will not compile. A "wrap one widget, then the next" crate bump is not available.
2. **Clojure apps should not feel the rename.** clj-gpui's public API is maps (`:type :divider`, `ui/table`, `ui/text-field`, `ui/editor`). Most 0.6 breaks are Rust-side names (`Divider` → `Separator`, `Table` → `DataTable`, `InputState::code_editor` → `EditorState`). We can absorb those in the host and keep protocol v7 unless a wire field must change.
3. **New components are not the migration.** Combobox, Rating, Stepper, Command palette, NavStack, chat `Message`/`Bubble`, `Textarea`, radar/sankey charts, and so on are why 0.6 is attractive, but they are product work. Shipping a compiling host that still paints today's widgets is the gate. Coverage expansion is a follow-up PR, the same way 0.5.1 coverage landed in layers.

Do **not** adopt `gpui-shell` (JavaScript host). Clojure is already our scriptable layer. Do **not** adopt `gpui-wry` (WebView); that remains out of scope. `gpui-fps` is optional later for `:chrome :dev`, not part of the compile.

```text
today                         after (recommended)
─────                         ───────────────────
Clojure gpui.ui  ──maps──►    Clojure gpui.ui  ──maps──►  (same constructors)
host (gpui 0.2.2              host (gpui-kit 0.6 facade
      + gpui-component 0.5.1)       → gpui-pre 0.3.x
                                    → gpui-base 0.6
                                    → gpui-component 0.6
                                    → gpui-kit-assets 0.6)
```

Decision needed before implementation: agree on the dependency shape in [§3](#3-dependency-shape) and the phase split in [§8](#8-implementation-phases). Everything else in this file is inventory for that work.

## 1. What changed upstream

The Longbridge project is no longer "a component crate on top of Zed's `gpui` crate." It is **GPUI Kit**: a layered toolkit that vendors GPUI because they did not want to wait for Zed to publish a new crates.io `gpui`.

| Layer | Crate | Role |
|---|---|---|
| Facade | `gpui-kit` 0.6.0 | One dependency. Re-exports GPUI as `gpui_kit::*`, plus `application()`, `init()`, and `actions!`. Default features: `component` + `assets`. |
| GPUI | `gpui-pre` 0.3.1 (workspace pin; 0.3.3 already on crates.io) | Zed GPUI snapshot (`zed@5b055fa`), published under `package = "gpui-pre"`. Applications import it as `gpui`. Matching crates: `gpui-pre-platform`, `gpui-pre-macros`, `gpui-pre-sum-tree`, … |
| Unstyled | `gpui-base` 0.6.0 | Behavior, state, overlays, dock layout algebra, input engine, motion, virtual lists. Independent of the styled theme. |
| Styled | `gpui-component` 0.6.0 | The widget library we wrap today. Still the right painting layer for clj-gpui. Builds on `gpui-base`. |
| Assets | `gpui-kit-assets` 0.6.0 | Replaces `gpui-component-assets`. Rust path `gpui_kit_assets`. Lucide SVGs; `IconName` is generated from this pack. |
| Out of scope | `gpui-shell`, `gpui-wry`, `gpui-fps` | JS runtime, Wry WebView, FPS HUD. |

Documentation moved from `longbridge.github.io/gpui-component` to [gpui-kit.com](https://gpui-kit.com). The GitHub repo is `longbridge/gpui-kit` (the old `gpui-component` repo is the 0.5 line).

Kit itself uses **Rust edition 2024**. That is a dependency concern, not a requirement that our `host/` crate switch editions. Edition 2024 needs rustc **1.85+**. This environment has rustc **1.98.1**; `rust-toolchain.toml` already tracks `stable`.

Do not confuse this with **GPUI Box** (`gpui-box` / `gpui-box-kit` on crates.io). That is a different independent GPUI distribution. We want Longbridge **GPUI Kit**.

## 2. Where we are today

Pinned in `host/Cargo.toml`:

```toml
gpui = "0.2.2"
gpui-component = "0.5.1"
gpui-component-assets = "0.5.1"
```

Host edition is 2021. Clojure protocol version is **7**. Coverage inventory is [docs/gpui-component.md](gpui-component.md), which is explicitly "0.5.1, not later git main."

The host files that import GPUI / gpui-component:

| File | Why it matters for 0.6 |
|---|---|
| `host/src/main.rs` | `gpui::Application::new()` + `gpui_component_assets::Assets` + `gpui_component::init` |
| `host/src/renderer.rs` | Almost every widget, overlays, dock, editor `InputState::code_editor`, `Table::new`, `Divider`, `WindowExt`, `Root::render_*_layer` |
| `host/src/overlay.rs` | `Dialog::new(window, cx)`-era builders, `divider` in static paint, live spec cells |
| `host/src/mapping.rs` | `IconName`, `Divider` orientation helpers, sizes, placements |
| `host/src/rows.rs` | `ListDelegate` / `TableDelegate` (`column()` currently returns `&Column`) |
| `host/src/extra.rs` | Charts, markdown `TextView`, `CljPanel` (`zoomable` → `Option<PanelControl>`), settings, virtual lists |
| `host/src/catalog.rs` | Bundled ThemeSet JSON |
| `host/src/preview.rs`, `preview_macos.rs` | Window capture vs GPUI 0.2.2 occlusion (`zed#63217`). `inactive_frame_interval` was missing on 0.2.2; `gpui-pre` 0.3.x may have it. |
| `host/themes/*.json` | Copied from gpui-component 0.5.1 |

Clojure constructors in `src/gpui/ui.clj` already describe several widgets as 0.5.1-specific (`InputState::code_editor`, table click batching, one crate sheet). Those comments will need a 0.6 pass even if the function names stay.

## 3. Dependency shape

### 3.1 Recommended: one facade crate, aliases in Rust

```toml
[dependencies]
gpui-kit = { version = "0.6.0", features = ["tree-sitter"] }
# plus existing anyhow, serde, xcap, macos objc2, …
```

In `main.rs` / modules:

```rust
use gpui_kit::{self as gpui, application};
use gpui_kit::component as gpui_component;
use gpui_kit::assets as gpui_kit_assets;

fn main() {
    application()
        .with_assets(gpui_kit_assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            // …
        });
}
```

Why this, not listing `gpui-pre` + `gpui-component` + `gpui-kit-assets` ourselves:

- Kit's job is to keep **matching** `gpui-pre` / `gpui-pre-platform` / macros versions together. Wrong mixes produce identical-looking `App` / `Window` types that are not the same crate.
- `gpui_kit::application()` is `gpui_platform::application()`. **`gpui::Application::new()` is gone** on 0.3.x (only `with_platform` / `new_inaccessible` remain).
- `gpui_kit::init(cx)` is `gpui_component::init`, which also initializes `gpui-base`. Calling both is wrong.
- Default `assets` feature is the icon pack. Without it, `IconName` / spinner / alert / select chevron break the same way they would if we dropped `gpui-component-assets` today.

Keep the host crate on **edition 2021** until something forces 2024. Do not set `edition = "2024"` just because Kit did.

Pin **`0.6.0`** for the first compiling PR (this release is hours old; `gpui-pre` already moved 0.3.1 → 0.3.3 the same day). Relax to `"0.6"` once a lockfile has proven the range.

### 3.2 Tree-sitter features (decision)

0.6 **does not** enable Tree-sitter on the default feature set. Highlighting is per-language:

`tree-sitter`, `tree-sitter-languages`, or `tree-sitter-rust` / `-javascript` / `-python` / …

Our widgets gallery uses `(ui/editor src {:language "clojure" …})`. Kit has **no** `tree-sitter-clojure` feature (search on `v0.6.0` is empty). Today that language name is already a best-effort highlighter, not a real Clojure grammar. 0.6 does not make that worse.

Options:

| Option | When to use |
|---|---|
| **A. `tree-sitter` only** (JSON grammar + engine) | Smallest compile. `ui/editor` is a text box with little highlighting. |
| **B. A short list** (`rust`, `javascript`, `python`, `html`, `css`, `markdown`, `yaml`, `toml`, `bash`) | Matches "highlighter widget, not LSP" without dragging every grammar. |
| **C. `tree-sitter-languages`** | Closest to "whatever 0.5.1 defaulted to," slowest host builds. |

**Recommend B** for the migration PR, documented in `ui/editor` as the shipped set. Adding a grammar is a Cargo feature, not a protocol change.

### 3.3 Rejected dependency shapes

- **`gpui-component = "0.6"` plus a manual `gpui-pre` pin**, without `gpui-kit`. Works, but we re-implement the facade's whole reason for existing.
- **Git dependency on `longbridge/gpui-kit`.** Unnecessary; 0.6.0 is on crates.io.
- **Staying on 0.5.1.** Fine as a "not now" product decision, but then this branch is closed. 0.5.1 will not grow Combobox/Stepper/etc., and it is stuck on GPUI 0.2.2 forever.

## 4. Clojure compatibility policy

The migration PR should be **host-internal** unless a Kit API cannot be expressed with the current tree.

| Clojure name | 0.6 host mapping | Public change? |
|---|---|---|
| `ui/divider` | `separator::Separator` | No. Keep `:type :divider` on the wire. |
| `ui/table` | `table::DataTable` + existing `TableDelegate` | No. The new declarative `Table` is a different widget; do not steal this name. |
| `ui/text-field` | `Input` / `InputState` (already documented single-line) | No. |
| `ui/editor` | `Editor` / `EditorState` (was `InputState::code_editor`) | No constructor change. Host slot type changes. |
| `ui/number-input` | still `NumberInput` wrapping `InputState` | Likely no. Re-verify step events. |
| `ui/otp-input` | still `OtpInput` / `OtpState` | Likely no. |
| `ui/dialog` | still `WindowExt::open_dialog` + `Dialog::new(cx)` | No, if confirm/alert/overlay-closable still exist (they do on the styled `Dialog`). |
| `ui/dock` | `DockLayout` + `panel_handle` + `DockSkin` | No Clojure shape if we keep `{id, label, side, content}`. |

**Do not** rename `ui/divider` → `ui/separator` or `ui/table` → `ui/data-table` in the same PR as the crate bump. If we want those aliases later, add them without removing the old names.

Protocol version stays **7** unless we add fields (`:rows` for textarea auto-grow, `:language` grammar that must round-trip a Kit enum, dock persistence JSON, …). New widgets in a later PR bump the protocol when they need new node kinds.

`gpui.theme/register!` already passes unknown ThemeConfig keys through. New tokens (`selection.background`, `radius.lg`, focus ring, markdown-table colors, …) work for custom sets without a Clojure schema change. Required tokens in the 0.6 JSON schema are still the old core set (`background`, `foreground`, `primary.*`, `base.*`, …).

## 5. Breaking changes that hit *this* host

Mapped from the [v0.6.0 notes](https://github.com/longbridge/gpui-kit/releases/tag/v0.6.0) onto clj-gpui code. Severity is "will not compile" vs "compiles but behavior may drift."

### 5.1 GPUI 0.2.2 → gpui-pre 0.3.x (compile)

- `gpui::Application::new()` → `gpui_kit::application()` (platform constructor).
- Official examples open windows inside `cx.spawn(async move |cx| { cx.open_window(...) })`. Our `open_window` is synchronous in `run`. Try sync first; if 0.3 requires the async open, wrap the existing `RootView` / `Root` construction in a spawn. Do not redesign the bridge for that.
- `WindowOptions`, `TitlebarOptions`, `on_window_closed`, `on_window_should_close`, `PathPromptOptions`, `HasWindowHandle` need a compile pass. Preview capture on macOS uses `raw-window-handle` 0.6 and objc2 0.6 — Kit still uses those versions, so the capture helper is more likely to need GPUI class-name / occlusion tweaks than a handle rewrite.
- `preview_macos.rs` documents that 0.2.2 pauses the display link when occluded ([zed#63217](https://github.com/zed-industries/zed/issues/63217)). Re-test Preview after the bump; `gpui-pre` is a newer Zed snapshot and may include `inactive_frame_interval` ([zed#62628](https://github.com/zed-industries/zed/pull/62628)). Keep the occlusion swizzle until proven unnecessary.

### 5.2 Input / Textarea / Editor split (compile + behavior)

0.6: `InputState` is **single-line only**. Multiline and code editing are separate entities.

| Today | 0.6 |
|---|---|
| `ui/text-field` → `InputState::new` + `Input::new` | Same types. Good: our docs already say single-line. |
| `ui/editor` → `InputState::new().code_editor(lang).rows(12)` + `Input::new` | `EditorState::new(lang, window, cx)` + `Editor::new(&state)`. No `.code_editor()`, no Input-only `prefix` / `suffix` / clear / mask. |
| (none) | `Textarea` / `TextareaState` — **new Clojure widget**, not required to compile. |

`NumberInput` / `OtpInput` remain in `gpui_component::input`. Number-input currently shares the text-field `InputSlot` and `as_number` flag; keep that pattern if `InputState` is still the inner entity.

Editor change subscription (`InputEvent::Change` coalescing in `protocol::InputChangeCoalesce`) must be re-checked against `EditorState` events. Fast typing + undo grouping was already a host footgun on 0.5.1; do not assume the same `InputEvent` enum covers Editor.

Default editor font is now the theme monospace, row height from font size. Our outer `viewport_sized(..., 200.0)` wrapper should still own Clojure `:height` / `:flex`.

### 5.3 Table rename (compile)

- Element: `Table::new(&state)` → `DataTable::new(&state)`.
- Delegate: still `TableDelegate` + `TableState<D>` in 0.6 sources.
- `fn column(&self, …) -> &Column` → `fn column(&self, …) -> Column` (**owned**). `rows.rs` must clone (or store columns in a way that returns owned values).
- `row_selector` → `row_header` if we ever set that option (we do not today).
- `is_eof` → `has_more` (default `false`). We do not implement `is_eof`, so lists/tables should compile without a rename.

Do not wrap the new declarative `Table` for `ui/table`. That would silently change virtualization, selection, and delegate behavior.

### 5.4 Divider → Separator (compile)

```rust
gpui_component::divider::Divider  // gone
gpui_component::separator::Separator
```

Wire `:type "divider"` and `ui/divider` stay. Overlay static paint (`overlay.rs` `"divider" => Divider::horizontal()`) and `renderer.rs` vertical/horizontal constructors switch types. Mapping helpers for orientation/dashed stay conceptually the same if Separator still has them — verify at compile.

### 5.5 Dialog (compile, then overlay tests)

- `Dialog::new(window, cx)` → `Dialog::new(cx)`.
- Styled `Dialog` still has `.title()`, `.confirm()`, `.alert()`, `.overlay_closable()`, `.on_ok` / `.on_cancel` / `.on_close`, and `ParentElement::extend` for children. Our live-spec builder in `overlay.rs` should port with the constructor change.
- `Root::render` still does **not** paint dialog/sheet/notification layers (confirmed on `v0.6.0` `root.rs`). The host must keep calling `Root::render_dialog_layer` / `render_sheet_layer` / `render_notification_layer` from `RootView`. Next-frame `WindowExt::open_dialog` to avoid re-entering `Root` remains the right shape.
- New **declarative** API (`trigger`, `content`, `DialogHeader`, `DialogTitle`, …) is for in-tree dialogs. Our model is controlled `:open?` via `WindowExt`. Do not switch to trigger-style in the migration PR.
- `AlertDialog` is a convenience on `WindowExt::open_alert_dialog`. Optional later mapping for `:variant :alert` if the styled `Dialog::alert()` path drifts.
- `AlertDialog::overlay_closable` is deprecated (alerts cannot dismiss on backdrop). Our host currently **forces** overlay-closable true even for confirm/alert unless Clojure sets `:overlay-closable false`. Revisit that override against 0.6 defaults; behavior change would need a README note, not necessarily a protocol bump.

### 5.6 Dock (largest behavioral rewrite)

This is the one subsystem that is not a rename. 0.5.1 `DockItem::tabs` + `set_left_dock` / `set_right_dock` / `set_bottom_dock` / `set_center` are gone.

0.6 split:

- **`gpui_base::dock`**: pure-data `DockLayout`, `PaneTree`, persistence JSON (v0.5.0 shape kept), drag/resize.
- **`gpui_component::dock`**: `DockSkin` (chrome). Without a skin, a `DockArea` docks but draws no tab bar.

Required host changes in `renderer.rs` + `extra.rs`:

1. Build with `DockSkin::dock_area(id, version, window, cx)` (or `DockArea::new(...).with_renderer(DockSkin::new(cx))`), not a bare `DockArea::new`.
2. Wrap every `CljPanel` with `gpui_component::dock::panel_handle(panel)` before inserting. Bare `Entity<P>` compiles on the base API and then renders `panel_name` (`"clj-gpui-panel"`) instead of the title.
3. Describe layout with `DockLayout`:

   ```rust
   let center = DockLayout::tabs().panel_view(panel_handle(editor), cx);
   let left = DockLayout::tabs().panel_view(panel_handle(files), cx);
   dock.set_center(center, window, cx);
   dock.set_dock(DockPlacement::Left, left, window, cx);
   dock.set_dock_size(DockPlacement::Left, px(240.), window, cx);
   ```

   Placement accessors `left_dock()` / `set_left_dock()` are replaced by `layout(placement)`, `set_dock`, `set_dock_size`, `is_dock_open`, `remove_dock`.

4. Split `CljPanel` traits:
   - `gpui_base::dock::Panel`: `panel_name`, `closable`, `zoomable() -> bool`, dump/active callbacks.
   - `gpui_component::dock::Panel`: `title`, `zoom_control() -> Option<PanelControl>` (replaces today's `zoomable() -> Option<PanelControl>`).
5. Panel bodies stay the static overlay subset + markdown/chart (not list/table/editor). Persistence is still out of scope unless we decide to expose it.

Expect this to be the longest single chunk after "make it compile."

### 5.7 Smaller host API hits

| 0.5.1 | 0.6 | Our use |
|---|---|---|
| `popover_style` on `StyledExt` | `ThemeStyled` | Only if we call it (grep at compile). |
| Boolean readers `can_zoom` / `can_close` | `is_zoomable` / `is_closable` | Dock panel + any host reads. |
| Public struct fields on calendar/combobox/settings snapshots | builders + accessors | `setting::RenderOptions`, `CalendarItemState` if we construct them. |
| `History` undo API | `UndoHistory` vs navigation `History` | We do not wrap History as a widget. Editor undo is inside EditorState. |
| Charts: `ScaleBand` needs `Eq + Hash` | `BarChart` / new candlestick | `extra.rs` bar/line/area/pie. Pie index colors from PR #9 must still compile. |
| `TextView` in `gpui_component::text` | compatibility façade over `gpui-base` | `ui/markdown` / `ui/html` should keep working; re-test selection/copy. |
| Theme JSON `$schema` URL | `longbridge/gpui-kit` | Refresh vendored files. |

### 5.8 Themes and assets

Vendored palettes live in `host/themes/`, listed in `catalog.rs` and `gpui.ui/named-themes`.

0.6 theme directory (`themes/` on tag `v0.6.0`):

- **Same families we already ship:** adventure, alduin, ayu, catppuccin, everforest, fahrenheit, flexoki, gruvbox, harper, hybrid, jellybeans, kibble, macos-classic, mellifluous, molokai, solarized, spaceduck, tokyonight, twilight.
- **New in Kit:** `asciinema.json`, `aurora.json`.
- **We have, Kit 0.6 tree does not:** `matrix.json`. `named-themes` also lists **"Adventure Time"** as its own display name; confirm whether 0.6 still has that variant inside `adventure.json` or dropped it.

Tokyo Night variant names (`Tokyo Night` / `Tokyo Storm` / `Tokyo Moon`) still exist in 0.6 JSON. Token tweaks are small (e.g. `muted.background`, `input.border` presence). **Replace the vendored files from the 0.6 tag** rather than hand-merging, then diff `named-themes`.

Policy for Matrix / Adventure Time if Kit dropped them:

- **Keep shipping our 0.5.1 JSON** for names apps already pass as `:theme`, or
- **Drop them** and document the break.

Recommend **keep Matrix (and Adventure Time if missing)** as extra bundled files we own, until we know nobody relies on the string. Add Aurora / Asciinema to `named-themes` as an additive list change (not a protocol bump).

Schema docs: [gpui-kit.com](https://gpui-kit.com) / `.theme-schema.json`. Update `host/themes/README.md` links away from `longbridge.github.io/gpui-component`.

Assets: `gpui_component_assets::Assets` → `gpui_kit_assets::Assets`. Re-check `mapping.rs` `IconName` match arms; 0.6 generates names from the new pack. Missing kebab names should fall back the same way they do today (`None` → no icon), but the gallery icons (`:bell`, `:inbox`, …) must keep resolving.

## 6. Overlay, list, and editor behavior to re-verify (not just compile)

These are the places 0.5.1 already had subtle host logic. A green `cargo test` is not enough.

1. **Dialog / sheet live spec cells** — builders must still read current `cb-N` ids; export-tree is monotonic. Overlay regression tests in `host/src/overlay_regression_tests.rs` need to compile against `Dialog::new(cx)` and Separator.
2. **Dialog callback batching** — OK → `on_ok` then `on_close`; Cancel/Escape/overlay → `on_cancel` then `on_close`; one batch so `:on-ok` cannot rewire `:on-close`.
3. **Crate dismiss vs Clojure `:open?`** — `crate_dismiss_waiting_for_clojure` / `acknowledge_dialog_tree`.
4. **One sheet** — last open in tree order. Confirm Kit still has a single active sheet on `Root`.
5. **List confirm vs select** — 0.5.1 arrows = Select only; click/Enter = Confirm, host synthesizes `:on-change` then `:on-confirm`. If 0.6 list events changed, the widgets gallery (`:list-sel` / `:list-confirm`) will show it.
6. **Table double-click batching** — `SelectRow` then `DoubleClickedRow` from one click.
7. **Editor change coalescing** — `InputChangeCoalesce` + wait-for-seq around submit.
8. **Number-input slot sharing** with text-field keys.
9. **Color picker `Some` → `nil` recreate.**
10. **Date picker `set_date` skip when unchanged** (keeps the calendar open).
11. **Nested `:theme` restore** — Theme still process-global; `ThemeScope` must still pop.
12. **`preview-png`** on Linux X11/Xvfb and macOS occlusion.

## 7. New Kit surface (explicitly later)

Do not wrap these in the compile PR. They are the reason to migrate, scheduled after the host is green.

| Kit 0.6 | Suggested Clojure (later) | Notes |
|---|---|---|
| `Textarea` / `TextareaState` | `ui/textarea` | First additive win. Same callback story as `text-field`. |
| `Combobox` | `ui/combobox` | Replaces some of the deferred "searchable select sections" C item. |
| `Rating`, `Stepper`, `Pagination` | `ui/rating`, `ui/stepper`, `ui/pagination` | Straightforward controlled widgets. |
| `ProgressCircle`, `Shimmer` | `ui/progress-circle`, `ui/shimmer` | Feedback. |
| `HoverCard`, `AlertDialog`, `FocusTrap` | maybe | AlertDialog may fold into `ui/dialog` variants. |
| `Command`, `NativeMenu`, `StatusBar` | maybe | OS menu / palette — think through whether Clojure owns the menu tree. |
| `Message`, `Bubble`, `Attachment`, `Marker`, `MessageScroller` | later | Chat/assistant layout. Large API. |
| `NavStack` | `ui/nav-stack` | Push/back/forward + motion. Needs host history state. |
| Declarative `Table` | **not** `ui/table` | New name (`ui/grid` / `ui/simple-table`) if we want it. |
| `DataTable` extras | flags on `ui/table` | Multi-row headers, cell selection, custom row heights, export. |
| `RadarChart`, `SankeyChart`, candlestick | `:kind` on `ui/chart` | Additive chart kinds; band scale `Eq + Hash`. |
| `SelectableText`, window `TextSelection` | maybe | Cross-element copy. Preview/Evalight might care. |
| Motion / spring | no widget | Only if we expose animation as a node. |
| Accessibility IDs / labels | optional attrs | Good later; not a blocker. |
| `gpui-fps` | `:chrome :dev` HUD | Debug only. |
| Dock persistence, LSP editor | still out of scope | Same as 0.5.1 coverage doc. |

Update [docs/gpui-component.md](gpui-component.md) **after** the host compiles: retitle to GPUI Kit 0.6 coverage, add the new rows as class C. Keep the 0.5.1 file in git history; do not maintain two inventories.

## 8. Implementation phases

Each phase is a discussion checkpoint. Phase 0 is this document. Implementation should not start until we pick the options in §3 and §5.8.

### Phase 0 — this plan (done when the PR merges)

- Branch + this file + no code.

### Phase 1 — dependency spike (short, throwaway-ok)

Goal: `cd host && cargo check` with empty or stubbed modules **or** the real tree and a list of errors.

1. Change `host/Cargo.toml` as in §3.1.
2. `cargo generate-lockfile` / `cargo check` and paste the first error wave into the PR.
3. Confirm rustc, Vulkan/Linux deps, and that `gpui-pre` resolved to a version Kit accepts (0.3.1 pin vs 0.3.3).
4. Stop and report if `gpui-pre` 0.3.3 is a breaking GPUI API vs Kit's 0.3.1 pin — `[patch]` or `=0.3.1` may be required.

This phase is allowed to be an ugly compile dump. It answers "how large is GPUI 0.3, really?" before rewriting dock.

### Phase 2 — mechanical compile of the existing host

Goal: `cargo test --manifest-path host/Cargo.toml` unit tests (mapping, overlay regression, rows, extra, zenity) compile and pass **without** claiming overlay/editor/dock parity.

Order of remaps (cheapest first):

1. `main.rs` application + assets + init.
2. `Divider` → `Separator`.
3. `Table` → `DataTable`; `column()` owned; imports.
4. `Dialog::new(cx)`.
5. `ui/editor` host: `EditorState` / `Editor`; drop `.code_editor()`.
6. Dock last (see §5.6).
7. IconName / theme JSON / leftover import paths.

Do not add Clojure constructors here.

### Phase 3 — behavioral parity for shipped widgets

Goal: protocol-test, Clojure unit tests, and the widget gallery look like 0.5.1.

- Overlay tests + a real window smoke (`examples/widgets`).
- Editor typing / blur / language switch (even if `clojure` highlighting is weak).
- List/table/tree selection + confirm.
- Nested themes + Catppuccin Violet example.
- `preview-png` on Linux Xvfb; note macOS for a machine with Screen Recording.
- Counter, TodoMVC, template — no API changes expected.

If a Kit default changes visible behavior (dialog overlay-closable, editor font, tab animation), document it in README rather than silently papering over it — unless it breaks a protocol test, in which case prefer preserving clj-gpui's documented contract.

### Phase 4 — docs and inventory

- Point README / protocol / `gpui.ui` docstrings at gpui-kit.com.
- Replace 0.5.1 pins in `docs/protocol.md`.
- Rewrite coverage doc for 0.6 (new class-C rows, dock/editor notes).
- `host/themes/README.md` provenance.
- LICENSE note: palettes still Apache-2.0, now from gpui-kit.

### Phase 5 — new widgets (separate PR, after discussion)

Start with `ui/textarea`, then Combobox / Rating / Stepper / extra chart kinds. Each gets gallery coverage and protocol notes. Not a dump of every 0.6 module.

## 9. Testing plan (when we implement)

| Layer | Command / action | Phase |
|---|---|---|
| Host unit | `cargo test --manifest-path host/Cargo.toml` | 2–3 |
| Clojure unit | `clojure -M:test` | 3 |
| Format | `clojure -M:cljfmt check` | 3 |
| Bridge without window | `clojure -M:protocol-test` | 3 |
| Gallery | `cd examples/widgets && clj -M:dev` | 3 |
| Themes | `examples/themes/catppuccin-violet` | 3 |
| Counter / TodoMVC | existing examples | 3 |
| Preview | `gpui.runtime/preview-png` after connect | 3 |

There is no CI in this repo yet. Do not block the migration on adding it, but Phase 1–2 will be painful without at least `cargo test` locally.

Browser verification does not apply (native GPUI, not a web app). Closest substitute is the widget gallery + protocol-test.

## 10. Risks

- **`gpui-pre` moves independently of Kit.** 0.3.3 published the same day as 0.6.0. Lock the first compiling tree; do not float `*` on GPUI.
- **Compile time.** `tree-sitter-languages` plus GPUI/GPU stack is a long first build. Feature set B in §3.2 is the mitigation.
- **Dock rewrite regresses chrome** if we forget `DockSkin` or `panel_handle` (blank tabs / `"clj-gpui-panel"` titles).
- **Editor entity split** breaks slot reuse between `ui/text-field` and `ui/editor` if any example shared keys (they should not; keys are typed). Number-input sharing with text-field is the real slot hazard.
- **Preview / occlusion** still macOS-fragile; 0.3 may help or may rename `GPUIWindow`.
- **Theme name churn** (Matrix, Adventure Time, new Aurora) surprises `ui/themes` consumers.
- **No Clojure tree-sitter** — ` :language "clojure"` stays cosmetic.
- **Edition / MSRV** — fine on current stable; anyone on rustc &lt; 1.85 cannot build the host. README should say "recent stable" more firmly (1.85+).

## 11. Open questions for discussion

Please answer these before Phase 1, or accept the defaults in **bold**:

1. **Facade vs raw crates?** Default: **`gpui-kit` 0.6.0 with module aliases** (§3.1).
2. **Tree-sitter feature set?** Default: **short list (B)**, not all languages.
3. **Dropped palettes?** Default: **keep Matrix / Adventure Time if Kit omitted them**; add Aurora + Asciinema.
4. **Host edition?** Default: **stay 2021**.
5. **Dialog overlay-closable override** for `:confirm` / `:alert`? Default: **keep today's "Clojure can dismiss overlay unless it opts out"** and note if Kit fights it.
6. **Phase 5 first widgets?** Default: **`ui/textarea`, then Combobox / Rating / Stepper**.
7. **`gpui-fps` in dev chrome?** Default: **no**, until someone wants it.
8. **One migration PR vs spike PR + real PR?** Default: **spike can be the first commits on this branch**; land when Phase 3 is honestly done, not after Phase 5.

## 12. What this branch will not do

- Implement the crate bump, widget remaps, or new `gpui.ui` constructors.
- Publish to Clojars or add CI.
- Take a git dependency on Kit `main`.
- Introduce `gpui-shell` or WebView.
- Change protocol v7 "just in case."

When we decide to implement, start at Phase 1 and keep this file updated with the decisions from §11.
