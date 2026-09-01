# clj-gpui

A library for writing **native GPUI applications in real Clojure**.

This is not a Clojure-like language, a Lisp-inspired DSL, or a toy interpreter. Application code is ordinary JVM Clojure: `def`, `defn`, `defonce`, atoms, `#()`, `map`, macros, namespaces. Rust owns the GPUI window and translates Clojure data into native [gpui-component](https://crates.io/crates/gpui-component) widgets.

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

# Format check / apply (cljfmt, community indentation)
clojure -M:cljfmt check
clojure -M:cljfmt fix

# End-to-end bridge test without opening a window
clojure -M:protocol-test

# Example native window (plain counter)
cd examples/counter && clj -M:dev

# Classic TodoMVC (light card, Enter to add)
cd examples/todomvc && clj -M:dev
```

Or from the repo root:

```bash
./scripts/run.sh
```

On first run, `gpui.dev` builds `host/` with `cargo build --release` if the binary is missing. Later runs rebuild when a host source file (`host/src/**/*.rs`, `Cargo.toml`, `Cargo.lock`) is newer than the binary. GPUI and its GPU stack take a while to compile once. A custom Cargo `--target` (or `[build] target` in `.cargo/config.toml`) is fine: the launcher looks under `target/<triple>/release/` as well as `target/release/`. Set `CLJ_GPUI_BIN` to skip Cargo entirely.

![clj-gpui native window](docs/screenshot.png)

The window footer shows the nREPL port (7888 by default). Connect with CIDER, Calva, or:

```bash
clojure -M:connect
```

Then, while the native window is running:

```clojure
(in-ns 'counter.app)
(swap! !state assoc :count 100)
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

If Cargo is not on `PATH`, build `host/` yourself and set `CLJ_GPUI_BIN` to that executable. `CLJ_GPUI_ROOT` points at a library checkout that contains `host/`.

## Application code

Prefer `[gpui.ui :as ui]` and `[gpui.ratom :as r]`. `gpui.core` re-exports `gpui.ui` for older snippets.

```clojure
(ns my.app
  (:require [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state (r/atom {:count 0 :draft ""}))

(defn app []
  (let [{:keys [count draft]} @!state]
    (ui/window
     {:title "clj-gpui" :chrome :dev :theme :system}
     (ui/vstack
      {:gap 12 :padding 8}
      (ui/label "clj-gpui" {:font-size 22 :font-weight :bold})
      (ui/label (str "Count: " count))
      (ui/hstack
       {:gap 8}
       (ui/button "−" #(swap! !state update :count dec))
       (ui/button "+" #(swap! !state update :count inc) {:primary true}))
      (ui/text-field
       draft
       {:id "note"
        :placeholder "A native text field"
        :on-change #(swap! !state assoc :draft %)})))))
```

That data is rendered as a native GPUI window: no browser, no webview, no Electron, no HTML, no CSS, no React. Buttons and checkboxes use 0-argument handlers. Text fields pass the current string to `:on-change` / `:on-submit`. `:on-double-click` is 0-arg. `:on-blur` gets the field string; `:on-escape` is 0-arg.

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
│  GPUI window / gpui-component widgets        │
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

## Formatting

Clojure is formatted with [cljfmt](https://github.com/weavejester/cljfmt) using [community indentation](https://guide.clojure.style/#one-space-indent) (one space when arguments start on the next line). Config is `.cljfmt.edn`. It covers `src/`, `test/`, `examples/`, and `template/`.

```bash
clojure -M:cljfmt check
clojure -M:cljfmt fix
```

The native host is ordinary Rust: `cargo fmt` in `host/` if you touch it.

## Repository layout

```text
deps.edn                      ; git-dep library entry
.cljfmt.edn                   ; cljfmt paths and community indentation
src/gpui/ui.clj               ; public widgets
src/gpui/ratom.clj            ; (r/atom ...)
src/gpui/core.clj             ; compatibility re-export of gpui.ui
src/gpui/runtime.clj          ; protocol, callbacks, nREPL, watcher
src/gpui/dev.clj              ; Clojure-first launcher
host/                         ; native GPUI + gpui-component host
examples/counter/             ; plain counter
examples/todomvc/             ; classic TodoMVC layout
template/                     ; copyable app skeleton
test/                         ; unit tests + gpui.test-app
docs/protocol.md
```

## Clojure UI API

```clojure
(ui/label "Hello" {:font-size 20 :font-weight :bold :color "#c0caf5"})
(ui/button "+" on-click)
(ui/button "Save" save! {:primary true})
(ui/window {:title "Todos" :chrome :app :width 640 :height 820} ...)
(ui/vstack {:theme :light :gap 8 :padding 16} ...)
(ui/hstack ...)
(ui/spacer)
(ui/checkbox checked on-click "Label")
(ui/checkbox done toggle {:shape :circle})
(ui/label title {:on-double-click start-edit})
(ui/scroll {:height 220} ...)
(ui/text-field value {:placeholder "…" :on-change f :on-submit g :on-blur save :on-escape cancel :focus true})
```

Return `ui/window` from `app`. `:title`, `:chrome`, and `:width` / `:height` only make sense there. `:chrome :dev` (default) shows the nREPL footer; `:chrome :app` hides it.

`:theme` is a style on any node: `:system` (follow the OS, the default), `:light`, or `:dark`. Put it on the window for the whole app (and the footer), or on a nested stack to theme just that subtree:

```clojure
(ui/window
 {:title "Studio" :width 960 :height 640}
 (ui/hstack
  {:flex 1}
  (ui/vstack {:theme :dark :width 220 :padding 12} (ui/label "Nav"))
  (ui/vstack {:theme :light :flex 1 :padding 16} (ui/label "Canvas"))))
```

`when` returning `nil`, `map`, and nested vectors are flattened by `ui/flatten-children`.

## Environment

| Variable | Meaning |
|---|---|
| `CLJ_GPUI_BIN` | Path to a `clj-gpui` executable, skipping Cargo |
| `CLJ_GPUI_ROOT` | Library checkout containing `host/` |
| `CLJ_GPUI_PORT` | Set by `gpui.dev` for the host (do not set yourself) |
| `CLJ_GPUI_HOST` | TCP host for the host process, default `127.0.0.1` |
| `CLJ_GPUI_APP` | Root var if not passed to `gpui.dev` |
| `CLJ_GPUI_SRC` | Directory the watcher scans, default `src` |
| `CLJ_GPUI_NREPL_PORT` | Preferred nREPL port, default `7888` |
| `VK_ICD_FILENAMES` | Linux software Vulkan ICD (lavapipe) |

## Known limitations

* **Two processes, JSON copies.** Fine for this slice. A future JNI path can keep the same Clojure API.
* **Whole-window rerender.** No incremental DOM-style diffing.
* **Callback ids are per-tree.** In-flight clicks after a reload can miss if the id was rebuilt.
* **Linux Vulkan.** Headless checks should use `clojure -M:protocol-test`. For a window without a discrete GPU, Mesa lavapipe works (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).
* **Packaging** is not solved: you still need a JRE plus the GPUI binary. Git deps; no Clojars or host binary downloads yet.

## License

MIT, unless a later commit says otherwise. GPUI is Apache-2.0.
