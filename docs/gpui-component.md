# GPUI Kit coverage

clj-gpui 0.1.0 is pinned to **[GPUI Kit 0.6.0](https://crates.io/crates/gpui-kit/0.6.0)** (`gpui-kit` facade → `gpui-pre` 0.3.x + `gpui-component` 0.6 + `gpui-kit-assets`). This document is the inventory of that exact crate, not later git `main`.

0.5.1 names (`ui/text-field`, `ui/divider`, `ui/table`) were dropped. Data tables are `ui/data-table`. `ui/table` is reserved for Kit's declarative `Table` (not wrapped yet).

Classification:

| Class | Meaning |
|---|---|
| **A** | Already supported before this coverage work |
| **B** | Straightforward with the current tree + callback architecture; implemented |
| **C** | Supportable, but needs new host state, overlays, delegates, or virtualization |
| **D** | Not appropriate as a `gpui.ui` widget (window/process internals, forbidden surfaces) |
| **E** | Helper / trait / layout primitive rather than a user-facing control |

`ui/window`, `ui/spacer`, and `ui/hstack` / `ui/vstack` are clj-gpui layout, not 1:1 crate types (`Root`, `h_flex`, `v_flex`).

## Coverage table

| GPUI Kit 0.6 | clj-gpui API | status | class | notes |
|----------------------|--------------|--------|-------|-------|
| `button::Button` | `ui/button` | ✅ | A | Variants, compact, disabled, tooltip |
| `checkbox::Checkbox` | `ui/checkbox` | ✅ | A | 0-arg `:on-click` (unchanged). `:shape :circle` is a clj-gpui extra |
| `input::Input` | `ui/input` | ✅ | A | Host-held `InputState`; Clojure owns the string |
| `label::Label` | `ui/label` | ✅ | A | Div text, not Kit `Label` (no mask/highlight) |
| `h_flex` / `v_flex` | `ui/hstack` / `ui/vstack` | ✅ | A | |
| `scroll::ScrollableElement` | `ui/scroll` | ✅ | A | Flex leftover viewport; see the list-scroll layout fix |
| `Root` | (host) | ✅ | D | Window wrapper; not an app widget |
| `theme::*` | `:theme` / `gpui.theme` | ✅ | A | Existing ThemeSet architecture |
| `switch::Switch` | `ui/switch` | ✅ | B | `:on-change` receives boolean |
| `button::Toggle` | `ui/toggle` | ✅ | B | Button-style toggle; `:on-change` receives boolean |
| `radio::Radio` / `RadioGroup` | `ui/radio-group` | ✅ | B | `:on-change` receives the original Clojure id |
| `slider::Slider` | `ui/slider` | ✅ | B | Host-held `SliderState`; `:on-change` receives number. Clojure is source of truth: a controlled value is applied even when it is off-step. Entity is kept across unmounts (crate bounds are private; dropping remounts at 100% fill). A layout canvas re-renders when the track size changes so fill and thumb stay aligned. Dynamic unique ids retain slots until the window closes; bounded cleanup is a follow-up |
| `progress::Progress` | `ui/progress` | ✅ | B | 0–100 |
| `separator::Separator` | `ui/separator` | ✅ | B | Horizontal default; `:orientation :vertical` |
| `spinner::Spinner` | `ui/spinner` | ✅ | B | Needs bundled icons. Host wrapper owns `:width` / `:height` / `:size` / `:flex` (crate type is not `Styled`) |
| `tag::Tag` | `ui/tag` | ✅ | B | `:variant` keywords |
| `alert::Alert` | `ui/alert` | ✅ | B | `:on-close` is 0-arg |
| `skeleton::Skeleton` | `ui/skeleton` | ✅ | B | |
| `kbd::Kbd` | `ui/kbd` | ✅ | B | GPUI keystroke strings (`"ctrl-s"`) |
| `link::Link` | `ui/link` | ✅ | B | Opens `href`; optional 0-arg `:on-click` |
| `group_box::GroupBox` | `ui/group-box` | ✅ | B | `:variant` `:normal` / `:fill` / `:outline` |
| `badge::Badge` | `ui/badge` | ✅ | B | Count or `:dot`; wraps a child. Host wrapper owns layout keys |
| `tab::TabBar` | `ui/tabs` | ✅ | B | Bar only; Clojure renders the selected panel; keyword ids round-trip |
| `select::Select` | `ui/select` | ✅ | B | Host-held `SelectState<SearchableVec>`; `:searchable true` filters by label; `nil` clears |
| `Icon` / `IconName` | `ui/icon` | ✅ | B | Kebab names (`:circle-check`); bundled assets |
| `clipboard::Clipboard` | `ui/clipboard` | ✅ | B | `:on-copied` receives the string. Host wrapper owns layout keys |
| `breadcrumb::Breadcrumb` | `ui/breadcrumb` | ✅ | B | Group `:on-change` receives the original Clojure id |
| `avatar::Avatar` | `ui/avatar` | ✅ | B | Initials from `:name`; no image `src` yet |
| `accordion::Accordion` | `ui/accordion` | ✅ | B | Controlled open id; `:multiple` uses a JSON array of ids in original item order. Outer wrapper owns `:width` / `:height` / `:size` / `:flex` (default flex-none + full width) so crate `size_full()` does not eat leftover column height |
| `description_list::DescriptionList` | `ui/description-list` | ✅ | B | `{:label :value}` maps; vertical + 1 column by default (crate is horizontal / 3-col). Same outer-owns-layout wrap as accordion |
| `tooltip::Tooltip` | `:tooltip` style | ✅ | B | String tooltip on any node; wrapper copies width/height/size/flex so layout is unchanged |
| `slider::Slider` range / log scale | — | ❌ | C | Range thumbs and logarithmic scale need richer values |
| `input::Textarea` | `ui/textarea` | ✅ | C | Host-held `TextareaState`. Same string callbacks as `ui/input`. `:rows` default 3 |
| `input::NumberInput` | `ui/number-input` | ✅ | C | Host-held `InputState` + `NumberInput` wrapper. Step buttons parse, add/sub `:step`, clamp `:min`/`:max`, emit a number. Typed values emit when they parse |
| `input::OtpInput` | `ui/otp-input` | ✅ | C | Host-held `OtpState`. `:on-change` only when every cell is filled (crate complete-only). `:count` default 6, clamped 1–12. `:masked` |
| `input::Editor` / `EditorState` | `ui/editor` | ✅ | C | Kit `Editor` highlighter. `:language` (default `text`). **No LSP**. `tree-sitter-languages` enabled; no Clojure grammar |
| `select` searchable sections / custom item render | — | ⚠️ | C | Basic string select is B; groups/custom rows are not |
| `list::List` | `ui/list` | ✅ | C | `{id, label}` rows; host `ListDelegate`. `:searchable true` filters by label. Selection callbacks restore original Clojure ids |
| `table::DataTable` | `ui/data-table` | ✅ | C | Columns in `:columns` → wire `options` (not `columns` u32). Rows `{id, cells}`. Host `TableDelegate`. `column()` returns owned `Column` |
| `table::Table` (declarative) | `ui/table` | ❌ | C | Reserved name. Not wrapped in this release |
| `tree::Tree` | `ui/tree` | ✅ | C | Nested `{id, label, items}`; `:expanded` is initial. Click sends original id. Expand state is host-local until item identity changes |
| `dialog::Dialog` | `ui/dialog` | ✅ | C | Controlled `:open?`; overlay via `WindowExt`. `:variant` `:confirm`. Overlay click dismisses unless `:overlay-closable false` |
| `dialog::AlertDialog` | `ui/alert-dialog` | ✅ | C | Same controlled overlay as dialog; not backdrop-dismissible |
| `popover::Popover` | `ui/popover` | ✅ | C | Controlled `:open?`; trigger must be a button; content rebuilt from child nodes |
| `menu::PopupMenu` / context / dropdown | `ui/dropdown-menu`, `ui/context-menu` | ✅ | C | `{id, label}` items, nested `:items` submenus, `-` separators. No GPUI Action required. Context-menu host is `v_flex` so a wrapped `:flex 1` listing does not collapse |
| `VirtualList` | `ui/virtual-list` | ✅ | C | `{id, label, height?}` rows; host `v_virtual_list` / `h_virtual_list`. Vertical by default; `:orientation :horizontal` for a row. Default row height 36 |
| `sheet::Sheet` | `ui/sheet` | ✅ | C | One crate sheet; last open in tree order. Live spec + next-frame `WindowExt`. `:placement`, `:footer` |
| `notification::Notification` | `ui/notification` | ✅ | C | Presence shows unless `:open? false`. Fingerprint skips re-push. Autohide default true |
| `button::DropdownButton` | — | ❌ | C | Use `ui/dropdown-menu` |
| `button::ButtonGroup` | — | ❌ | E | Use `ui/hstack` of buttons |
| `color_picker::ColorPicker` | `ui/color-picker` | ✅ | C | Hex string / JSON `null`. Host `ColorPickerState` |
| `date_picker::DatePicker` / `calendar` | `ui/date-picker` | ✅ | C | ISO `YYYY-MM-DD` or `[start, end]`. `:range` / `:multiple`. Format `%Y-%m-%d` |
| `dock::*` / Tiles | `ui/dock` | ✅ | C | `DockSkin` + `DockLayout` + `panel_handle`. Tabs per side from item `:side`. Panel bodies: static overlay subset + markdown/chart (not list/data-table/editor). No layout persistence |
| `resizable::*` | `ui/resizable` | ✅ | C | Host `ResizableState`. `:on-change` is px size array |
| `sidebar::*` | `ui/sidebar` | ✅ | C | `{id, label, icon}` menu. `:collapsed`, `:side` |
| `setting::*` | `ui/settings` | ✅ | C | Pages / groups / fields. `:on-change` is `{:id :value}` with original field id |
| `chart::*` / `plot::*` | `ui/chart` | ✅ | C | `:line` / `:bar` / `:area` / `:pie`. Items `{label, value}` |
| `text::TextView` (markdown/HTML) | `ui/markdown`, `ui/html` | ✅ | C | Selectable; scrollable when `:height` or `:flex 1` |
| `highlighter::*` | — | ❌ | E | Tree-sitter internals for the editor |
| `form::{v,h}_form` / `field` | — | ❌ | E | Layout sugar; `vstack` is enough |
| `collapsible::Collapsible` | — | ❌ | E | Trait, not a widget |
| `avatar::AvatarGroup` | — | ❌ | C | Image stack + overflow |
| `TitleBar` | `ui/window` | ❌ | D | Window chrome is already Clojure-owned |
| `WindowBorder` | — | ❌ | D | Linux decoration helper |
| `Inspector` | — | ❌ | D | Debug-only |
| `History` | — | ❌ | E | Undo stack, not UI |
| `webview` | — | ❌ | D | Explicitly out of scope |
| `animation` helpers | — | ❌ | E | Not a control |
| `IndexPath` / `Rope` / geometry | — | ❌ | E | Host types |

0.6 also has `Combobox`, `Rating`, `Stepper`, chat `Message`/`Bubble`, and `NavStack`. Those are follow-ups (Phase 5), not this inventory's remaining C work. `gpui-fps` is postponed. `gpui-shell` / `gpui-wry` are out of scope.

## Category C — remaining

Slider range thumbs / log scale, searchable select sections, `DropdownButton`, `AvatarGroup`, declarative `ui/table`, Combobox / Rating / Stepper, and `gpui-fps` are still deferred. Full LSP for the code editor is out of scope; `ui/editor` is the highlighter widget.

### Overlay family (dialog, popover, menus, sheet, notification)

Kit 0.6 `Root::render` does not paint overlay layers; the host calls `Root::render_dialog_layer`, `Root::render_sheet_layer`, and `Root::render_notification_layer` from `RootView`. Open/close for dialogs and the single crate sheet still goes through `WindowExt` on the next frame so `RootView::render` does not re-enter `Root`. The overlay builder is stored for the overlay's lifetime and runs on every layer paint. It reads a live spec cell (callback ids, title, body, children, footer) instead of capturing the tree from open time — `export-tree` rebuilds the callback registry each render, so a stale `cb-7` would otherwise invoke the wrong function. The builder must not `entity.read` / `update` RootView. Dialog `:on-close` is 0-arg; `:on-ok` / `:on-cancel` are 0-arg. Crate order: OK → `on_ok` then `on_close`; Cancel / Escape / close button / overlay click → `on_cancel` then `on_close`. The host sends `:on-open-change false` from `on_close`. Those ids are captured for the action and sent as **one** callback batch so `:on-ok` cannot `export-tree` and rewire `:on-close`. Each Clojure handler runs at most once per action. Clicking the overlay dismisses the dialog (`:overlay-closable` defaults true even for `:variant :confirm`, which the crate otherwise locks). After a crate-side dismiss the host does not re-open until Clojure’s tree drops `:open?`. Static overlay children use a full path element id (`dialog-key/content/0/1`) so nested stacks cannot collide. Static buttons honor `disabled`, `primary` / `variant`, `on-click`, and `text`. Popover is in-tree and controlled (`:open?` + `:on-open-change`). Dropdown/context menus use `PopupMenuItem::on_click` (no GPUI Action). Nested `:items` are submenus. Item `:on-click` then menu `:on-change` is one batch. `ui/context-menu` is a flex column host (`v_flex` + `min_h_0`), not a block `div`; leftover height is inherited from a `:flex 1` child only when the menu omitted `:flex`, so wrapping a list/table/tree does not collapse the listing.

The crate holds **one** sheet. Last open `sheet` in tree order wins. Same live-cell + next-frame open as dialogs. `:placement` is the slide edge. `:footer` is a child node. Notifications are a stack keyed by Clojure id (`Notification::id1`). Presence in the tree means show unless `:open false`. A fingerprint of title|message|variant|autohide skips re-push (re-push would reset autohide). Tree removal dismisses with `suppress_close` so Clojure is not double-notified. Click uses crate `on_click` (fires before delayed dismiss). Autohide defaults **true**.

### Delegate collections (list, table, tree, virtual-list)

Clojure sends `{id, label}` (list), `{id, cells}` (table), nested `{id, label, items}` (tree), or `{id, label, height?}` (virtual-list, vertical unless `:orientation :horizontal`). Rust owns `ListDelegate` / `TableDelegate` / `TreeState` / `VirtualListView`. Selection callbacks send original Clojure ids. Table column defs travel in `options`, not the description-list `columns` u32. After rows/columns change the host calls `TableState::refresh`. Programmatic table `:selected` uses `set_selected_row` / `clear_selection` with a suppress flag so `SelectRow` does not bounce back as `:on-change`. Tree `:selected` is controlled: the host keeps cloned `TreeItem`s (shared expand `Rc`) and maps ids onto the current visible flattened index. Nested ids apply only while ancestors are expanded. Tree expand/collapse is host-local until the item identity changes (`set_items` would reset it). List `:on-change` is selection; `:on-confirm` is activation (click / Enter). Arrows emit Select only; click and Enter emit Confirm only in 0.5.1, so the host fires `:on-change` then `:on-confirm` on Confirm as one batch against the same callback generation, then one tree. Table left click always emits `SelectRow`; `click_count == 2` then emits `DoubleClickedRow` from that same `on_row_left_click`. A count-1 click is only `:on-change` (end of the GPUI effect cycle). A count-2 click is `:on-change` then `:on-confirm` (or `:on-double-click`) as one batch so those two never cross callback generations. Searchable lists reapply the active query when Clojure replaces rows. `list` / `table` / `tree` / `virtual-list` use an outer layout wrapper (same idea as scroll / content-sized widgets): `:size` is square, omitted width fills, default ~200px height unless `:height` / `:size` / `:flex 1`, visual keys live on the wrapper.

### Product widgets (number, OTP, color, date, editor, dock, sidebar, settings, charts, markdown)

Clojure stays the semantic owner. Rust holds widget `Entity` state only where GPUI requires it. Number-input reuses the input `InputState` slot with a `NumberInput` wrapper; step events parse the current text, apply `:step` / `:min` / `:max`, and `set_value` (which emits `:on-change` as a JSON number). A later input with the same element key clears `as_number` so non-numeric text still emits a string; the step subscription stays on the entity but no-ops while `as_number` is false. OTP `:on-change` is crate-complete-only; incomplete typing is not overwritten while focused. Color is hex or JSON `null`. Kit `ColorPickerState::set_value` takes only `Hsla`; a controlled `Some` → `nil` recreates the state entity (starts empty, no `:on-change`). Date is ISO; `set_date` is skipped when the value is unchanged so an open picker is not closed every frame. Editor is highlighter-only (`EditorState` + `set_highlighter` on language change). Undo/redo groups and fast typing emit several crate `Change` events; the host defers one `:on-change` with the latest string and will not send another until the next tree assigns a fresh `cb-N` (export-tree is monotonic, so a second send of the same id is `unknown callback`). Charts fill an outer `viewport_sized` wrapper (same layout/style keys as list/table). Resizable slots use `used_resizables` + `retain` like other dynamic entities. Notification fingerprint ignores callback ids so an unchanged toast is not re-pushed; click reads the current `cb-N` from the slot at click time. Dock `CljPanel::panel_name` is always `"clj-gpui-panel"`; panel paint does not re-enter `RootView` (static overlay painter + markdown/chart). Settings fields are rebuilt each `RootView` frame (`Settings` is `RenderOnce`). A `:variant :dropdown` / `:select` field with option `:items` is a field, not a group.

## Path to near-complete coverage

Clojure stays the semantic owner. Rust holds widget `Entity` state only where GPUI requires it. The slot map now covers input, textarea, slider, select, list, data-table, tree, OTP, color, date, editor, virtual-list, dock, and resizable. Overlay sync covers dialog, alert-dialog, sheet, and notification; popover/menus are in-tree. Remaining C work is slider range/log, select sections, DropdownButton, AvatarGroup, declarative Table, Combobox / Rating / Stepper — not a new architecture.

## Callback payloads (protocol v8)

| Widget | callback | payload |
|---|---|---|
| `switch` / `toggle` | `:on-change` | boolean |
| `slider` | `:on-change` | number (Clojure value is applied as-is, then clamped; `step` is drag granularity) |
| `select` / `radio-group` / `tabs` / `breadcrumb` | `:on-change` | original Clojure option id |
| `accordion` | `:on-change` | open id, or a vector of ids in original item order when `:multiple true` |
| `alert` | `:on-close` | none (0-arg) |
| `clipboard` | `:on-copied` | copied string |
| `input` / `textarea` | `:on-change` / `:on-submit` / `:on-blur` | string |
| `button` / `checkbox` | `:on-click` | none (unchanged) |
| `list` / `data-table` / `tree` / menus | `:on-change` | original Clojure row/item id |
| `list` | `:on-confirm` | original Clojure row id (click / Enter; same batch as `:on-change`) |
| `data-table` | `:on-confirm` / `:on-double-click` | original Clojure row id (double-click; same batch as that click's `:on-change`) |
| `dialog` / `alert-dialog` | `:on-close` / `:on-ok` / `:on-cancel` | none (0-arg) |
| `dialog` / `alert-dialog` / `popover` / `sheet` | `:on-open-change` | boolean |
| `sheet` / `notification` | `:on-close` | none (0-arg) |
| `number-input` | `:on-change` | number |
| `otp-input` | `:on-change` | string, only when every cell is filled |
| `color-picker` | `:on-change` | hex string or `nil` |
| `date-picker` | `:on-change` | ISO string, `[start end]`, or `nil` |
| `editor` | `:on-change` / `:on-blur` | string |
| `virtual-list` / `sidebar` | `:on-change` | original Clojure row id |
| `settings` | `:on-change` | `{:id original-field-id :value …}` |
| `resizable` | `:on-change` | vector of px sizes |

The wire still uses JSON strings. `gpui.ui` keeps a map of wire id → original Clojure id and restores it in the callback. `{:id :dark}` yields `:dark`; `{:id "custom-id"}` yields `"custom-id"`. If two options share a wire id (`:dark` and `"dark"`), the first wins.
