# Clojure ↔ GPUI protocol

Newline-delimited JSON over a local TCP connection. Clojure listens;
the native host connects as a client.

Environment for the host process:

| Variable | Meaning |
|---|---|
| `CLJ_GPUI_PORT` | TCP port of the Clojure listener (required) |
| `CLJ_GPUI_HOST` | TCP host, default `127.0.0.1` |

Protocol version is **9**. Clojure sends it on `:ready`. The host refuses a mismatch.

## Handshake

1. Clojure binds `127.0.0.1:0`, then spawns the host with `CLJ_GPUI_PORT` set.
2. Host connects and waits for a `ready` line.
3. Host sends `render` (and later `callback` / `reload`) as JSON objects with a numeric `id`.
4. Clojure replies with `{"op":"response","id":…, …}`.

### `ready` (Clojure → host)

```json
{"op":"ready","protocol-version":9,"nrepl":7888,"app":"counter.app/app"}
```

### `request-render` (Clojure → host)

Sent when an `r/atom` changes, a file watcher reloads, or `gpui.ui/request-render!` is called. The host follows up with `render`.

```json
{"op":"request-render"}
```

### `pick-directory` (Clojure → host)

Ask the host to show a native folder picker. Does not block Clojure. The host later issues `directory-picked`.

```json
{"op":"pick-directory","request-id":"pick-1","title":"Choose a folder"}
```

On Linux the host uses the xdg desktop portal, then `zenity --file-selection --directory` if the portal is missing. Zenity runs on a background thread so the dialog cannot stall GPUI. User cancel is `cancelled`; a missing zenity binary or a non-cancel zenity failure is `error`.

### `reveal-path` / `open-path` (Clojure → host)

```json
{"op":"reveal-path","path":"/Users/me/Documents"}
{"op":"open-path","path":"/Users/me/Documents"}
```

`reveal-path` shows the path in Finder / the file manager. `open-path` opens it with the system handler. No reply.

### `capture-preview` (Clojure → host)

Ask the host for a PNG of the current native window. Clojure waits on the matching `preview-captured` RPC. Evalight calls `gpui.runtime/preview-png` over nREPL after connect / Run.

```json
{"op":"capture-preview","request-id":"cap-1"}
```

The host does **not** read the GPUI framebuffer. Capture runs on a background thread after dirtying the window. GPUI 0.2.2 stops its macOS CVDisplayLink unless `NSWindowOcclusionStateVisible` is set ([zed#63217](https://github.com/zed-industries/zed/issues/63217)), so a window covered by Evalight would otherwise never present. On the first `capture-preview` the host overrides `-[GPUIWindow occlusionState]` (not global `NSWindow`) so the display link keeps running, then ScreenCaptureKit-reads the window in-process. Ordinary apps keep GPUI's occlusion power-saving until Preview is used.

`WindowOptions::inactive_frame_interval` from [zed#62628](https://github.com/zed-industries/zed/pull/62628) is not in crates.io `gpui` 0.2.2 and only throttles animation while unfocused. Inactive is not the same as occluded.

Linux and Windows spawn a helper of the same binary (`clj-gpui --capture-preview --pid <host-pid> [--title …] [--wid …]`) and [xcap](https://crates.io/crates/xcap) 0.4.1. A second process is required so Windows `xcap::Window::all()` can see the GPUI window (it skips the current process to avoid `GetWindowText` deadlocks) and so `PrintWindow` is not issued while the UI thread is blocked. On Linux, xcap enumerates and captures through X11/XCB: X11 and XWayland windows work; native Wayland windows are not reliably listed and capture may return `nil`.

macOS captures **in-process** with ScreenCaptureKit `SCContentFilter(desktopIndependentWindow:)`. A helper has a different PID and cannot snapshot a covered window. `CGWindowListCreateImage` is a fallback if ScreenCaptureKit is unavailable.

No window, minimized, headless, native Wayland (this xcap path), or a missing macOS Screen Recording permission is an omitted `png` field (`nil` in Clojure). The helper must not print the image anywhere except its stdout. The UI-tree schema is unchanged from v6 until v8.

v7 added this capture pair. A v6 host ignores `capture-preview`, so the version must match exactly.

v8 is the GPUI Kit 0.6 rename: `text-field` → `input`, `divider` → `separator`, `table` → `data-table`, plus `textarea` and `alert-dialog`. Editor is Kit `EditorState`, not `InputState::code_editor`.

v9 adds Kit's remaining first-pass widgets: declarative `table`, `combobox`, `rating`, `stepper`, and the `gpui-fps` HUD on `:chrome :dev`.

## Host → Clojure ops

Each request includes a unique numeric `id`. Clojure echoes it on the response.

### `render`

```json
{"op":"render","id":1}
```

Response:

```json
{"op":"response","id":1,"ok":true,"tree":{…},"themes":[]}
```

`themes` is always an array of ThemeSet objects registered in the Clojure process (`gpui.theme/register!`). Each set is GPUI Kit JSON: required `name` / `mode` / `colors`, plus any other ThemeConfig fields Clojure preserved (`highlight`, `font.size`, …). `[]` means the host should drop previously installed Clojure ThemeSets. UI nodes still name a palette with the string `theme` field; they do not embed the color map.

On an application exception Clojure still returns `ok: true` with an error UI tree so the window can paint.

### `callback`

```json
{"op":"callback","id":2,"callback-id":"cb-2"}
```

Invokes the real Clojure IFn that was registered when the current tree was exported, then the host **always** issues another `render`. That second fetch carries input submit `seq` and covers handlers that do not touch an atom. While the callback runs, Clojure does not send `request-render` from `r/atom` watches, so a typical `swap!` click is one paint, not two. nREPL updates, hot reload, and `ui/request-render!` still use `request-render`.

Optional `value` is a JSON value (string, number, boolean, array, or `null`):

```json
{"op":"callback","id":2,"callback-id":"cb-2","value":"hello"}
{"op":"callback","id":3,"callback-id":"cb-3","value":true}
{"op":"callback","id":4,"callback-id":"cb-4","value":36.5}
```

When `value` is present (including `""`, `false`, `0`, and `null`), Clojure calls `(f value)`. Buttons and checkboxes omit `value`; Clojure calls the handler with no arguments.

A native user action that fires several handlers (list click/Enter, table double-click `SelectRow`+`DoubleClickedRow`, dialog OK/Cancel, menu item with both item `:on-click` and menu `:on-change`) is one host-internal batch. The worker sends the existing `"callback"` op once per handler, in order, against the **same** callback registry generation, then issues **one** `"render"`. There is no new Clojure wire op. Intermediate callback requests set `"defer-render": true` so an `r/atom` watch cannot enqueue `request-render` (which would `export-tree` and rebuild `cb-N` ids) before the rest of the batch and the host's following `render`.

Failure policy: **stop remaining callbacks on the first failure** (unknown id, thrown handler, or `ok: false`). Earlier atom mutations still paint because the worker fetches a tree after the stop. The error is not swallowed (`HostEvent::Error` after that tree). Prefer this over continuing so a failed prerequisite cannot invoke a later action.

`Cmd` (`Callback` / `CallbackBatch`) is host-internal and is not part of the JSON protocol version.

v4 changed `value` from string-only to any JSON type so switch/slider/select can pass booleans, numbers, and ids without encoding them as strings. Text fields still send strings. New node types were added in the same bump so a v3 host cannot silently paint “Unknown GPUI node” placeholders.

v5 added overlay nodes (`dialog`, `popover`, `dropdown-menu`, `context-menu`) and row-delegate collections (`list`, `table`, `tree`). A v4 host would paint “Unknown GPUI node” for those types, so the version must match exactly.

v6 added product widgets: `sheet`, `notification`, `number-input`, `otp-input`, `color-picker`, `date-picker`, `editor`, `virtual-list`, `chart`, `markdown`, `html`, `sidebar`, `settings`, `dock`, `resizable`. Overlay sheet/notification reuse the v5 live-spec + next-frame `WindowExt` pattern. A v5 host would paint “Unknown GPUI node” for those types. Editor and text-field `:on-change` coalesce grouped crate `Change` emits (undo/redo, burst typing) into one callback per registry generation so a stale `cb-N` cannot surface as `unknown callback`. Notification click also reads the current `cb-N` at click time (fingerprint ignores ids so an unchanged toast is not re-pushed). Color `null` clears by recreating `ColorPickerState` because 0.5.1 `set_value` cannot take `None`.

Option ids on the wire are JSON strings. Clojure restores the original application id in the callback (`:dark` not `"dark"`). Accordion `:multiple` uses a JSON array of ids, including ids that contain commas.

### `reload`

```json
{"op":"reload","id":3}
```

`(require ns :reload)` of `gpui.ui`, `gpui.core`, `gpui.ratom`, `gpui.theme`, every watched application `.clj` namespace, and the root app namespace. Helper namespaces are reloaded before the root; `(require app :reload)` alone does not reload already-loaded deps. `defonce` / `r/atom` bindings are kept. Response includes a fresh `tree` and the current `:themes` array. A compile/syntax error still returns `ok: true` with an error UI tree so the window stays up.

### `directory-picked`

Result of `pick-directory`. `path` is set when the user chose a folder. `cancelled` is true if they dismissed the dialog. `error` is a string when the dialog could not be shown.

```json
{"op":"directory-picked","id":4,"request-id":"pick-1","path":"/tmp","cancelled":false}
```

Clojure invokes the `gpui.platform/pick-directory` callback. It does not automatically re-export the tree; a typical handler `swap!`s an `r/atom`.

### `preview-captured`

Result of `capture-preview`. `png` is a base64 PNG of the native window, omitted when capture failed.

```json
{"op":"preview-captured","id":5,"request-id":"cap-1","png":"iVBOR…"}
```

Clojure's `gpui.runtime/preview-png` returns that string, or `nil`. It never throws.

From a running `clj -M:dev` nREPL:

```clojure
(resolve 'gpui.runtime/preview-png)
(gpui.runtime/preview-png)
```

## Node schema (version 9)

Every node is a JSON object. Unknown fields are ignored by the host.

| Field | Type | Used by |
|---|---|---|
| `type` | string | all (`window`, `label`, `button`, `vstack`, `hstack`, `spacer`, `checkbox`, `scroll`, `input`, `textarea`, `switch`, `toggle`, `radio-group`, `slider`, `progress`, `separator`, `spinner`, `tag`, `alert`, `skeleton`, `kbd`, `link`, `group-box`, `badge`, `tabs`, `select`, `combobox`, `icon`, `clipboard`, `breadcrumb`, `avatar`, `accordion`, `description-list`, `dialog`, `alert-dialog`, `popover`, `dropdown-menu`, `context-menu`, `list`, `data-table`, `table`, `table-header`, `table-body`, `table-footer`, `table-row`, `table-head`, `table-cell`, `table-caption`, `tree`, `sheet`, `notification`, `number-input`, `otp-input`, `color-picker`, `date-picker`, `editor`, `virtual-list`, `chart`, `markdown`, `html`, `sidebar`, `settings`, `dock`, `resizable`, `rating`, `stepper`) |
| `id` | string | optional stable identity, especially `input`, `textarea`, `slider`, `select`, `combobox`, `list`, `data-table`, `tree`, `dialog`, `alert-dialog`, `sheet`, `notification`, `editor`, `rating`, `stepper` |
| `text` | string | `label`, `button`, `checkbox`, `input`, `textarea`, `switch`, `toggle`, `separator`, `tag`, `alert`, `kbd`, `link`, `clipboard`, `avatar`, `editor`, `markdown`, `html`, `number-input`, `table-head` / `table-cell` / `table-caption` (when they have no children) |
| `placeholder` | string | `input`, `textarea`, `select`, `combobox`, `date-picker`, `number-input` |
| `children` | array of nodes | layouts, `scroll`, `group-box`, `badge`, `dialog`, `popover`, `context-menu`, `sheet`, `resizable`, declarative `table` and its Kit primitives (`table-header`, `table-body`, `table-footer`, `table-row`, `table-head`, `table-cell`, `table-caption`) |
| `items` / `options` | array of `{id,label,text,disabled,content,on-click,span,items,cells,separator,width,align,checked,icon,expanded,value,height,side,variant,min,max,step}` | `radio-group`, `select`, `combobox`, `tabs`, `breadcrumb`, `accordion`, `description-list` (`span` is description-list column span). Nested `items` are menu submenus / tree children / settings groups. Data-table **columns** are `options`; **rows** are `items` with `cells`. Chart points use `value`. Virtual-list rows may set `height`. Dock panels set `side` + `content`. Do not reuse `columns` (u32) for table column defs. Clojure `ui/table` shorthand expands to primitive children before the wire |
| `trigger` | node | `popover`, `dropdown-menu` (usually a `button`) |
| `footer` | node | `sheet` footer |
| `on-click` | string callback id | `button`, `checkbox`, `label`, `vstack`, `hstack`, `link`, `notification` |
| `on-double-click` | string callback id | `label` (0-arg; wins over `on-click` when `click_count >= 2`); `data-table` double-click row (row id) |
| `on-change` | string callback id | `input`/`textarea`/`editor` (string), `switch`/`toggle` (bool), `slider`/`number-input`/`rating` (number), `select`/`combobox`/`radio-group`/`tabs`/`breadcrumb`/`accordion`/`list`/`data-table`/`tree`/`dropdown-menu`/`context-menu`/`virtual-list`/`sidebar`/`stepper` (wire id; Clojure restores the original id). Accordion / combobox `:multiple` sends a JSON array in original item order. `otp-input` string when full. `color-picker` hex or `null`. `date-picker` ISO string or `[start, end]`. `settings` `{"id","value"}`. `resizable` array of px sizes |
| `on-submit` | string callback id | `input` (Enter). `textarea`: when set, Enter submits and Shift+Enter inserts a newline (Kit `submit_on_enter`); omitted, both keys insert a newline |
| `on-blur` | string callback id | `input`, `textarea`, `otp-input`, `editor` (called with the current string) |
| `on-escape` | string callback id | `input`, `textarea`, `editor` (0-arg) |
| `on-close` | string callback id | `alert`, `dialog`, `alert-dialog`, `sheet`, `notification` (0-arg) |
| `on-ok` / `on-cancel` | string callback id | `dialog`, `alert-dialog` (0-arg; crate then closes and fires `on-close`) |
| `on-confirm` | string callback id | `list` (click / Enter; original Clojure row id). Arrows only fire `on-change`; click/Enter fire `on-change` then `on-confirm` as one batch. `data-table`: count-1 click is only `on-change` (end of the GPUI effect cycle). Count-2 `on_row_left_click` emits `SelectRow` then `DoubleClickedRow`, batched as `on-change` then `on-confirm` (or `on-double-click`). `combobox`: Kit may emit `Change` then `Confirm` for one pick; the host batches `:on-change` then `:on-confirm` against the same generation, then fetches one tree. Confirm without Change (dismiss) is `:on-confirm` only |
| `on-open-change` | string callback id | `popover` (boolean); `dialog` / `alert-dialog` / `sheet` (`false` on dismiss) |
| `on-copied` | string callback id | `clipboard` (copied string) |
| `focus` | bool | `input`, `textarea`: request keyboard focus |
| `checked` | bool | `checkbox`, `switch`, `toggle` |
| `value` | JSON number, string, array, bool, or null | `slider`/`progress`/`number-input`/`rating` (number), `select`/`combobox`/`radio-group`/`tabs`/`list`/`data-table`/`tree`/`virtual-list`/`sidebar`/`stepper` (selected id or `null` to clear), `accordion` / combobox `:multiple` (id, `null`, or array of ids), `otp-input` (string), `color-picker` (hex), `date-picker` (ISO string or `[start, end]`) |
| `min`, `max`, `step` | number | `slider`, `number-input`. `rating` uses `max` (default 5). Slider `step` is drag granularity; the host applies Clojure's controlled value even when it is off-step, then clamps to `min`/`max` |
| `orientation` | string | `radio-group`, `slider`, `separator`, `resizable`, `stepper`: `horizontal` (default) or `vertical`. `virtual-list` and `description-list`: `vertical` (default) or `horizontal` |
| `columns` | number | `description-list`: grid columns 1–10 (default 1). The crate's own default is 3; the host does not use that |
| `disabled` | bool | buttons and most controls |
| `tooltip` | string | any node: GPUI Kit tooltip |
| `accessibility-label` | string | declarative `table`: Kit `Table::accessibility_label` (screen-reader name). A visible `table-caption` is not used as that name |
| `href` | string | `link` |
| `icon` | string | `icon`, `spinner` (kebab `circle-check`) |
| `control-size` | string | `xs`/`small`/`medium`/`large` (Clojure `:size :small` is rewritten so pixel `:size` stays numeric) |
| `count` | number | `badge`; `otp-input` length (default 6, clamped 1–12) |
| `dot` | bool | `badge` |
| `dashed` | bool | `separator` |
| `outline` | bool | `tag` |
| `searchable` | bool | `select` / `combobox`: show a filter field; host uses `SearchableVec` so typing actually filters. Combobox defaults true in `ui/combobox`. `list`: filter rows by label |
| `open` | bool | `dialog`, `alert-dialog`, `popover`, `sheet`: controlled open (`:open?` in Clojure). Omitted/false dialogs/sheets are not shown. `notification`: omitted/true shows; `false` hides |
| `overlay-closable` | bool | `dialog`, `sheet`: click the dimmed overlay to dismiss (default true). `alert-dialog` is not backdrop-dismissible |
| `placement` | string | `sheet`: `left` / `right` / `top` / `bottom` (default `right`) |
| `autohide` | bool | `notification` (default true) |
| `language` | string | `editor` highlighter (`rust`, `json`, `markdown`, …; default `text`). Kit's `tree-sitter-languages` bundle is enabled; there is no Clojure grammar |
| `rows` | number | `textarea` visible height (default 3) |
| `masked` | bool | `otp-input` |
| `collapsed` | bool | `sidebar` |
| `side` | string | `sidebar` (`left`/`right`); dock item `left`/`right`/`bottom`/`center` |
| `format` | string | `markdown` vs `html` (node `type` `html` is enough) |
| `range` | bool | `date-picker` range mode |
| `multiple` | bool | `accordion`, `combobox` |
| `message` | string | `alert` (alias of `text`) |
| `shape` | string | `checkbox`: `circle` for a round toggle |
| `primary` | bool | `button` (alias for `variant: primary`) |
| `variant` | string | `button`, `tag`, `alert`, `tabs`, `group-box`, `toggle`, `dialog` (`confirm` / `alert`), `notification` (`info`/`success`/`warning`/`error`), `chart` (`line`/`bar`/`area`/`pie`), settings field kind |
| `title` | string | `window` (or any root): native window title (default `clj-gpui`). Also `alert` / `group-box` / `dialog` / `alert-dialog` / `sheet` / `notification` / `sidebar` titles |
| `compact` | bool | `button` |
| `strikethrough` | bool | text |
| `shadow` | bool | layouts |
| `bg`, `border`, `border-bottom` | hex string | layouts / text |
| `align` | string | `center`, `start`, `end`. Also `table-head` / `table-cell` text alignment (`end` / `right` → Kit `text_right`) |
| `span` | number | `table-head` / `table-cell` Kit `col_span` (`0` / omitted is 1). Description-list item span stays on `items[]` |
| `justify` | string | `center`, `end`, `between` |
| `gap`, `padding`, `width`, `height`, `size`, `flex` | number | layout / spacer |
| `font-size` | number | text |
| `font-family` | string | text (e.g. `.SystemUIFont`) |
| `font-weight` | string (`thin`, `extralight`, `light`, `bold`, `semibold`, `medium`, …) | text |
| `color` | hex string (`#b83f45`) | text |
| `theme` | string | any node: `system` (default), `light`, `dark`, a shipped GPUI Kit palette such as `Tokyo Night` (kebab `tokyo-night` is the same), a custom ThemeSet family name, or a variant name. Nested nodes scope that subtree |
| `chrome` | string | `window` (or any root): `dev` (default, nREPL footer + `gpui-fps` HUD) or `app` (no host chrome) |
| `window-width`, `window-height` | number | `window` (or any root): native window size in pixels |

Functions never go on the wire. `gpui.runtime` replaces `fn?` values under `:on-click` / `:on-change` / `:on-submit` / `:on-double-click` / `:on-blur` / `:on-escape` / `:on-close` / `:on-copied` / `:on-ok` / `:on-cancel` / `:on-confirm` / `:on-open-change` with ids such as `"cb-2"`. Nested `:items` / `:options` / `:content` / `:trigger` / `:footer` are walked too. The registry is rebuilt on every export.

The native host paints these nodes with [GPUI Kit](https://gpui-kit.com) 0.6 (`gpui-kit` crate, `tree-sitter-languages`). Icon-bearing widgets (`icon`, `spinner`, `alert`, `select` chevron, `clipboard`) load SVGs from `gpui-kit-assets`. See [gpui-component.md](gpui-component.md) for the coverage inventory.

A `scroll` node is a vertical overflow viewport. Without `height`, the host gives it `flex: 1` and `min-height: 0` so it takes leftover space in a column instead of growing with its children. `height` is a fixed pixel viewport. `width` constrains the viewport; omitted, it fills the parent. `size` is a square viewport, matching other nodes (it wins over `width` / `height`). Visual styles (`padding`, `bg`, `border`, …) apply to the inner scroll body, not twice. `flex: 1` on other nodes also sets `min-height: 0`.

`list`, `data-table`, and `tree` use an outer clj-gpui wrapper for layout geometry and visual keys; the inner crate widget keeps `size_full()` for virtualization. `:size` is a square (it wins over `:width` / `:height`). Omitted `:width` fills the parent. Explicit `:height` is a pixel viewport. `:flex 1` fills leftover column height with `min-height: 0`. If height, size, and flex are all omitted, the host uses a default viewport (~200px list/tree, ~220px table) so crate `size_full()` does not collapse or steal the column.

`context-menu` is a flex column host (`v_flex` + `min-height: 0`), not a block `div`. A `:flex 1` list/table/tree inside a non-flex wrapper skips default viewport height and collapses. If the menu omitted `:flex`, leftover height is inherited from any flex-fill child so wrapping a listing does not drop it.

GPUI Kit 0.6 `Root::render` does not paint dialog / sheet / notification layers; the host calls `Root::render_dialog_layer`, `Root::render_sheet_layer`, and `Root::render_notification_layer` from `RootView`. Open/close for dialogs and the single crate sheet still goes through `WindowExt` on the next frame so `RootView::render` does not re-enter `Root`. Builders read a live spec cell (latest callback ids, title, body, children, footer) so an unrelated Clojure rerender cannot leave a stale `cb-7` on an already-open overlay. Overlay click dismisses dialogs/sheets by default (`:overlay-closable false` restores the crate lock). After overlay/Escape dismiss the host does not re-open until Clojure’s tree drops `open`. Notifications are a stack: presence shows unless `open` is false; unchanged title/message/variant/autohide is not re-pushed. Tree removal dismisses without a second `:on-close`. Static overlay children (dialog/sheet/dock panels) use a full path element id. `popover` is in-tree; its trigger must be a button (`Selectable`). Menu item clicks send the original Clojure id; item `:on-click` then menu `:on-change` is one batch. List `:on-change` is selection and `:on-confirm` is activation; both restore the original Clojure id and, on click/Enter, run as one batch before the next tree. Table single click is `:on-change`; a double-click is crate `SelectRow` then `DoubleClickedRow` from one `on_row_left_click`, batched as `:on-change` then `:on-confirm`.

Declarative `table` is Kit `Table` (not `DataTable`): content-sized, not virtualized, no selection callbacks. The wire is Kit primitives — `table-header` / `table-body` / `table-footer` / `table-row` / `table-head` / `table-cell` / `table-caption` — so `col_span`, alignment, and children belong on individual cells. `table-head` and `table-cell` children are ordinary clj-gpui nodes. Clojure `{:columns :rows :footer :caption}` shorthand expands into those primitives; column `:span` applies to the header cell only. `accessibility-label` is Kit `Table::accessibility_label` (a visible caption is not the accessible name). A host fallback still paints the older `options`/`items`/`variant: footer` shape. `combobox` keeps host `ComboboxState` (recreated if `searchable` / `multiple` change). Same-action Kit `Change` then `Confirm` is one `:on-change` + `:on-confirm` batch. Native `Change` updates the slot's cached selection so a Clojure echo of those ids does not call `set_selected_values` (which clears the search query). An item-collection change still rebuilds selection so renamed/removed options do not stick. A different Clojure value still overrides native state. `rating` is 0..=`max` (default 5); the host calls `.max` then `.value` because Kit clamps `.value` to the current max. `stepper` `value` is the selected item id.

`otp-input` `:on-change` fires only when every cell is filled. `editor` is Kit `Editor` / `EditorState` (highlighter language, no LSP). Dock panel bodies are the static overlay subset plus `markdown`/`chart`, not list/data-table/editor.

`spinner`, `badge`, and `clipboard` are not GPUI Kit `Styled` types. The host wraps them in a `div` that receives the usual layout and visual keys (`width`, `height`, `size`, `flex`, `padding`, `bg`, …). `accordion` and `description-list` use the same outer-owns-layout pattern, but the wrapper defaults to `flex-none` and full width so crate `size_full()` cannot steal leftover column height. Inner chrome is not styled twice.

Keywords in the tree become JSON strings (`:semibold` → `"semibold"`).

Put `:theme` on **any** node. The host does not choose a theme on its own:

* `:system` (default if omitted) follows the OS appearance, including later changes, using GPUI Kit Default Light / Default Dark
* `:light` pins Default Light for that subtree
* `:dark` pins Default Dark for that subtree
* a **named palette** such as `"Tokyo Night"` or `:ayu-light` calls GPUI Kit `Theme::apply_config` with that [theme](https://gpui-kit.com)
* a **custom ThemeSet** registered from Clojure (or loaded from JSON) is also a name: the variant (`"Catppuccin Violet Dark"`) pins that config; the family (`"Catppuccin Violet"`) picks the light or dark member from OS appearance

The host matches names case-insensitively and treats `-` / `_` as spaces, so `:tokyo-night`, `"tokyo night"`, and `"Tokyo Night"` are the same palette. Clojure `gpui.theme` uses that same identity for `register!` / `unregister!` / `json-str`.

Lookup order (first match wins): Clojure `:themes` on the render response, then `CLJ_GPUI_THEMES`, then `./themes`, then bundled JSON, then ThemeRegistry (`Default Light` / `Default Dark`). JSON directories are cached by file mtime; a change on disk is picked up on the next lookup. Duplicate variant names are deterministic: first ThemeSet in the Clojure array, then JSON files in sorted path order.

Drop extra GPUI Kit theme-set JSON files in a `themes/` directory next to the process working directory, or in `CLJ_GPUI_THEMES`. Those override bundled names. Clojure-registered sets override JSON.

A nested `:theme` wraps that subtree during layout and paint so siblings keep their own theme. The footer / waiting state follow the **root** node's `:theme` (usually the `window`).

GPUI Kit's `Theme` is process-global. Nested scopes work because layout, prepaint, and paint of a subtree run synchronously and restore the previous theme before the sibling is drawn. A second window would share that global; clj-gpui is still one window. There is no headless GPUI fixture here that can paint two themed buttons without a real window, so sibling isolation is enforced in the host's `ThemeScope` and covered on the Clojure side by serialization tests.

Window chrome is Clojure-owned on a `window` node (the host still reads these keys from whatever node is the tree root):

* `:title` sets the native window title (default `clj-gpui`)
* `:chrome :dev` (default) shows the nREPL footer and the `gpui-fps` HUD; `:chrome :app` hides host chrome
* `:window-width` / `:window-height` resize the window when those values change in the tree. On `ui/window`, Clojure maps `:width` / `:height` to these keys so they are not layout. If the root is not a `window`, root `:width` / `:height` are still used when the `window-*` keys are omitted.

The size is applied when the tree’s requested size changes, not on every user drag.
