# Clojure ↔ GPUI protocol

Newline-delimited JSON over a local TCP connection. Clojure listens;
the native host connects as a client.

Environment for the host process:

| Variable | Meaning |
|---|---|
| `CLJ_GPUI_PORT` | TCP port of the Clojure listener (required) |
| `CLJ_GPUI_HOST` | TCP host, default `127.0.0.1` |

Protocol version is **6**. Clojure sends it on `:ready`. The host refuses a mismatch.

## Handshake

1. Clojure binds `127.0.0.1:0`, then spawns the host with `CLJ_GPUI_PORT` set.
2. Host connects and waits for a `ready` line.
3. Host sends `render` (and later `callback` / `reload`) as JSON objects with a numeric `id`.
4. Clojure replies with `{"op":"response","id":…, …}`.

### `ready` (Clojure → host)

```json
{"op":"ready","protocol-version":6,"nrepl":7888,"app":"counter.app/app"}
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

`themes` is always an array of ThemeSet objects registered in the Clojure process (`gpui.theme/register!`). Each set is gpui-component JSON: required `name` / `mode` / `colors`, plus any other ThemeConfig fields Clojure preserved (`highlight`, `font.size`, …). `[]` means the host should drop previously installed Clojure ThemeSets. UI nodes still name a palette with the string `theme` field; they do not embed the color map.

On an application exception Clojure still returns `ok: true` with an error UI tree so the window can paint.

### `callback`

```json
{"op":"callback","id":2,"callback-id":"cb-2"}
```

Invokes the real Clojure IFn that was registered when the current tree was exported, then the host **always** issues another `render`. That second fetch carries text-field submit `seq` and covers handlers that do not touch an atom. While the callback runs, Clojure does not send `request-render` from `r/atom` watches, so a typical `swap!` click is one paint, not two. nREPL updates, hot reload, and `ui/request-render!` still use `request-render`.

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

v6 added product widgets: `sheet`, `notification`, `number-input`, `otp-input`, `color-picker`, `date-picker`, `editor`, `virtual-list`, `chart`, `markdown`, `html`, `sidebar`, `settings`, `dock`, `resizable`. Overlay sheet/notification reuse the v5 live-spec + next-frame `WindowExt` pattern. A v5 host would paint “Unknown GPUI node” for those types. Editor and text-field `:on-change` coalesce grouped crate `Change` emits (undo/redo, burst typing) into one callback per registry generation so a stale `cb-N` cannot surface as `unknown callback`.

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

## Node schema (version 5)

Every node is a JSON object. Unknown fields are ignored by the host.

| Field | Type | Used by |
|---|---|---|
| `type` | string | all (`window`, `label`, `button`, `vstack`, `hstack`, `spacer`, `checkbox`, `scroll`, `text-field`, `switch`, `toggle`, `radio-group`, `slider`, `progress`, `divider`, `spinner`, `tag`, `alert`, `skeleton`, `kbd`, `link`, `group-box`, `badge`, `tabs`, `select`, `icon`, `clipboard`, `breadcrumb`, `avatar`, `accordion`, `description-list`, `dialog`, `popover`, `dropdown-menu`, `context-menu`, `list`, `table`, `tree`, `sheet`, `notification`, `number-input`, `otp-input`, `color-picker`, `date-picker`, `editor`, `virtual-list`, `chart`, `markdown`, `html`, `sidebar`, `settings`, `dock`, `resizable`) |
| `id` | string | optional stable identity, especially `text-field`, `slider`, `select`, `list`, `table`, `tree`, `dialog`, `sheet`, `notification`, `editor` |
| `text` | string | `label`, `button`, `checkbox`, `text-field`, `switch`, `toggle`, `divider`, `tag`, `alert`, `kbd`, `link`, `clipboard`, `avatar`, `editor`, `markdown`, `html`, `number-input` |
| `placeholder` | string | `text-field`, `select`, `date-picker`, `number-input` |
| `children` | array of nodes | layouts, `scroll`, `group-box`, `badge`, `dialog`, `popover`, `context-menu`, `sheet`, `resizable` |
| `items` / `options` | array of `{id,label,text,disabled,content,on-click,span,items,cells,separator,width,checked,icon,expanded,value,height,side,variant,min,max,step}` | `radio-group`, `select`, `tabs`, `breadcrumb`, `accordion`, `description-list` (`span` is description-list column span). Nested `items` are menu submenus / tree children / settings groups. Table **columns** are `options`; table **rows** are `items` with `cells`. Chart points use `value`. Virtual-list rows may set `height`. Dock panels set `side` + `content`. Do not reuse `columns` (u32) for table column defs |
| `trigger` | node | `popover`, `dropdown-menu` (usually a `button`) |
| `footer` | node | `sheet` footer |
| `on-click` | string callback id | `button`, `checkbox`, `label`, `vstack`, `hstack`, `link`, `notification` |
| `on-double-click` | string callback id | `label` (0-arg; wins over `on-click` when `click_count >= 2`); `table` double-click row (row id) |
| `on-change` | string callback id | `text-field` (string), `switch`/`toggle` (bool), `slider`/`number-input` (number), `select`/`radio-group`/`tabs`/`breadcrumb`/`accordion`/`list`/`table`/`tree`/`dropdown-menu`/`context-menu`/`virtual-list`/`sidebar` (wire id; Clojure restores the original id). Accordion `:multiple` sends a JSON array in original item order. `otp-input` string when full. `color-picker` hex or `null`. `date-picker` ISO string or `[start, end]`. `editor` string. `settings` `{"id","value"}`. `resizable` array of px sizes |
| `on-submit` | string callback id | `text-field` (Enter; called with the field string) |
| `on-blur` | string callback id | `text-field`, `otp-input`, `editor` (called with the current string) |
| `on-escape` | string callback id | `text-field`, `editor` (0-arg) |
| `on-close` | string callback id | `alert`, `dialog`, `sheet`, `notification` (0-arg) |
| `on-ok` / `on-cancel` | string callback id | `dialog` (0-arg; crate then closes and fires `on-close`) |
| `on-confirm` | string callback id | `list` (click / Enter; original Clojure row id). Arrows only fire `on-change`; click/Enter fire `on-change` then `on-confirm` as one batch. `table`: count-1 click is only `on-change` (end of the GPUI effect cycle). Count-2 `on_row_left_click` emits `SelectRow` then `DoubleClickedRow`, batched as `on-change` then `on-confirm` (or `on-double-click`) |
| `on-open-change` | string callback id | `popover` (boolean); `dialog` / `sheet` (`false` on dismiss) |
| `on-copied` | string callback id | `clipboard` (copied string) |
| `focus` | bool | `text-field`: request keyboard focus |
| `checked` | bool | `checkbox`, `switch`, `toggle` |
| `value` | JSON number, string, array, bool, or null | `slider`/`progress`/`number-input` (number), `select`/`radio-group`/`tabs`/`list`/`table`/`tree`/`virtual-list`/`sidebar` (selected id or `null` to clear), `accordion` (id, `null`, or array of ids when `multiple`), `otp-input` (string), `color-picker` (hex), `date-picker` (ISO string or `[start, end]`) |
| `min`, `max`, `step` | number | `slider`, `number-input`. Slider `step` is drag granularity; the host applies Clojure's controlled value even when it is off-step, then clamps to `min`/`max` |
| `orientation` | string | `radio-group`, `slider`, `divider`, `resizable`: `horizontal` (default) or `vertical`. `virtual-list` and `description-list`: `vertical` (default) or `horizontal` |
| `columns` | number | `description-list`: grid columns 1–10 (default 1). The crate's own default is 3; the host does not use that |
| `disabled` | bool | buttons and most controls |
| `tooltip` | string | any node: gpui-component tooltip |
| `href` | string | `link` |
| `icon` | string | `icon`, `spinner` (kebab `circle-check`) |
| `control-size` | string | `xs`/`small`/`medium`/`large` (Clojure `:size :small` is rewritten so pixel `:size` stays numeric) |
| `count` | number | `badge`; `otp-input` length (default 6, clamped 1–12) |
| `dot` | bool | `badge` |
| `dashed` | bool | `divider` |
| `outline` | bool | `tag` |
| `searchable` | bool | `select`: show a filter field; host uses `SearchableVec` so typing actually filters. `list`: filter rows by label |
| `open` | bool | `dialog`, `popover`, `sheet`: controlled open (`:open?` in Clojure). Omitted/false dialogs/sheets are not shown. `notification`: omitted/true shows; `false` hides |
| `overlay-closable` | bool | `dialog`, `sheet`: click the dimmed overlay to dismiss (default true) |
| `placement` | string | `sheet`: `left` / `right` / `top` / `bottom` (default `right`) |
| `autohide` | bool | `notification` (default true) |
| `language` | string | `editor` highlighter (`rust`, `clojure`, …; default `text`) |
| `masked` | bool | `otp-input` |
| `collapsed` | bool | `sidebar` |
| `side` | string | `sidebar` (`left`/`right`); dock item `left`/`right`/`bottom`/`center` |
| `format` | string | `markdown` vs `html` (node `type` `html` is enough) |
| `range` | bool | `date-picker` range mode |
| `multiple` | bool | `accordion` |
| `message` | string | `alert` (alias of `text`) |
| `shape` | string | `checkbox`: `circle` for a round toggle |
| `primary` | bool | `button` (alias for `variant: primary`) |
| `variant` | string | `button`, `tag`, `alert`, `tabs`, `group-box`, `toggle`, `dialog` (`confirm` / `alert`), `notification` (`info`/`success`/`warning`/`error`), `chart` (`line`/`bar`/`area`/`pie`), settings field kind |
| `title` | string | `window` (or any root): native window title (default `clj-gpui`). Also `alert` / `group-box` / `dialog` / `sheet` / `notification` / `sidebar` titles |
| `compact` | bool | `button` |
| `strikethrough` | bool | text |
| `shadow` | bool | layouts |
| `bg`, `border`, `border-bottom` | hex string | layouts / text |
| `align` | string | `center`, `start`, `end` |
| `justify` | string | `center`, `end`, `between` |
| `gap`, `padding`, `width`, `height`, `size`, `flex` | number | layout / spacer |
| `font-size` | number | text |
| `font-family` | string | text (e.g. `.SystemUIFont`) |
| `font-weight` | string (`thin`, `extralight`, `light`, `bold`, `semibold`, `medium`, …) | text |
| `color` | hex string (`#b83f45`) | text |
| `theme` | string | any node: `system` (default), `light`, `dark`, a shipped gpui-component palette such as `Tokyo Night` (kebab `tokyo-night` is the same), a custom ThemeSet family name, or a variant name. Nested nodes scope that subtree |
| `chrome` | string | `window` (or any root): `dev` (default, nREPL footer) or `app` (no host chrome) |
| `window-width`, `window-height` | number | `window` (or any root): native window size in pixels |

Functions never go on the wire. `gpui.runtime` replaces `fn?` values under `:on-click` / `:on-change` / `:on-submit` / `:on-double-click` / `:on-blur` / `:on-escape` / `:on-close` / `:on-copied` / `:on-ok` / `:on-cancel` / `:on-confirm` / `:on-open-change` with ids such as `"cb-2"`. Nested `:items` / `:options` / `:content` / `:trigger` / `:footer` are walked too. The registry is rebuilt on every export.

The native host paints these nodes with [gpui-component](https://crates.io/crates/gpui-component) 0.5.1. Icon-bearing widgets (`icon`, `spinner`, `alert`, `select` chevron, `clipboard`) load SVGs from `gpui-component-assets` 0.5.1. See [gpui-component.md](gpui-component.md) for the coverage inventory.

A `scroll` node is a vertical overflow viewport. Without `height`, the host gives it `flex: 1` and `min-height: 0` so it takes leftover space in a column instead of growing with its children. `height` is a fixed pixel viewport. `width` constrains the viewport; omitted, it fills the parent. `size` is a square viewport, matching other nodes (it wins over `width` / `height`). Visual styles (`padding`, `bg`, `border`, …) apply to the inner scroll body, not twice. `flex: 1` on other nodes also sets `min-height: 0`.

`list`, `table`, and `tree` use an outer clj-gpui wrapper for layout geometry and visual keys; the inner crate widget keeps `size_full()` for virtualization. `:size` is a square (it wins over `:width` / `:height`). Omitted `:width` fills the parent. Explicit `:height` is a pixel viewport. `:flex 1` fills leftover column height with `min-height: 0`. If height, size, and flex are all omitted, the host uses a default viewport (~200px list/tree, ~220px table) so crate `size_full()` does not collapse or steal the column.

gpui-component 0.5.1 `Root::render` does not paint dialog / sheet / notification layers; the host calls `Root::render_dialog_layer`, `Root::render_sheet_layer`, and `Root::render_notification_layer` from `RootView`. Open/close for dialogs and the single crate sheet still goes through `WindowExt` on the next frame so `RootView::render` does not re-enter `Root`. Builders read a live spec cell (latest callback ids, title, body, children, footer) so an unrelated Clojure rerender cannot leave a stale `cb-7` on an already-open overlay. Overlay click dismisses dialogs/sheets by default (`:overlay-closable false` restores the crate lock). After overlay/Escape dismiss the host does not re-open until Clojure’s tree drops `open`. Notifications are a stack: presence shows unless `open` is false; unchanged title/message/variant/autohide is not re-pushed. Tree removal dismisses without a second `:on-close`. Static overlay children (dialog/sheet/dock panels) use a full path element id. `popover` is in-tree; its trigger must be a button (`Selectable`). Menu item clicks send the original Clojure id; item `:on-click` then menu `:on-change` is one batch. List `:on-change` is selection and `:on-confirm` is activation; both restore the original Clojure id and, on click/Enter, run as one batch before the next tree. Table single click is `:on-change`; a double-click is crate `SelectRow` then `DoubleClickedRow` from one `on_row_left_click`, batched as `:on-change` then `:on-confirm`.

`otp-input` `:on-change` fires only when every cell is filled. `editor` is `InputState::code_editor` (highlighter language, no LSP). Dock panel bodies are the static overlay subset plus `markdown`/`chart`, not list/table/editor.

`spinner`, `badge`, and `clipboard` are not gpui-component `Styled` types. The host wraps them in a `div` that receives the usual layout and visual keys (`width`, `height`, `size`, `flex`, `padding`, `bg`, …). `accordion` and `description-list` use the same outer-owns-layout pattern, but the wrapper defaults to `flex-none` and full width so crate `size_full()` cannot steal leftover column height. Inner chrome is not styled twice.

Keywords in the tree become JSON strings (`:semibold` → `"semibold"`).

Put `:theme` on **any** node. The host does not choose a theme on its own:

* `:system` (default if omitted) follows the OS appearance, including later changes, using gpui-component Default Light / Default Dark
* `:light` pins Default Light for that subtree
* `:dark` pins Default Dark for that subtree
* a **named palette** such as `"Tokyo Night"` or `:ayu-light` calls gpui-component `Theme::apply_config` with that [theme](https://longbridge.github.io/gpui-component/docs/theme)
* a **custom ThemeSet** registered from Clojure (or loaded from JSON) is also a name: the variant (`"Catppuccin Violet Dark"`) pins that config; the family (`"Catppuccin Violet"`) picks the light or dark member from OS appearance

The host matches names case-insensitively and treats `-` / `_` as spaces, so `:tokyo-night`, `"tokyo night"`, and `"Tokyo Night"` are the same palette. Clojure `gpui.theme` uses that same identity for `register!` / `unregister!` / `json-str`.

Lookup order (first match wins): Clojure `:themes` on the render response, then `CLJ_GPUI_THEMES`, then `./themes`, then bundled JSON, then ThemeRegistry (`Default Light` / `Default Dark`). JSON directories are cached by file mtime; a change on disk is picked up on the next lookup. Duplicate variant names are deterministic: first ThemeSet in the Clojure array, then JSON files in sorted path order.

Drop extra gpui-component theme-set JSON files in a `themes/` directory next to the process working directory, or in `CLJ_GPUI_THEMES`. Those override bundled names. Clojure-registered sets override JSON.

A nested `:theme` wraps that subtree during layout and paint so siblings keep their own theme. The footer / waiting state follow the **root** node's `:theme` (usually the `window`).

gpui-component's `Theme` is process-global. Nested scopes work because layout, prepaint, and paint of a subtree run synchronously and restore the previous theme before the sibling is drawn. A second window would share that global; clj-gpui is still one window. There is no headless GPUI fixture here that can paint two themed buttons without a real window, so sibling isolation is enforced in the host's `ThemeScope` and covered on the Clojure side by serialization tests.

Window chrome is Clojure-owned on a `window` node (the host still reads these keys from whatever node is the tree root):

* `:title` sets the native window title (default `clj-gpui`)
* `:chrome :dev` (default) shows the nREPL footer; `:chrome :app` hides host chrome
* `:window-width` / `:window-height` resize the window when those values change in the tree. On `ui/window`, Clojure maps `:width` / `:height` to these keys so they are not layout. If the root is not a `window`, root `:width` / `:height` are still used when the `window-*` keys are omitted.

The size is applied when the tree’s requested size changes, not on every user drag.
