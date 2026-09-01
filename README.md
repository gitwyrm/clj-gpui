# ClojureGPUI

A library for writing **native GPUI applications in real Clojure**.

This is not a Clojure-like language, a Lisp-inspired DSL, or a toy interpreter. Application code is ordinary JVM Clojure: `def`, `defn`, `defonce`, atoms, `#()`, `map`, macros, namespaces. Rust owns the GPUI window and translates Clojure data into native elements.

There is no Clojars release and no CI yet. Depend on this repo with `:local/root` or a git SHA.

## Quick start

Requirements:

* A recent stable Rust toolchain (`cargo` on `PATH`)
* Java 21+ and the [Clojure CLI](https://clojure.org/guides/install_clojure)
* Linux or macOS (GPUI's current platforms)
* A working display. On Linux, GPUI needs Vulkan. Software rendering via Mesa lavapipe is enough for a first window.

From a checkout of this repository:

```bash
# Clojure unit tests
clojure -M:test

# End-to-end bridge test without opening a window
clojure -M:protocol-test

# Example native window
cd examples/counter && clj -M:dev
```

Or from the repo root:

```bash
./scripts/run.sh
```

On first run, `gpui.dev` builds `host/` with `cargo build --release` if the binary is missing. GPUI and its GPU stack take a while to compile once.

![ClojureGPUI native window](docs/screenshot.png)

The window footer shows the nREPL port (7888 by default). Connect with CIDER, Calva, or:

```bash
clojure -M:connect
```

Then, while the native window is running:

```clojure
(in-ns 'counter.app)
(swap! !state assoc :count 100)
(swap! !state update :items conj {:id 99 :title "From the REPL" :done false})
(defn app [] (gpui.ui/label "Redefined from nREPL"))
(gpui.ui/request-render!)
```

Atom watches already request a rerender. Redefining `app` without changing an atom needs `(gpui.ui/request-render!)`.

### Hot reload

Edit `examples/counter/src/counter/app.clj` and save. The runtime reloads the namespace with `(require ns :reload)` and asks GPUI to paint again. `defonce` / `r/atom` state survives because the namespace is not unloaded.

## Use it in your project

Until this is published, add a git or local dependency:

```clojure
;; next to a checkout
{:deps {clj-gpui/clj-gpui {:local/root "../clj-gpui"}}
 :aliases {:dev {:main-opts ["-m" "gpui.dev" "my.app/app"]}}}

;; git SHA (no Clojars yet)
{:deps {clj-gpui/clj-gpui {:git/url "https://github.com/YOUR/clj-gpui.git"
                           :git/sha "REPLACE_WITH_SHA"}}}
```

Copy `template/` as a starting app. Then:

```bash
clj -M:dev
```

`gpui.dev` binds a local TCP port, starts nREPL, watches `src/`, and spawns the native host. The host **connects** to Clojure; it does not launch the JVM.

If Cargo is not on `PATH`, build `host/` yourself and set `CLOJUREGPUI_BIN` to that executable. `CLOJUREGPUI_ROOT` points at a library checkout that contains `host/`.

## Application code

Prefer `[gpui.ui :as ui]` and `[gpui.ratom :as r]`. `gpui.core` re-exports `gpui.ui` for older snippets.

```clojure
(ns my.app
  (:require [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state
  (r/atom {:count 0
           :items [{:title "Write UI in real Clojure" :done true}]}))

(defn item-view [idx {:keys [title done]}]
  (ui/checkbox
   done
   #(swap! !state update-in [:items idx :done] not)
   title))

(defn app []
  (let [{:keys [count items]} @!state]
    (ui/vstack
     {:gap 12 :padding 8}
     (ui/label "ClojureGPUI" {:font-size 22 :font-weight :bold})
     (ui/label (str "Count: " count))
     (ui/hstack
      {:gap 8}
      (ui/button "−" #(swap! !state update :count dec))
      (ui/button "+" #(swap! !state update :count inc)))
     (when (> count 5)
       (ui/label "That's a lot!"))
     (map-indexed item-view items))))
```

That data is rendered as a native GPUI window: no browser, no webview, no Electron, no HTML, no CSS, no React.

Handlers are 0-argument Clojure functions. The host invokes them by callback id after a click.

## Architecture

```text
┌──────────────────────────────────────────────┐
│              Clojure process                 │
│  gpui.dev listens on 127.0.0.1:<ephemeral>   │
│  gpui.ui / gpui.ratom / your app             │
│  nREPL · file watcher                        │
└──────────────────────▲───────────────────────┘
                       │ newline-delimited JSON
                       │ host connects as client
┌──────────────────────┴───────────────────────┐
│              Rust process                    │
│  GPUI window / event loop / GPU renderer     │
│  renderer.rs  ← UI tree as JSON maps         │
│  bridge.rs    ← TCP client + RPC             │
└──────────────────────────────────────────────┘
```

The UI boundary is ordinary persistent Clojure maps. Functions cannot go on the wire; they become callback ids (`"cb-2"`). See [docs/protocol.md](docs/protocol.md).

### Why this architecture

| Approach | Verdict |
|---|---|
| **JVM Clojure + local IPC** (this library) | Real Clojure, real nREPL, UI-as-data, GPUI keeps the OS event loop. |
| JNI embedding of the JVM in the GPUI process | Attractive later for a single process. The logical protocol would look the same. |
| GraalVM Native Image | Useful for distribution later. Fights REPL / hot reload. |
| jank | Real Clojure dialect, but not a GPUI host today. |
| Clojure-to-Rust compiler | Long-term inspiration (ClojureDart analogue). Not this repository. |
| A Lisp interpreter in Rust | Rejected. That would not be Clojure. |

### Rerendering

`(r/atom ...)` returns a real `clojure.core/Atom`. The only extra behavior is an `add-watch` (`:gpui.ratom/watch`) that sends `request-render`. The host fetches a fresh tree and paints the whole window.

`ui/watch!` attaches the same watch to an existing atom.

### Hot reload

A polling watcher on the **application** `src/**/*.clj` (not library `runtime.clj` / `dev.clj`) does `(require ns :reload)`. `clojure.tools.namespace` `refresh` is not used, because unloading namespaces would reset `defonce`.

If `app` throws, Clojure returns an error UI tree (`ok: true`) so the native window still paints.

## Repository layout

```text
deps.edn                      ; git-dep library entry
src/gpui/ui.clj               ; public widgets
src/gpui/ratom.clj            ; (r/atom ...)
src/gpui/core.clj             ; compatibility re-export of gpui.ui
src/gpui/runtime.clj          ; protocol, callbacks, nREPL, watcher
src/gpui/dev.clj              ; Clojure-first launcher
host/                         ; native GPUI binary (Cargo)
examples/counter/             ; demo app (:local/root "../..")
template/                     ; copyable app skeleton
test/                         ; unit tests + gpui.test-app
docs/protocol.md
```

## Clojure UI API

```clojure
(ui/label "Hello" {:font-size 20 :font-weight :bold :color "#c0caf5"})
(ui/button "+" on-click)
(ui/vstack {:gap 8 :padding 16} ...)
(ui/hstack ...)
(ui/spacer)
(ui/checkbox checked on-click "Label")
(ui/scroll {:height 220} ...)
```

`when` returning `nil`, `map`, and nested vectors are flattened by `ui/flatten-children`.

## Environment

| Variable | Meaning |
|---|---|
| `CLOJUREGPUI_BIN` | Path to a `clojuregpui` executable, skipping Cargo |
| `CLOJUREGPUI_ROOT` | Library checkout containing `host/` |
| `CLOJUREGPUI_PORT` | Set by `gpui.dev` for the host (do not set yourself) |
| `CLOJUREGPUI_HOST` | TCP host for the host process, default `127.0.0.1` |
| `CLOJUREGPUI_APP` | Root var if not passed to `gpui.dev` |
| `CLOJUREGPUI_SRC` | Directory the watcher scans, default `src` |
| `CLOJUREGPUI_NREPL_PORT` | Preferred nREPL port, default `7888` |
| `VK_ICD_FILENAMES` | Linux software Vulkan ICD (lavapipe) |

## Known limitations

* **Two processes, JSON copies.** Fine for this slice. A future JNI path can keep the same Clojure API.
* **Whole-window rerender.** No incremental DOM-style diffing.
* **Callback ids are per-tree.** In-flight clicks after a reload can miss if the id was rebuilt.
* **No text input yet.**
* **Linux Vulkan.** Headless checks should use `clojure -M:protocol-test`. For a window without a discrete GPU, Mesa lavapipe works (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).
* **Packaging** is not solved: you still need a JRE plus the GPUI binary. Git deps; no Clojars or host binary downloads yet.
* **gpui-component** was skipped. Plain GPUI made the first vertical slice smaller.

## License

MIT, unless a later commit says otherwise. GPUI is Apache-2.0.
