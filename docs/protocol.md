# Clojure ↔ GPUI protocol

Newline-delimited JSON over a local TCP connection. Clojure listens;
the native host connects as a client.

Environment for the host process:

| Variable | Meaning |
|---|---|
| `CLJ_GPUI_PORT` | TCP port of the Clojure listener (required) |
| `CLJ_GPUI_HOST` | TCP host, default `127.0.0.1` |

Protocol version is **10**. Clojure sends it on `:ready`. The host refuses a mismatch.

## Handshake

1. Clojure binds `127.0.0.1:0`, then spawns the host with `CLJ_GPUI_PORT` set.
2. Host connects and waits for a `ready` line.
3. Host sends `render` (and later `callback` / `reload`) as JSON objects with a numeric `id`.
4. Clojure replies with `{"op":"response","id":…, …}`.

### `ready` (Clojure → host)

```json
{"op":"ready","protocol-version":10,"nrepl":7888,"app":"counter.app/app"}
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

v10 adds the rest of Kit's chart kinds (`radar`, `candlestick`, `sankey`) and Kit `BarChart` alignment (`left` / `right` for horizontal bars, plus `:labels` / `:value-axis`). The same protocol version also exposes Kit 0.6 chart builders on `ui/chart` (line/area/pie/bar/radar/candlestick/sankey options) without imposing extra limits Kit itself does not. Additive on the same version: `pagination`, `progress-circle`, `shimmer` (`ShimmerText`), `hover-card`, avatar image `src`, `avatar-group`, Select `SelectGroup` sections (`options[].items`) plus Select chrome (`cleanable`, `title-prefix`, `menu-width`, `menu-max-h`, `search-placeholder`, `empty`, `focus-ring`), Combobox chrome on those same fields plus `check-icon`, the chat family (`message`, `bubble`, `attachment`, `marker`, `message-scroller` and their slot types), and `nav-stack` / `nav-page` (Kit `NavStack`; Clojure-owned trail in `value`, transition seconds in `duration`, optional `motion`, opt-in `transition-style` / `overflow`, `on-forward-change` for Kit `forward_views()`, `reuse-forward` to force a fresh `push` when the new id equals the nearest forward entry, `replace-generation` for same-id Kit `replace()`). String `empty` / option `display` are the string forms of Kit `Select::empty` / `Combobox::empty` / `display_title`, not the full `IntoElement` / `AnyElement` APIs. Combobox `render_trigger` / `footer` and `ComboboxState::query` / `set_query` are not on the wire. A custom NavStack `item` renderer (full `NavPage` surface: mounted `view()` plus `index` / `phase` / `operation` / eased `progress`) is not on the wire. A v9 host would paint the new kinds as a line chart and ignore the extra fields.

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

## Node schema (version 10)

Every node is a JSON object. Unknown fields are ignored by the host.

| Field | Type | Used by |
|---|---|---|
| `type` | string | all (`window`, `label`, `button`, `vstack`, `hstack`, `spacer`, `checkbox`, `scroll`, `input`, `textarea`, `switch`, `toggle`, `radio-group`, `slider`, `progress`, `progress-circle`, `separator`, `spinner`, `tag`, `alert`, `skeleton`, `shimmer`, `kbd`, `link`, `group-box`, `badge`, `tabs`, `select`, `combobox`, `icon`, `clipboard`, `breadcrumb`, `avatar`, `avatar-group`, `accordion`, `description-list`, `dialog`, `alert-dialog`, `popover`, `hover-card`, `dropdown-menu`, `dropdown-button`, `context-menu`, `list`, `data-table`, `table`, `table-header`, `table-body`, `table-footer`, `table-row`, `table-head`, `table-cell`, `table-caption`, `tree`, `sheet`, `notification`, `number-input`, `otp-input`, `color-picker`, `date-picker`, `editor`, `virtual-list`, `chart`, `markdown`, `html`, `sidebar`, `settings`, `dock`, `resizable`, `rating`, `stepper`, `pagination`, `message`, `message-group`, `message-avatar`, `message-header`, `message-content`, `message-footer`, `bubble`, `bubble-content`, `bubble-group`, `bubble-reactions`, `attachment`, `attachment-media`, `attachment-media-overlay`, `attachment-content`, `attachment-title`, `attachment-description`, `attachment-actions`, `attachment-group`, `marker`, `marker-icon`, `marker-content`, `message-scroller`, `nav-stack`, `nav-page`) |
| `id` | string | optional stable identity, especially `input`, `textarea`, `slider`, `select`, `combobox`, `list`, `data-table`, `tree`, `dialog`, `alert-dialog`, `sheet`, `notification`, `editor`, `rating`, `stepper`, `pagination`, `progress-circle`, `shimmer`, `hover-card`, `message-scroller`, `nav-stack`, `nav-page`, `attachment`, `attachment-group`, `marker`, and each `message` row inside a scroller |
| `text` | string | `label`, `button`, `checkbox`, `input`, `textarea`, `switch`, `toggle`, `separator`, `tag`, `alert`, `kbd`, `link`, `clipboard`, `avatar`, `editor`, `markdown`, `html`, `number-input`, `table-head` / `table-cell` / `table-caption` (when they have no children), `bubble` / `marker` / `attachment-title` / `attachment-description` / `marker-content` (string form) |
| `placeholder` | string | `input`, `textarea`, `select`, `combobox`, `date-picker`, `number-input` |
| `children` | array of nodes | layouts, `scroll`, `group-box`, `badge`, `dialog`, `popover`, `hover-card`, `avatar-group` (avatar nodes), `context-menu`, `sheet`, `resizable`, declarative `table` and its Kit primitives (`table-header`, `table-body`, `table-footer`, `table-row`, `table-head`, `table-cell`, `table-caption`), chat primitives (`message`, `message-group`, `bubble`, `attachment`, `marker`, `message-scroller`, and their slot types), `nav-stack` (`nav-page` templates), `nav-page` |
| `items` / `options` | array of `{id,label,text,disabled,display,content,on-click,span,items,cells,separator,width,align,checked,icon,expanded,value,values,height,side,variant,min,max,step,color,stroke,fill,stroke-style,inner-radius,outer-radius,label-lines,open,high,low,close,source,target}` | `radio-group`, `select`, `combobox`, `tabs`, `breadcrumb`, `accordion`, `description-list` (`span` is description-list column span). Nested `items` are menu submenus / tree children / settings groups / **Select / Combobox `SelectGroup` sections**. Select option `display` is the string form of Kit `SelectItem::display_title` (`AnyElement` custom display is not wrapped). Data-table **columns** are `options`; **rows** are `items` with `cells`. Chart points use `value` (or `values` for radar/area series). Candlestick points use `open`/`high`/`low`/`close`. Pie slices may set `inner-radius` / `outer-radius` (Kit radius fns) and `color` (Kit `chart_2` when omitted). Area/radar series maps may set `stroke` (alias `color`) / `fill` / `stroke-style`. Sankey links use `source`/`target`/`value`; sankey nodes may set `label-lines`. Virtual-list rows may set `height`. Dock panels set `side` + `content`. Do not reuse `columns` (u32) for table column defs. Clojure `ui/table` shorthand expands to primitive children before the wire |
| `links` | array of items | `chart` `:sankey` flows (`source`, `target`, `value`) |
| `series` | array of items | `chart` `:radar` / `:area` series names, stroke/fill colors, and stroke styles, in value-index order |
| `trigger` | node | `popover`, `dropdown-menu` (usually a `button`); `dropdown-button` (action-half `button`, optional); `hover-card` (any widget) |
| `footer` | node | `sheet` footer. Not `message` — message footers are `message-footer` children |
| `on-click` | string callback id | `button`, `checkbox`, `label`, `vstack`, `hstack`, `link`, `notification`, `attachment` (needs `id` as well) |
| `on-double-click` | string callback id | `label` (0-arg; wins over `on-click` when `click_count >= 2`); `data-table` double-click row (row id) |
| `on-change` | string callback id | `input`/`textarea`/`editor` (string), `switch`/`toggle` (bool), `slider` (number, or `[start, end]` when range), `number-input`/`rating`/`pagination` (number), `select`/`combobox`/`radio-group`/`tabs`/`breadcrumb`/`accordion`/`list`/`data-table`/`tree`/`dropdown-menu`/`dropdown-button`/`context-menu`/`virtual-list`/`sidebar`/`stepper` (wire id; Clojure restores the original id). Accordion / combobox `:multiple` sends a JSON array in original item order. `otp-input` string when full. `color-picker` hex or `null`. `date-picker` ISO string or `[start, end]`. `settings` `{"id","value"}`. `resizable` array of px sizes |
| `on-release` | string callback id | `slider`: same payload as `on-change` (number, or `[start, end]` when range). Kit `SliderEvent::Release` after a real click/drag. Same-gesture Change then Release is one `:on-change` + `:on-release` batch (same generation). Programmatic `set_value` emits neither |
| `on-submit` | string callback id | `input` (Enter). `textarea`: when set, Enter submits and Shift+Enter inserts a newline (Kit `submit_on_enter`); omitted, both keys insert a newline |
| `on-blur` | string callback id | `input`, `textarea`, `otp-input`, `editor` (called with the current string) |
| `on-escape` | string callback id | `input`, `textarea`, `editor` (0-arg) |
| `on-close` | string callback id | `alert`, `dialog`, `alert-dialog`, `sheet`, `notification` (0-arg) |
| `on-ok` / `on-cancel` | string callback id | `dialog`, `alert-dialog` (0-arg; crate then closes and fires `on-close`) |
| `on-confirm` | string callback id | `list` (click / Enter; original Clojure row id). Arrows only fire `on-change`; click/Enter fire `on-change` then `on-confirm` as one batch. `data-table`: count-1 click is only `on-change` (end of the GPUI effect cycle). Count-2 `on_row_left_click` emits `SelectRow` then `DoubleClickedRow`, batched as `on-change` then `on-confirm` (or `on-double-click`). `combobox`: Kit may emit `Change` then `Confirm` for one pick; the host batches `:on-change` then `:on-confirm` against the same generation, then fetches one tree. Confirm without Change (dismiss) is `:on-confirm` only |
| `on-open-change` | string callback id | `popover` / `hover-card` (boolean); `dialog` / `alert-dialog` / `sheet` (`false` on dismiss) |
| `on-forward-change` | string callback id | `nav-stack`: Kit `forward_views()` as a JSON array of original page ids, nearest first. Empty after first mount is not sent; a later Push/Rebuild that clears forward still notifies `[]` |
| `on-copied` | string callback id | `clipboard` (copied string) |
| `focus` | bool | `input`, `textarea`: request keyboard focus |
| `checked` | bool | `checkbox`, `switch`, `toggle` |
| `value` | JSON number, string, array, bool, or null | `slider` (number, or `[start, end]` for range thumbs), `progress`/`progress-circle`/`number-input`/`rating`/`pagination` (number), `select`/`combobox`/`radio-group`/`tabs`/`list`/`data-table`/`tree`/`virtual-list`/`sidebar`/`stepper` (selected id or `null` to clear), `accordion` / combobox `:multiple` (id, `null`, or array of ids), `otp-input` (string), `color-picker` (hex), `date-picker` (ISO string or `[start, end]`), `nav-stack` (page ids root-first; omitted is the first catalog page; `[]` clears; an unknown id rejects the whole trail) |
| `min`, `max`, `step` | number | `slider`, `number-input`. `rating` uses `max` (default 5). Slider `step` is drag granularity; the host applies Clojure's controlled value even when it is off-step, then clamps to `min`/`max`. Logarithmic sliders need `min > 0` |
| `total` | number | `pagination` page count (Kit default 1; Kit clamps to ≥1) |
| `visible-pages` | number | `pagination` numbered buttons (Kit default 5). Omitted leaves Kit's default |
| `loading` | bool | `progress-circle` indeterminate animation. When true, Kit ignores `value`. `marker`: Kit `Marker::loading` |
| `orientation` | string | `radio-group`, `slider`, `separator`, `resizable`, `stepper`: `horizontal` (default) or `vertical`. `virtual-list` and `description-list`: `vertical` (default) or `horizontal`. `attachment`: Kit axis (`horizontal` default) |
| `columns` | number | `description-list`: grid columns 1–10 (default 1). The crate's own default is 3; the host does not use that |
| `disabled` | bool | buttons and most controls |
| `tooltip` | string | any node: GPUI Kit tooltip |
| `interactive` | bool | `chart` `:line` / `:bar` / `:area` / `:radar`: Kit hover tooltip via `.id(...)`. Default false (Kit `id: None`). Not the string `tooltip` field |
| `accessibility-label` | string | declarative `table`: Kit `Table::accessibility_label` (screen-reader name). A visible `table-caption` is not used as that name. `progress-circle`: Kit `ProgressCircle::accessibility_label` |
| `href` | string | `link` |
| `src` | string | `avatar`: Kit `ImageSource` (http URL or file path). Empty/omitted is initials or the placeholder icon. Remote http URLs need the host HTTP client (installed at startup) |
| `icon` | string | `icon`, `spinner` (kebab `circle-check`); `avatar` placeholder icon (Kit default User); `select` / `combobox` trigger chevron |
| `control-size` | string | `xs`/`small`/`medium`/`large` (Clojure `:size :small` is rewritten so pixel `:size` stays numeric). `dropdown-button`: omitted outer size inherits the inner action Button's size. Inner `trigger` may set `control-size` too |
| `count` | number | `badge`; `otp-input` length (default 6, clamped 1–12) |
| `dot` | bool | `badge`. `chart` `:line` / `:radar`: show vertices. Line default is false (Kit) |
| `dashed` | bool | `separator` |
| `outline` | bool | `tag`, `button`, `dropdown-button`. Also accepted as `variant: outline` on buttons |
| `searchable` | bool | `select` / `combobox`: show a filter field; host uses `SearchableVec` so typing actually filters. Nested `options[].items` are Kit `SelectGroup` sections (`IndexPath` section+row). Group titles are not selectable callback ids. Combobox defaults true in `ui/combobox`. `list`: filter rows by label |
| `cleanable` | bool | `select` / `combobox`: Kit `cleanable` (clear button when a value is selected) |
| `title-prefix` | string | `select`: Kit `Select::title_prefix` |
| `menu-width` | number | `select` / `combobox`: Kit `menu_width` in pixels. Omitted is Kit `Auto` |
| `menu-max-h` | number | `select` / `combobox`: Kit `menu_max_h` in pixels. Omitted is Kit's 20rem default |
| `search-placeholder` | string | `select` / `combobox`: Kit `search_placeholder` |
| `check-icon` | string | `combobox`: Kit `Combobox::check_icon` (selected-row mark, kebab icon name) |
| `empty` | string | `select` / `combobox`: string form of Kit `empty` when the list has no rows. Kit accepts arbitrary `IntoElement`; custom empty widgets are not wrapped |
| `focus-ring` | bool | `select` / `combobox`: Kit `FocusableExt::focus_ring`. Omitted leaves Kit's true |
| `open` | bool | `dialog`, `alert-dialog`, `popover`, `sheet`: controlled open (`:open?` in Clojure). Omitted/false dialogs/sheets are not shown. `notification`: omitted/true shows; `false` hides |
| `overlay-closable` | bool | `dialog`, `sheet`: click the dimmed overlay to dismiss (default true). `alert-dialog` is not backdrop-dismissible |
| `placement` | string | `sheet`: `left` / `right` / `top` / `bottom` (default `right`). `hover-card` / `dropdown-button`: Kit `Anchor` (`top-center` default on hover-card; `top-right` default on dropdown-button; also `top-left` / `top-right` / `bottom-*` / `left` / `right`) |
| `open-delay` | number | `hover-card`: seconds before show (Kit default 0.6). Omitted leaves Kit's default |
| `close-delay` | number | `hover-card`: seconds before hide (Kit default 0.3). Omitted leaves Kit's default |
| `appearance` | bool | `hover-card`: Kit default popover chrome (Kit default true). Omitted leaves Kit's default. `select` / `combobox`: Kit `appearance` (Kit default true) |
| `content-inset` | bool | `message-header` / `message-footer`: Kit `content_inset`. Omitted inherits from a ghost bubble |
| `status` | string | `attachment`: `pending` / `uploading` / `processing` / `failed` / `complete` (default) |
| `scrollbar` | bool | `message-scroller`: Kit `scrollbar`. Omitted leaves Kit's true |
| `jump-button` | bool | `message-scroller`: Kit `jump_button`. Omitted leaves Kit's true |
| `jump-button-label` | string | `message-scroller`: Kit `with_jump_button_label` (tooltip only). The Button's visible / accessible name is `jump-button-renderer` `text` |
| `jump-button-transition` | number | `message-scroller`: Kit `with_jump_button_transition` in seconds. Omitted leaves Kit's 200ms. Zero disables the transition |
| `bottom-fade` | hex string | `message-scroller`: Kit `with_bottom_fade` |
| `loading-style` | string | `marker`: `spinner` (default) / `shimmer` |
| `role` | string | `marker`: `status` / `alert` / `log`. Takes effect together with `id` |
| `stack-style` | object | `message`: Kit `with_stack_style`. Nested style map (`gap`, `padding`, `bg`, …). Omitted `type` is allowed |
| `shimmer-style` | object | `attachment-title` / `marker`: Kit `ShimmerStyle` (`duration`, `highlight-color`, `spread` / `spread-px`, `reverse`, `once`) |
| `separator-style` | object | `marker`: Kit `separator_style` nested style map |
| `content-style` | object | `message-scroller`: Kit `with_content_style` |
| `list-style` | object | `message-scroller`: Kit `with_list_style` |
| `row-style` | object | `message-scroller`: Kit `with_row_style` |
| `jump-button-style` | object | `message-scroller`: Kit `with_jump_button_style` |
| `jump-button-renderer` | object | `message-scroller`: Kit `with_jump_button_renderer` chrome (`text` / Clojure `:label` is Kit `Button::label`; also `variant`, `control-size`, `icon`, `tooltip`) |
| `limit` | number | `avatar-group`: max visible avatars (Kit default 3). Omitted leaves Kit's default. Forwarded unclamped |
| `ellipsis` | bool | `avatar-group`: show a ⋯ overflow avatar when there are more than `limit` (Kit default false) |
| `autohide` | bool | `notification` (default true) |
| `language` | string | `editor` highlighter (`rust`, `json`, `markdown`, …; default `text`). Kit's `tree-sitter-languages` bundle is enabled; there is no Clojure grammar |
| `rows` | number | `textarea` visible height (default 3) |
| `masked` | bool | `otp-input` |
| `collapsed` | bool | `sidebar` |
| `side` | string | `sidebar` (`left`/`right`); dock item `left`/`right`/`bottom`/`center`. `bubble-reactions`: Kit `BubbleReactionSide` (`top` / `bottom`, default `bottom`) |
| `format` | string | `markdown` vs `html` (node `type` `html` is enough) |
| `range` | bool | `date-picker` range mode. `slider`: two thumbs (`true`, or a 2-number `value`) |
| `multiple` | bool | `accordion`, `combobox` |
| `message` | string | `alert` (alias of `text`) |
| `shape` | string | `checkbox`: `circle` for a round toggle |
| `primary` | bool | `button` / `dropdown-button` (alias for `variant: primary` when `variant` is omitted) |
| `selected` | bool | `button` / `dropdown-button`: Kit `Selectable` chrome. Not list / table / tree selection (those Clojure `:selected` keys become `value`) |
| `variant` | string | `button` / `dropdown-button`: Kit `ButtonVariants` (`primary`, `secondary`, `danger`, `warning`, `success`, `info`, `ghost`, `link`, `text`). `tag`, `alert`, `tabs`, `group-box`, `toggle`, `dialog` (`confirm` / `alert`), `notification` (`info`/`success`/`warning`/`error`), `chart` (`line`/`bar`/`area`/`pie`/`radar`/`candlestick`/`sankey`), settings field kind. `bubble`: `filled` (default) / `secondary` / `muted` / `tinted` / `outline` / `ghost` / `destructive`. `marker`: `plain` / `separator` / `border` |
| `alignment` | string | `chart` `:bar`: Kit `BarAlignment` (`bottom` default, `top`, `left`, `right`). `left` is horizontal bars growing right. `message` / `bubble` / `bubble-reactions`: Kit `MessageAlignment` (`start` / `end`) |
| `label-axis` | bool | `chart`: band-axis labels (default true) |
| `value-axis` | bool | `chart`: value-axis tick labels (default false) |
| `tick-margin` | number | `chart`: stride over band-axis category labels. Kit does not clamp; the host forwards `max(1)` so `0` cannot divide by zero |
| `value-tick-count` | number | `chart`: value-axis intervals (default 4) |
| `grid` | bool | `chart`: grid lines (default true) |
| `labels` | bool | `chart` `:bar`: paint numeric labels on bars (default false). `:pie`: draw slice labels |
| `name` | string | `chart` `:line` / `:bar`: Kit tooltip series name |
| `stroke` | hex string | `chart` `:line`: series stroke. `:area` series maps: per-series stroke (`:color` is an alias). Unspecified area series keep Kit `chart_2`. Not layout `color` |
| `stroke-style` | string | `chart` `:line` / `:area`: `natural` (default), `linear`, `step-after`. Unspecified area series keep Kit `natural` |
| `x-axis` | bool | `chart` `:line` / `:area` / `:candlestick`: category axis (default true). Not an alias of bar `label-axis` |
| `corner-radius` / `corner-radii` | number or `{top-left,top-right,bottom-right,bottom-left}` | `chart` `:bar`: Kit `Corners` |
| `fill-gradient` | bool, `bar`, `chart`, or two `{color,at}` stops | `chart` `:bar`: Kit `fill_gradient` (clears solid `fill`). Stop `at` is forwarded unclamped; Kit clips/interpolates |
| `fill-gradient-mode` | string | `chart` `:bar`: `bar` (default) or `chart` when `fill-gradient` is true |
| `inner-radius` | number | `chart` `:pie`: donut hole in pixels (Kit default 0). Also a per-slice item field for Kit `inner_radius_fn` |
| `outer-radius` | number | `chart` `:pie` / `:radar`: pixels. Omitted pie paint forwards Kit's layout default (`height × 0.4`) because Kit's paint path still uses 0 and drops the ring. Also a per-slice item field for Kit `outer_radius_fn` |
| `pad-angle` | number | `chart` `:pie` |
| `label-color` | hex string | `chart` `:pie` / `:radar` |
| `label-line-color` | hex string | `chart` `:pie` leader lines |
| `label-gap` | number | `chart` `:pie` / `:radar` / `:sankey` |
| `grid-levels` | number | `chart` `:radar`: concentric rings (Kit default 4, ≥1) |
| `body-width-ratio` | number | `chart` `:candlestick`: body width vs band (Kit default 0.8). Forwarded unclamped |
| `node-align` | string | `chart` `:sankey`: `justify` (default), `left`, `right`, `center` |
| `value-scale` | string | `chart` `:sankey`: `linear` (default) or `sqrt` |
| `scale` | string | `slider`: `linear` (default / omitted) or `logarithmic` (`log`). Not sankey `value-scale`. Logarithmic needs `min > 0`; otherwise the host keeps linear and warns |
| `node-width` / `node-padding` / `iterations` / `node-corner-radius` / `link-opacity` / `min-link-width` | number | `chart` `:sankey` layout |
| `node-label` / `value-label` | bool | `chart` `:sankey`: convenience labels (default true). Custom item `label-lines` take precedence |
| `title` | string | `window` (or any root): native window title (default `clj-gpui`). Also `alert` / `group-box` / `dialog` / `alert-dialog` / `sheet` / `notification` / `sidebar` titles |
| `compact` | bool | `button`, `pagination` (prev/next only), `dropdown-button` (action half) |
| `duration` | number | `shimmer`: sweep duration in seconds (Kit default 2). Omitted leaves Kit's default. `nav-stack`: Kit `Transition` seconds. Omitted / ≤0 is immediate |
| `motion` | string | `nav-stack`: Kit `NavMotion`. `immediate` skips the stack transition. Omitted / `animated` runs the transition when `duration` is set and > 0 |
| `transition-style` | string | `nav-stack`: convenience Kit `item` renderer. `slide` is the showcase slide. Omitted keeps Kit's default unchanged `NavPage` renderer. Independent of `duration` |
| `overflow` | string | `nav-stack`: `hidden` clips. Omitted does not clip. Not AvatarGroup ellipsis |
| `overflow-hidden` | bool | `nav-stack`: explicit clip opt-in. Omitted / false does not clip |
| `reuse-forward` | bool | `nav-stack`: omitted / true reuses the nearest retained forward entry (`forward()`). `false` forces a fresh `push()` even when the new page id equals that nearest id, and discards the forward branch |
| `replace-generation` | number or string | `nav-stack`: same-id Kit `replace()` token. Changing it on the current `CljNavPage` entity creates a fresh page entity and calls `replace()` (forward is kept). Unchanged across rerenders is a no-op. Bound to that entity (not the catalog id). Navigation to another history entry keeps the old binding until the first later token change, which rebinds; only the next change may `replace()` |
| `spread` | number | `shimmer`: relative highlight half-width (Kit default 0.3; Kit clamps 0.05..=1). Forwarded unclamped |
| `spread-px` | number | `shimmer`: absolute highlight half-width in pixels. Wins over `spread` when both are set |
| `reverse` | bool | `shimmer`: right-to-left sweep. `slider`: fill from thumb to max (single-value only; ignored for range) |
| `once` | bool | `shimmer`: one sweep instead of a loop |
| `highlight-color` | hex string | `shimmer` sweep color. Not layout `color` |
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
| `color` | hex string (`#b83f45`) | text; `progress-circle` arc (Kit theme `progress_bar` when omitted) |
| `theme` | string | any node: `system` (default), `light`, `dark`, a shipped GPUI Kit palette such as `Tokyo Night` (kebab `tokyo-night` is the same), a custom ThemeSet family name, or a variant name. Nested nodes scope that subtree |
| `chrome` | string | `window` (or any root): `dev` (default, nREPL footer + `gpui-fps` HUD) or `app` (no host chrome) |
| `window-width`, `window-height` | number | `window` (or any root): native window size in pixels |

Functions never go on the wire. `gpui.runtime` replaces `fn?` values under `:on-click` / `:on-change` / `:on-release` / `:on-submit` / `:on-double-click` / `:on-blur` / `:on-escape` / `:on-close` / `:on-copied` / `:on-ok` / `:on-cancel` / `:on-confirm` / `:on-open-change` / `:on-forward-change` with ids such as `"cb-2"`. Nested `:items` / `:options` / `:links` / `:series` / `:content` / `:trigger` / `:footer` are walked too. The registry is rebuilt on every export.

The native host paints these nodes with [GPUI Kit](https://gpui-kit.com) 0.6 (`gpui-kit` crate, `tree-sitter-languages`). Icon-bearing widgets (`icon`, `spinner`, `alert`, `select` chevron, `clipboard`) load SVGs from `gpui-kit-assets`. See [gpui-component.md](gpui-component.md) for the coverage inventory.

A `scroll` node is a vertical overflow viewport. Without `height`, the host gives it `flex: 1` and `min-height: 0` so it takes leftover space in a column instead of growing with its children. `height` is a fixed pixel viewport. `width` constrains the viewport; omitted, it fills the parent. `size` is a square viewport, matching other nodes (it wins over `width` / `height`). Visual styles (`padding`, `bg`, `border`, …) apply to the inner scroll body, not twice. `flex: 1` on other nodes also sets `min-height: 0`.

`list`, `data-table`, and `tree` use an outer clj-gpui wrapper for layout geometry and visual keys; the inner crate widget keeps `size_full()` for virtualization. `:size` is a square (it wins over `:width` / `:height`). Omitted `:width` fills the parent. Explicit `:height` is a pixel viewport. `:flex 1` fills leftover column height with `min-height: 0`. If height, size, and flex are all omitted, the host uses a default viewport (~200px list/tree, ~220px table) so crate `size_full()` does not collapse or steal the column.

`context-menu` is a flex column host (`v_flex` + `min-height: 0`), not a block `div`. A `:flex 1` list/table/tree inside a non-flex wrapper skips default viewport height and collapses. If the menu omitted `:flex`, leftover height is inherited from any flex-fill child so wrapping a listing does not drop it.

GPUI Kit 0.6 `Root::render` does not paint dialog / sheet / notification layers; the host calls `Root::render_dialog_layer`, `Root::render_sheet_layer`, and `Root::render_notification_layer` from `RootView`. Open/close for dialogs and the single crate sheet still goes through `WindowExt` on the next frame so `RootView::render` does not re-enter `Root`. Builders read a live spec cell (latest callback ids, title, body, children, footer) so an unrelated Clojure rerender cannot leave a stale `cb-7` on an already-open overlay. Overlay click dismisses dialogs/sheets by default (`:overlay-closable false` restores the crate lock). After overlay/Escape dismiss the host does not re-open until Clojure’s tree drops `open`. Notifications are a stack: presence shows unless `open` is false; unchanged title/message/variant/autohide is not re-pushed. Tree removal dismisses without a second `:on-close`. Static overlay children (dialog/sheet/dock panels) use a full path element id. `popover` is in-tree; its trigger must be a button (`Selectable`). `hover-card` is also in-tree: hover-driven (not `:open?`), trigger is any widget, omitted delays keep Kit's 0.6s / 0.3s. Menu item clicks send the original Clojure id; item `:on-click` then menu `:on-change` is one batch. List `:on-change` is selection and `:on-confirm` is activation; both restore the original Clojure id and, on click/Enter, run as one batch before the next tree. Table single click is `:on-change`; a double-click is crate `SelectRow` then `DoubleClickedRow` from one `on_row_left_click`, batched as `:on-change` then `:on-confirm`.

Declarative `table` is Kit `Table` (not `DataTable`): content-sized, not virtualized, no selection callbacks. The wire is Kit primitives — `table-header` / `table-body` / `table-footer` / `table-row` / `table-head` / `table-cell` / `table-caption` — so `col_span`, alignment, and children belong on individual cells. `table-head` and `table-cell` children are ordinary clj-gpui nodes. Clojure `{:columns :rows :footer :caption}` shorthand expands into those primitives; column `:span` applies to the header cell only. `accessibility-label` is Kit `Table::accessibility_label` (a visible caption is not the accessible name). A host fallback still paints the older `options`/`items`/`variant: footer` shape. `select` keeps host `SelectState` (recreated if `searchable` / grouped-ness / option fingerprint change). Controlled id changes use Kit `set_selected_value` so a live search query is not indexed with a full-list `IndexPath`. Native `Confirm` updates the slot's cached selection first so a Clojure echo is a no-op and does not clear an in-progress query. `:focus-ring` is Kit `FocusableExt` (omit = Kit true). String `:empty` / option `:display` are not Kit's full `IntoElement` / `AnyElement` APIs; custom row/section `render` is later custom rendering. Group titles are not in the Select / Combobox callback id map. `combobox` keeps host `ComboboxState` (recreated if `searchable` / `multiple` / grouped-ness change). Nested `options[].items` are Kit `SelectGroup` sections (`SearchableVec<SelectGroup>`; leaf values stay `SharedString`). Grouped collection fingerprint changes rebuild the slot so query text and matched sections agree (same Rebuild rule as Select). Flat comboboxes still `set_items` plus `set_selected_values` so renamed/removed options do not stick. Same-action Kit `Change` then `Confirm` is one `:on-change` + `:on-confirm` batch. Native `Change` updates the slot's cached selection so a Clojure echo of those ids does not call `set_selected_values` (which clears the search query). A different Clojure value still overrides native state. Combobox chrome (`cleanable`, `menu-width`, `menu-max-h`, `search-placeholder`, `icon`, `check-icon`, `appearance`, `focus-ring`, string `empty`) is forwarded. Remaining Combobox surface is `render_trigger`, `footer`, empty as `IntoElement`, and `ComboboxState::query` / `set_query` (programmatic search text). `rating` is 0..=`max` (default 5); the host calls `.max` then `.value` because Kit clamps `.value` to the current max. `stepper` `value` is the selected item id. `pagination` `value` is the 1-based page; `total` is the page count; `:on-change` is the new page number. `progress-circle` is 0–100 like `progress`, plus `loading` and optional children inside the ring. `shimmer` is Kit `ShimmerText`; omitted duration/spread/highlight keep Kit defaults. `hover-card` is Kit `HoverCard`: hover-driven, optional delays, any-widget trigger, children as the card body. `dropdown-button` is Kit `DropdownButton` (action half + caret menu; same item `:on-click` then menu `:on-change` batch). `avatar` `src` is a Kit image source; `avatar-group` stacks avatar children with Kit `limit` / `ellipsis`. Slider `value` may be `[start, end]` for range thumbs; `scale` is `linear` or `logarithmic` (log needs `min > 0`).

Chat `message` / `bubble` / `attachment` / `marker` are Kit `RenderOnce` primitives on the wire (same completeness rule as `table`). A `bubble` child of `message-content` uses Kit `.bubble` so Ghost still strips header/footer inset. Message `:footer` is a `message-footer` child, not the sheet `footer` field. Direct children after an explicit `bubble-content` append through Kit `ParentElement` so the content slot's `StyleRefinement` is kept. `attachment-media` applies `with_size` only when `control-size` is set (omitted inherits the parent Attachment size). Ordinary media children are `.child`; Kit `.overlay` is `attachment-media-overlay`. Nested style maps cover `with_stack_style`, `ShimmerStyle`, `separator_style`, and MessageScroller `with_*_style` / `with_jump_button_renderer`. Chat nodes use the same visual/layout style vocabulary as ordinary widgets. MessageScroller root `Styled` applies to the Kit widget; the host wrapper keeps viewport/box geometry. `jump-button-label` is the jump button tooltip; renderer `text` (Clojure `:label`) is Kit `Button::label`. `message-scroller` keeps host `MessageScrollerState` (tail follow on). Row identity is `id` or `idx:{n}`; prepend/append without `reset` needs a stable `id`. Row fingerprints ignore generated callback ids; append/prepend also remeasures when a surviving row changed. Scroller rows are the static overlay subset plus this chat family. Remaining: `scroll_to_item` / `scroll_to_end`, and Kit's arbitrary row renderer (`IntoElement` / stateful nodes).

`nav-stack` keeps host `NavStackState`. Clojure `value` is the page-id trail (root first); children are `nav-page` catalog templates. Omitted `value` is the first page id; only `[]` clears. An explicit trail with an unknown page id is rejected (native stack unchanged, host warning) rather than dropping unknown ids. The host diffs the last trail against the desired trail and the host-side forward branch (Kit-internal order, last = nearest) and returns a plan of Kit `push` / `pop` / `forward` / `pop_to_root` / `replace` steps. The longest matching active prefix is kept; Rebuild (`clear` + immediate pushes) is last resort (empty current, explicit `[]`, or a root id that cannot be `replace`d). Multi-step pops keep popped entities on the forward branch; restoring that same trail is the same number of `forward` calls. Intermediate plan steps use Immediate; the last step uses the node's `motion`. Growing by an id that matches the nearest forward entry is `forward` (restore the retained entity) unless `reuse-forward` is `false`, which forces a fresh `push` and discards the remainder of the forward branch. Push clears host forward when Kit does; replace preserves it. `replace-generation` (integer or string) requests a same-id Kit `replace()`: changing the token while the current `CljNavPage` entity stays the same creates a fresh `CljNavPage` and calls `replace()` with the node's motion; leaving it unchanged only `replace_live`s existing pages. The host binds the token to that entity (not the catalog page id) so a later navigation cannot apply a stale bump to a different history entry, including another instance of the same page id. Ordinary rerenders that keep the trail and token unchanged do not transfer the binding to the newly current entity; the first later token change on that entity rebinds, and only the following change may `replace()`. Setting `value` to just the root from depth > 2 is one `pop_to_root` (popped active entries join forward in Kit order). Each stack entry is a distinct `CljNavPage` entity instantiated from the catalog template (repeated ids are two history entries). Live cells on both the active trail and the forward branch are replaced with `Context::notify()` so an unchanged trail still picks up regenerated callback ids, including after a later `forward`. `on-forward-change` is Kit `forward_views()` as a JSON array of page ids, nearest first; it is deferred (`cx.defer_in`) so the callback cannot re-enter `export-tree` during `RootView::render`. Empty after first mount is not sent; a later Push/Rebuild that clears forward still notifies `[]`. Duplicate catalog template ids warn once (lookup uses the last template). `duration` is Kit `Transition` seconds only; `motion: immediate` skips animation. `transition-style: slide` is the opt-in showcase `item` renderer; omitted keeps Kit's default `NavPage` renderer. `overflow: hidden` / `overflow-hidden: true` clip; omitted does not. Remaining: a custom `item` renderer with the complete `NavPage` surface — the mounted `view()` plus `index`, `phase`, `operation`, and eased `progress`. The renderer must keep access to that retained page element rather than only exposing metadata or painting unrelated replacement content. Pages cannot re-enter `RootView`; they paint the overlay static subset plus the chat family.

`otp-input` `:on-change` fires only when every cell is filled. `editor` is Kit `Editor` / `EditorState` (highlighter language, no LSP). Dock panel bodies and `nav-page` bodies are the static overlay subset plus `markdown`/`chart` / the chat family, not list/data-table/editor. `chart` kinds are Kit's: `line`, `bar` (including `:alignment left` horizontal bars), `area`, `pie`, `radar`, `candlestick`, `sankey`. Convenience helpers may simplify Kit; `ui/chart` must not hide Kit 0.6 builders or add limits Kit does not. Hover tooltips stay off unless `:interactive true`. Unspecified area series and pie slices keep Kit `chart_2` (area fill at 0.4 opacity). Radar `:content` paints ordinary clj-gpui widgets (badge, avatar, avatar-group, hover-card, pagination, progress-circle, shimmer, …), not only the static overlay subset. Horizontal bar default height grows with category count on both the RootView and Dock wrappers. Stacked bars are a story-only `Plot`, not a Kit widget; they are not wrapped.

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
