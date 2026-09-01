# Clojure ↔ GPUI protocol

Newline-delimited JSON over a local TCP connection. Clojure listens;
the native host connects as a client.

Environment for the host process:

| Variable | Meaning |
|---|---|
| `CLJ_GPUI_PORT` | TCP port of the Clojure listener (required) |
| `CLJ_GPUI_HOST` | TCP host, default `127.0.0.1` |

Protocol version is **3**. Clojure sends it on `:ready`. The host refuses a mismatch.

## Handshake

1. Clojure binds `127.0.0.1:0`, then spawns the host with `CLJ_GPUI_PORT` set.
2. Host connects and waits for a `ready` line.
3. Host sends `render` (and later `callback` / `reload`) as JSON objects with a numeric `id`.
4. Clojure replies with `{"op":"response","id":…, …}`.

### `ready` (Clojure → host)

```json
{"op":"ready","protocol-version":3,"nrepl":7888,"app":"counter.app/app"}
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

Optional `value` is included for text-field events:

```json
{"op":"callback","id":2,"callback-id":"cb-2","value":"hello"}
```

Buttons and checkboxes omit `value`; Clojure calls the handler with no arguments. When `value` is present (including `""`), Clojure calls `(f value)`.

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

## Node schema (version 3)

Every node is a JSON object. Unknown fields are ignored by the host.

| Field | Type | Used by |
|---|---|---|
| `type` | string | all (`window`, `label`, `button`, `vstack`, `hstack`, `spacer`, `checkbox`, `scroll`, `text-field`) |
| `id` | string | optional stable identity, especially `text-field` |
| `text` | string | `label`, `button`, `checkbox`, `text-field` (current value) |
| `placeholder` | string | `text-field` |
| `children` | array of nodes | layouts, `scroll` |
| `on-click` | string callback id | `button`, `checkbox`, `label`, `vstack`, `hstack` |
| `on-double-click` | string callback id | `label` (0-arg; wins over `on-click` when `click_count >= 2`) |
| `on-change` | string callback id | `text-field` (called with the field string) |
| `on-submit` | string callback id | `text-field` (Enter; called with the field string) |
| `on-blur` | string callback id | `text-field` (called with the field string) |
| `on-escape` | string callback id | `text-field` (0-arg) |
| `focus` | bool | `text-field`: request keyboard focus |
| `checked` | bool | `checkbox` |
| `shape` | string | `checkbox`: `circle` for a round toggle |
| `primary` | bool | `button` (alias for `variant: primary`) |
| `variant` | string | `button` (`primary`, `ghost`, `text`, `outline`, `danger`) |
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
| `title` | string | `window` (or any root): native window title (default `clj-gpui`) |
| `chrome` | string | `window` (or any root): `dev` (default, nREPL footer) or `app` (no host chrome) |
| `window-width`, `window-height` | number | `window` (or any root): native window size in pixels |

Functions never go on the wire. `gpui.runtime` replaces `fn?` values under `:on-click` / `:on-change` / `:on-submit` / `:on-double-click` / `:on-blur` / `:on-escape` with ids such as `"cb-2"`. The registry is rebuilt on every export.

The native host paints these nodes with [gpui-component](https://crates.io/crates/gpui-component) 0.5.1 (`Button`, `Checkbox`, `Input`, `v_flex` / `h_flex`, themed `Root`).

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
