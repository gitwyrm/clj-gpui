# gpui-component coverage

clj-gpui 0.1.0 is pinned to **[gpui-component 0.5.1](https://crates.io/crates/gpui-component/0.5.1)**. This document is the inventory of that exact crate, not later git `main`.

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

| gpui-component 0.5.1 | clj-gpui API | status | class | notes |
|----------------------|--------------|--------|-------|-------|
| `button::Button` | `ui/button` | ✅ | A | Variants, compact, disabled, tooltip |
| `checkbox::Checkbox` | `ui/checkbox` | ✅ | A | 0-arg `:on-click` (unchanged). `:shape :circle` is a clj-gpui extra |
| `input::Input` | `ui/text-field` | ✅ | A | Host-held `InputState`; Clojure owns the string |
| `label::Label` | `ui/label` | ✅ | A | Div text, not gpui-component `Label` (no mask/highlight) |
| `h_flex` / `v_flex` | `ui/hstack` / `ui/vstack` | ✅ | A | |
| `scroll::ScrollableElement` | `ui/scroll` | ✅ | A | Flex leftover viewport; see the list-scroll layout fix |
| `Root` | (host) | ✅ | D | Window wrapper; not an app widget |
| `theme::*` | `:theme` / `gpui.theme` | ✅ | A | Existing ThemeSet architecture |
| `switch::Switch` | `ui/switch` | ✅ | B | `:on-change` receives boolean |
| `button::Toggle` | `ui/toggle` | ✅ | B | Button-style toggle; `:on-change` receives boolean |
| `radio::Radio` / `RadioGroup` | `ui/radio-group` | ✅ | B | `:on-change` receives the original Clojure id |
| `slider::Slider` | `ui/slider` | ✅ | B | Host-held `SliderState`; `:on-change` receives number. Clojure is source of truth: a controlled value is applied even when it is off-step. Entity is kept across unmounts (crate bounds are private; dropping remounts at 100% fill). A layout canvas re-renders when the track size changes so fill and thumb stay aligned. Dynamic unique ids retain slots until the window closes; bounded cleanup is a follow-up |
| `progress::Progress` | `ui/progress` | ✅ | B | 0–100 |
| `divider::Divider` | `ui/divider` | ✅ | B | Horizontal default; `:orientation :vertical` |
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
| `input::NumberInput` | — | ❌ | C | Extra `NumberInputState` + step semantics |
| `input::OtpInput` | — | ❌ | C | Per-cell state |
| `input` code editor | — | ❌ | C | Rope, LSP, highlighter; not a form control |
| `select` searchable sections / custom item render | — | ⚠️ | C | Basic string select is B; groups/custom rows are not |
| `list::List` | `ui/list` | ✅ | C | `{id, label}` rows; host `ListDelegate`. `:searchable true` filters by label. Selection callbacks restore original Clojure ids |
| `table::Table` / `DataTable` | `ui/table` | ✅ | C | Columns in `:columns` → wire `options` (not `columns` u32). Rows `{id, cells}`. Host `TableDelegate` |
| `tree::Tree` | `ui/tree` | ✅ | C | Nested `{id, label, items}`; `:expanded` is initial. Click sends original id. Expand state is host-local until item identity changes |
| `dialog::Dialog` | `ui/dialog` | ✅ | C | Controlled `:open?`; overlay via `WindowExt`. `:variant` `:confirm` / `:alert`. Overlay click dismisses unless `:overlay-closable false` |
| `popover::Popover` | `ui/popover` | ✅ | C | Controlled `:open?`; trigger must be a button; content rebuilt from child nodes |
| `menu::PopupMenu` / context / dropdown | `ui/dropdown-menu`, `ui/context-menu` | ✅ | C | `{id, label}` items, nested `:items` submenus, `-` separators. No GPUI Action required |
| `VirtualList` | — | ❌ | C | Measured variable-height lists |
| `sheet::Sheet` | — | ❌ | C | Overlay layer |
| `notification::Notification` | — | ❌ | C | Overlay stack |
| `button::DropdownButton` | — | ❌ | C | Use `ui/dropdown-menu` |
| `button::ButtonGroup` | — | ❌ | E | Use `ui/hstack` of buttons |
| `color_picker::ColorPicker` | — | ❌ | C | `ColorPickerState` + color type |
| `date_picker::DatePicker` / `calendar` | — | ❌ | C | Dates are not JSON-native; state entity |
| `dock::*` / Tiles | — | ❌ | C | Panel graph, persistence |
| `resizable::*` | — | ❌ | C | Drag state across rerenders |
| `sidebar::*` | — | ❌ | C | Composite app chrome |
| `setting::*` | — | ❌ | C | Settings schema + pages |
| `chart::*` / `plot::*` | — | ❌ | C | Data series + scales |
| `text::TextView` (markdown/HTML) | — | ❌ | C | Document model, selection |
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

0.5.1 has **no** `Combobox`, `Rating`, or `Stepper` modules. Those names appear in later gpui-component docs only.

## Category C — remaining

Sheet, notification, and the OS app menu bar still need overlay-stack work. VirtualList needs measured variable-height rows. Date / color / number / OTP / editor each have a dedicated `*State` and a non-JSON-native value. Dock, sidebar, settings, charts, and markdown are application chrome.

### Overlay family (implemented for dialog, popover, menus)

gpui-component 0.5.1 `Root::render` does not paint the dialog layer; the host calls `Root::render_dialog_layer` from `RootView`. Open/close still goes through `WindowExt` on the next frame so `RootView::render` does not re-enter `Root`. The dialog builder is stored for the overlay's lifetime and runs on every `render_dialog_layer` paint. It reads a live spec cell (callback ids, title, body, children) instead of capturing the tree from open time — `export-tree` rebuilds the callback registry each render, so a stale `cb-7` would otherwise invoke the wrong function. Title and body therefore update immediately while the dialog stays open. The builder must not `entity.read` / `update` RootView. `:on-close` is 0-arg; `:on-ok` / `:on-cancel` are 0-arg. Crate order: OK → `on_ok` then `on_close`; Cancel / Escape / close button / overlay click → `on_cancel` then `on_close`. The host sends `:on-open-change false` from `on_close`. Each Clojure handler runs at most once per action. Clicking the overlay dismisses the dialog (`:overlay-closable` defaults true even for `:variant :confirm`, which the crate otherwise locks). After a crate-side dismiss the host does not re-open until Clojure’s tree drops `:open?`. Static overlay children use a full path element id (`dialog-key/content/0/1`) so nested stacks cannot collide. Static buttons honor `disabled`, `primary` / `variant`, `on-click`, and `text`. Popover is in-tree and controlled (`:open?` + `:on-open-change`). Dropdown/context menus use `PopupMenuItem::on_click` (no GPUI Action). Nested `:items` are submenus.

Sheet and notification are still deferred.

### Delegate collections (implemented for list, table, tree)

Clojure sends `{id, label}` (list), `{id, cells}` (table), or nested `{id, label, items}` (tree). Rust owns `ListDelegate` / `TableDelegate` / `TreeState`. Selection callbacks send original Clojure ids. Table column defs travel in `options`, not the description-list `columns` u32. After rows/columns change the host calls `TableState::refresh`. Programmatic table `:selected` uses `set_selected_row` / `clear_selection` with a suppress flag so `SelectRow` does not bounce back as `:on-change`. Tree `:selected` is controlled: the host keeps cloned `TreeItem`s (shared expand `Rc`) and maps ids onto the current visible flattened index. Nested ids apply only while ancestors are expanded. Tree expand/collapse is host-local until the item identity changes (`set_items` would reset it). List `:on-change` is selection; `:on-confirm` is activation (click / Enter). Arrows emit Select only; click and Enter emit Confirm only in 0.5.1, so the host fires `:on-change` then `:on-confirm` on Confirm. Searchable lists reapply the active query when Clojure replaces rows. `list` / `table` / `tree` use an outer layout wrapper (same idea as scroll / content-sized widgets): `:size` is square, omitted width fills, default ~200px height unless `:height` / `:size` / `:flex 1`, visual keys live on the wrapper.

VirtualList (variable measured height) is still deferred.

## Path to near-complete coverage

Clojure stays the semantic owner. Rust holds widget `Entity` state only where GPUI requires it. The slot map now covers text-field, slider, select, list, table, and tree. Overlay sync covers dialog; popover/menus are in-tree. Remaining C work is product widgets (sheet, notification, dates, dock, charts), not a new architecture.

## Callback payloads (protocol v5)

| Widget | callback | payload |
|---|---|---|
| `switch` / `toggle` | `:on-change` | boolean |
| `slider` | `:on-change` | number (Clojure value is applied as-is, then clamped; `step` is drag granularity) |
| `select` / `radio-group` / `tabs` / `breadcrumb` | `:on-change` | original Clojure option id |
| `accordion` | `:on-change` | open id, or a vector of ids in original item order when `:multiple true` |
| `alert` | `:on-close` | none (0-arg) |
| `clipboard` | `:on-copied` | copied string |
| `text-field` | `:on-change` / `:on-submit` / `:on-blur` | string (unchanged) |
| `button` / `checkbox` | `:on-click` | none (unchanged) |
| `list` / `table` / `tree` / menus | `:on-change` | original Clojure row/item id |
| `list` | `:on-confirm` | original Clojure row id (click / Enter; also follows `:on-change`) |
| `dialog` | `:on-close` / `:on-ok` / `:on-cancel` | none (0-arg) |
| `dialog` / `popover` | `:on-open-change` | boolean |

The wire still uses JSON strings. `gpui.ui` keeps a map of wire id → original Clojure id and restores it in the callback. `{:id :dark}` yields `:dark`; `{:id "custom-id"}` yields `"custom-id"`. If two options share a wire id (`:dark` and `"dark"`), the first wins.
