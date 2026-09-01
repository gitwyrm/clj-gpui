# Clojure ↔ GPUI protocol

Newline-delimited JSON over a local TCP connection. Clojure listens;
the native host connects as a client.

Environment for the host process:

| Variable | Meaning |
|---|---|
| `CLJ_GPUI_PORT` | TCP port of the Clojure listener (required) |
| `CLJ_GPUI_HOST` | TCP host, default `127.0.0.1` |

Protocol version is **1**. Clojure sends it on `:ready`. The host refuses a mismatch.

## Handshake

1. Clojure binds `127.0.0.1:0`, then spawns the host with `CLJ_GPUI_PORT` set.
2. Host connects and waits for a `ready` line.
3. Host sends `render` (and later `callback` / `reload`) as JSON objects with a numeric `id`.
4. Clojure replies with `{"op":"response","id":…, …}`.

### `ready` (Clojure → host)

```json
{"op":"ready","protocol-version":1,"nrepl":7888,"app":"counter.app/app"}
```

### `request-render` (Clojure → host)

Sent when an `r/atom` changes, a file watcher reloads, or `gpui.ui/request-render!` is called. The host follows up with `render`.

```json
{"op":"request-render"}
```

## Host → Clojure ops

Each request includes a unique numeric `id`. Clojure echoes it on the response.

### `render`

```json
{"op":"render","id":1}
```

Response:

```json
{"op":"response","id":1,"ok":true,"tree":{…}}
```

On an application exception Clojure still returns `ok: true` with an error UI tree so the window can paint.

### `callback`

```json
{"op":"callback","id":2,"callback-id":"cb-2"}
```

Invokes the real Clojure IFn that was registered when the current tree was exported, then the host typically issues another `render`.

### `reload`

```json
{"op":"reload","id":3}
```

`(require ns :reload)` of `gpui.ui`, `gpui.core`, `gpui.ratom`, and the app namespace. `defonce` / `r/atom` bindings are kept. Response includes a fresh `tree`.

## Node schema (version 1)

Every node is a JSON object. Unknown fields are ignored by the host.

| Field | Type | Used by |
|---|---|---|
| `type` | string | all (`label`, `button`, `vstack`, `hstack`, `spacer`, `checkbox`, `scroll`) |
| `text` | string | `label`, `button`, `checkbox` |
| `children` | array of nodes | layouts, `scroll` |
| `on-click` | string callback id | `button`, `checkbox` |
| `on-change` | string callback id | reserved |
| `checked` | bool | `checkbox` |
| `gap`, `padding`, `width`, `height`, `size`, `flex` | number | layout / spacer |
| `font-size` | number | text |
| `font-weight` | string (`bold`, `semibold`, `medium`, `light`, …) | text |
| `color` | hex string (`#c0caf5`) | text |

Functions never go on the wire. `gpui.runtime` replaces `fn?` values under `:on-click` / `:on-change` with ids such as `"cb-2"`. The registry is rebuilt on every export.

Keywords in the tree become JSON strings (`:semibold` → `"semibold"`).

The native window title is currently the constant `gpui.ui/window-title` (`clj-gpui`).
