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
| `slider::Slider` | `ui/slider` | ✅ | B | Host-held `SliderState`; `:on-change` receives number |
| `progress::Progress` | `ui/progress` | ✅ | B | 0–100 |
| `divider::Divider` | `ui/divider` | ✅ | B | Horizontal default; `:orientation :vertical` |
| `spinner::Spinner` | `ui/spinner` | ✅ | B | Needs bundled icons |
| `tag::Tag` | `ui/tag` | ✅ | B | `:variant` keywords |
| `alert::Alert` | `ui/alert` | ✅ | B | `:on-close` is 0-arg |
| `skeleton::Skeleton` | `ui/skeleton` | ✅ | B | |
| `kbd::Kbd` | `ui/kbd` | ✅ | B | GPUI keystroke strings (`"ctrl-s"`) |
| `link::Link` | `ui/link` | ✅ | B | Opens `href`; optional 0-arg `:on-click` |
| `group_box::GroupBox` | `ui/group-box` | ✅ | B | `:variant` `:normal` / `:fill` / `:outline` |
| `badge::Badge` | `ui/badge` | ✅ | B | Count or `:dot`; wraps a child |
| `tab::TabBar` | `ui/tabs` | ✅ | B | Bar only; Clojure renders the selected panel; keyword ids round-trip |
| `select::Select` | `ui/select` | ✅ | B | Host-held `SelectState<SearchableVec>`; `:searchable true` filters by label; `nil` clears |
| `Icon` / `IconName` | `ui/icon` | ✅ | B | Kebab names (`:circle-check`); bundled assets |
| `clipboard::Clipboard` | `ui/clipboard` | ✅ | B | `:on-copied` receives the string |
| `breadcrumb::Breadcrumb` | `ui/breadcrumb` | ✅ | B | Group `:on-change` receives the original Clojure id |
| `avatar::Avatar` | `ui/avatar` | ✅ | B | Initials from `:name`; no image `src` yet |
| `accordion::Accordion` | `ui/accordion` | ✅ | B | Controlled open id; `:multiple` uses a JSON array of ids |
| `description_list::DescriptionList` | `ui/description-list` | ✅ | B | `{:label :value}` maps |
| `tooltip::Tooltip` | `:tooltip` style | ✅ | B | String tooltip on any node; wrapper copies width/height/size/flex so layout is unchanged |
| `slider::Slider` range / log scale | — | ❌ | C | Range thumbs and logarithmic scale need richer values |
| `input::NumberInput` | — | ❌ | C | Extra `NumberInputState` + step semantics |
| `input::OtpInput` | — | ❌ | C | Per-cell state |
| `input` code editor | — | ❌ | C | Rope, LSP, highlighter; not a form control |
| `select` searchable sections / custom item render | — | ⚠️ | C | Basic string select is B; groups/custom rows are not |
| `list::List` | — | ❌ | C | `ListDelegate`, virtualization, search |
| `table::Table` / `DataTable` | — | ❌ | C | `TableDelegate`, columns, sort, virtualization |
| `tree::Tree` | — | ❌ | C | `TreeDelegate`, expand/collapse |
| `VirtualList` | — | ❌ | C | Measured variable-height lists |
| `dialog::Dialog` | — | ❌ | C | Overlay layer + `WindowExt` |
| `sheet::Sheet` | — | ❌ | C | Overlay layer |
| `popover::Popover` | — | ❌ | C | Anchor + overlay |
| `notification::Notification` | — | ❌ | C | Overlay stack |
| `menu::PopupMenu` / context / app menu | — | ❌ | C | Actions, nesting, OS menu bar |
| `button::DropdownButton` | — | ❌ | C | Popup menu |
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

## Category C — deferred

Do not cram these into the current protocol.

### Overlay family (dialog, sheet, popover, notification, menus)

gpui-component paints these through `Root` / `WindowExt` as a **layer above** the tree, not as ordinary children. clj-gpui currently has one render tree and one callback registry. A host abstraction is missing:

- open/close owned by Clojure (controlled) vs internal dismiss
- focus restore
- stacking order
- JSON-safe result payloads (confirm, cancel, selected command)

**Suggested API later:** `(ui/dialog {:open? … :on-close …} …)` and `(ui/context-menu items)`. **Order:** dialog, then popover, then menus.

### Delegate collections (list, table, tree, virtual list)

These require a Rust `Delegate` that can render **rows as GPUI elements** and report selection via `IndexPath`. Mapping every row through the JSON tree is possible but will not virtualize. A reusable host piece would be: Clojure sends `{id, cells/label}` rows; Rust owns scroll/measure; selection callbacks send ids.

**Order:** list (id + label), then table (columns), then tree.

### Date / color / number / OTP / editor

Each has a dedicated `*State` entity and a non-trivial value type (chrono date, HSLA, digit vector, rope). Generalize the existing `InputState` / `SliderState` / `SelectState` slot map first, then add one value codec at a time.

### Dock, sidebar, settings, charts, markdown

Application chrome and documents. They need persistence or a document model. They should not be the next batch.

## Path to near-complete coverage

Yes: keep **Clojure as the semantic owner**, keep **Rust widget state only where GPUI requires an Entity**, and grow **one slot map** (`text-field` → `slider` → `select` already follows this). Overlays need one new host capability. Delegates need one row protocol. That is a coherent sequence, not a pile of FFI wrappers.

## Callback payloads (protocol v4)

| Widget | callback | payload |
|---|---|---|
| `switch` / `toggle` | `:on-change` | boolean |
| `slider` | `:on-change` | number |
| `select` / `radio-group` / `tabs` / `breadcrumb` | `:on-change` | original Clojure option id |
| `accordion` | `:on-change` | open id, or a vector of ids when `:multiple true` |
| `alert` | `:on-close` | none (0-arg) |
| `clipboard` | `:on-copied` | copied string |
| `text-field` | `:on-change` / `:on-submit` / `:on-blur` | string (unchanged) |
| `button` / `checkbox` | `:on-click` | none (unchanged) |

The wire still uses JSON strings. `gpui.ui` keeps a map of wire id → original Clojure id and restores it in the callback. `{:id :dark}` yields `:dark`; `{:id "custom-id"}` yields `"custom-id"`. If two options share a wire id (`:dark` and `"dark"`), the first wins.
