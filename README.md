# ClojureGPUI

A proof of concept for writing **native GPUI applications in real Clojure**.

This is not a Clojure-like language, a Lisp-inspired DSL, or a toy interpreter. Application code is ordinary JVM Clojure: `def`, `defn`, `defonce`, atoms, `#()`, `map`, macros, namespaces, and the rest of the language are provided by Clojure itself. Rust owns the GPUI window and translates Clojure data into native elements.

The long-term inspiration is [ClojureDart](https://github.com/Tensegrity-Pro/ClojureDart), but targeting Rust/GPUI instead of Dart/Flutter. This repository deliberately does **not** start that compiler. It finds the smallest architecture that lets genuine Clojure control a native GPUI window, including callbacks, atom-driven rerenders, hot reload, and nREPL.

## Demo

`demo.app` is ordinary Clojure:

```clojure
(ns demo.app
  (:require [demo.helpers :as helpers]
            [gpui.core :as ui]))

(defonce state
  (ui/watch!
   (atom {:count 0
          :items [{:title "Write UI in real Clojure" :done true}]})))

(defn increment! []
  (swap! state update :count inc))

(defn item-view [idx {:keys [title done]}]
  (ui/hstack
    (ui/checkbox done #(swap! state update-in [:items idx :done] not))
    (ui/label (helpers/bullet title))))

(defn app []
  (let [{:keys [count items]} @state]
    (ui/vstack
      {:gap 12}
      (ui/label "ClojureGPUI" {:font-size 22 :font-weight :bold})
      (ui/label (str "Count: " count))
      (ui/hstack
        (ui/button "−" #(swap! state update :count dec))
        (ui/button "+" increment!))
      (when (> count 5)
        (ui/label "That's a lot!"))
      (map-indexed item-view items))))
```

That data is rendered as a native GPUI window: no browser, no webview, no Electron, no HTML, no CSS, no React.

## Quick start

Requirements:

* A recent stable Rust toolchain
* Java 21+ and the [Clojure CLI](https://clojure.org/guides/install_clojure)
* Linux or macOS (GPUI's current platforms)
* A working display. On Linux, GPUI needs Vulkan. Software rendering via Mesa lavapipe is enough for a first window.

```bash
# Clojure unit tests
cd clojure && clojure -M:test

# End-to-end bridge test without opening a window
cd rust && cargo run --release -- --protocol-test

# Native window (from the repo root)
./scripts/run.sh
```

On first `cargo` build, GPUI and its GPU stack take a while to compile.

The window footer shows the nREPL port (7888 by default). Connect with CIDER, Calva, or:

```bash
cd clojure && clojure -M:connect
```

Then, while the native window is running:

```clojure
(in-ns 'demo.app)
(swap! state assoc :count 100)
(swap! state update :items conj {:title "From the REPL" :done false})
(defn app [] (gpui.core/label "Redefined from nREPL"))
(gpui.core/request-render!)
```

Atom watches already request a rerender. Redefining `app` without changing an atom needs `(gpui.core/request-render!)`.

### Hot reload

Edit `clojure/src/demo/app.clj` and save. The runtime reloads the namespace with `(require ns :reload)` and asks GPUI to paint again. `defonce` state survives because the namespace is not unloaded.

## Architecture

```text
┌──────────────────────────────────────────────┐
│              Rust process                    │
│  GPUI window / event loop / GPU renderer     │
│                                              │
│   renderer.rs  ← UI tree as JSON maps        │
│   bridge.rs    ← newline-delimited JSON TCP  │
└──────────────────────▲───────────────────────┘
                       │ 127.0.0.1
                       │ {:op "render"}
                       │ {:op "callback" :callback-id "cb-2"}
                       │ {:op "request-render"}
┌──────────────────────┴───────────────────────┐
│         JVM Clojure child process            │
│                                              │
│   gpui.runtime   socket + callback registry  │
│   gpui.core      UI constructors (data)      │
│   demo.app       real Clojure application    │
│   nREPL          127.0.0.1:7888              │
│   file watcher   (require :reload)           │
└──────────────────────────────────────────────┘
```

### Why this architecture

Approaches considered before writing code:

| Approach | Verdict for this proof of concept |
|---|---|
| **JVM Clojure + local IPC** (chosen) | Real Clojure, real nREPL, real `tools.namespace`-style reload, UI-as-data boundary, GPUI keeps the OS event loop. |
| JNI embedding of the JVM in the GPUI process | Attractive later for a single process. Painful now: GPUI must own the main thread, JNI thread attachment, and classpath packaging. The logical protocol would look the same. |
| Rust launching Clojure and talking over stdin | Weaker than a socket once nREPL and a file watcher also need to notify the host. |
| GraalVM Native Image | Useful for distribution later. Native Image cannot `eval` or reload the way JVM Clojure can, so it would fight the REPL/hot-reload goals. |
| jank (native Clojure on LLVM) | Real Clojure dialect, but C++/LLVM rather than GPUI, and not a drop-in host today. |
| ClojureDart-style compiler to Rust | The right long-term analogue. Far larger than a first vertical slice. |
| A Lisp interpreter in Rust | Explicitly rejected. That would not be Clojure. |

IPC won because the preferred Clojure/GPUI boundary is already **ordinary persistent data**. Functions cannot go on the wire, so they become callback ids. That is the same design you would want even with JNI.

GPUI stays a renderer and event pump. It is not a second application-language runtime.

### How Clojure is hosted

The Rust binary binds `127.0.0.1:0`, then spawns:

```text
clojure -M -m gpui.runtime
```

with `CLOJUREGPUI_PORT` set. The Clojure process is a normal tools.deps app. It requires `demo.app`, starts nREPL, starts a source watcher, connects back to the host, and sends `{:op "ready"}`.

Set `CLOJUREGPUI_CLOJURE_DIR` if the host cannot find `clojure/deps.edn`. Set `CLOJUREGPUI_APP` (default `demo.app/app`) to point at a different root var.

### How values cross the boundary

Clojure UI constructors return maps:

```clojure
{:type :vstack
 :gap 12
 :children [{:type :label :text "Hello"}
            {:type :button :text "+" :on-click #()}]}
```

Before JSON serialization, `gpui.runtime` walks the tree and replaces `fn?` values under `:on-click` / `:on-change` with ids such as `"cb-2"`. Keywords become strings. Nested sequences from `map` / `for` are flattened by `gpui.core/flatten-children`, which is why this works:

```clojure
(ui/vstack
  (ui/label "Todos")
  (map item-view items)
  (when (> count 5)
    (ui/label "That's a lot!")))
```

`when` returning `nil` is dropped. Lazy seqs are realized. Nothing on the Rust side reimplements `map`, `when`, or sequences.

### How callbacks cross the boundary

1. Clojure builds a tree containing a real IFn.
2. The runtime stores that IFn in an atom keyed by `cb-N`.
3. GPUI receives `{"type":"button","text":"+","on-click":"cb-2"}`.
4. A native click sends `{"op":"callback","callback-id":"cb-2"}`.
5. Clojure looks up the IFn and invokes it. Typically that `swap!`s an atom.

The callback is not a Rust expression language. It is the same function object you passed to `ui/button`.

The registry is rebuilt on every render so ids do not leak across trees.

### How GPUI asks Clojure for the UI tree

The host sends `{"op":"render","id":1}`. Clojure calls the root var (`demo.app/app`), sanitizes the result, and replies with `{"op":"response","id":1,"ok":true,"tree":{...}}`. GPUI 0.2's `Render` implementation walks that tree into `div` elements.

### Rerendering

`gpui.core/watch!` installs an `add-watch` that sends `{"op":"request-render"}`. The host fetches a fresh tree and calls `cx.notify()` on the root view. The whole window is redrawn. There is no fine-grained reactive graph yet, and there does not need to be.

From a REPL you can also call `(gpui.core/request-render!)`.

### Hot reload

A polling watcher on `clojure/src/**/*.clj` (except `runtime.clj`) does:

```clojure
(require 'gpui.core :reload)
(require 'demo.app :reload)
```

That is the normal Clojure REPL meaning of reload: `defn`s update, `defonce` bindings stay. `clojure.tools.namespace` `refresh` is *not* used as the default because unloading namespaces would reset `defonce`. It remains a good option for a deeper reset later.

Clojure source changes do not rebuild the Rust host.

### REPL

nREPL is the real nREPL server (`nrepl/nrepl`). Editor tooling works. Evaluating `(reset! state {:count 100 ...})` fires the atom watch and the native UI updates.

## Repository layout

```text
clojure/
  deps.edn
  src/gpui/core.clj        ; UI constructors + watch!/request-render!
  src/gpui/runtime.clj     ; socket host, callbacks, nREPL, watcher
  src/demo/app.clj         ; demo application
  src/demo/helpers.clj     ; extra namespace, to prove require works
  test/gpui/core_test.clj
rust/
  Cargo.toml
  src/main.rs
  src/bridge.rs            ; process + JSON protocol
  src/renderer.rs          ; Clojure maps → GPUI elements
  src/protocol.rs
scripts/run.sh
scripts/protocol-test.sh
```

## Clojure UI API

Constructors live in `gpui.core`. They take optional style maps with GPUI-oriented keys, not CSS:

```clojure
(ui/label "Hello" {:font-size 20 :font-weight :bold :color "#c0caf5"})
(ui/button "+" on-click)
(ui/vstack {:gap 8 :padding 16} ...)
(ui/hstack ...)
(ui/spacer)
(ui/checkbox checked on-click "Label")
(ui/scroll {:height 220} ...)
```

Initial widgets: label, button, vertical stack, horizontal stack, spacer, checkbox, scroll. Styling is deliberately small (`gap`, `padding`, `font-size`, `font-weight`, `color`, `width`, `height`).

## Protocol test

`cargo run --release -- --protocol-test` starts Clojure, fetches a tree, clicks the `+` callback, asserts `Count: 1`, reloads, and asserts `defonce` kept the count. It does not need a GPU.

## Known limitations

* **Two processes, JSON copies.** Fine for this slice. A future JNI or shared-memory path can keep the same Clojure API.
* **Whole-window rerender.** No incremental DOM-style diffing.
* **Callback ids are per-tree.** In-flight clicks after a reload can miss if the id was rebuilt. Acceptable for a prototype.
* **No text input yet.** Plain GPUI input is more involved than buttons; checkbox and scroll were the cheap extras.
* **Linux Vulkan.** GPUI is GPU-accelerated. Headless CI should use `--protocol-test`. For a window without a discrete GPU, Mesa lavapipe works (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).
* **First Clojure start** may download Maven artifacts.
* **Packaging** is not solved: you still ship a JRE plus the GPUI binary.
* **gpui-component** was investigated and skipped. Plain GPUI made the first vertical slice smaller.

## Long-term investigation

### A. JVM Clojure + GPUI bridge (this prototype)

Advantages: real Clojure, real libraries, real nREPL, hot reload, small implementation, GPUI stays native.

Disadvantages: JVM startup and memory, two runtimes, distribution means bundling a JRE (or requiring one), extra hop for every render/callback. Desktop memory will look like "Zed-class GPU app + a Clojure REPL", not like a 5 MB static binary.

This is the right development architecture even if a later compiler is used for production, the same way ClojureDart keeps a REPL story on the Dart VM.

### B. Clojure AOT / GraalVM Native Image

[clj-easy/graal-build-time](https://github.com/clj-easy/graal-build-time) makes `native-image` practical for closed Clojure programs. That could shrink startup and hide the JRE for a shipped app.

It is a poor fit for the *interactive* product this proof of concept is testing. Native Image does not provide Clojure's runtime compiler. People usually embed [SCI](https://github.com/babashka/sci) for a restricted eval. That is no longer "the real Clojure compiler".

A plausible future split: JVM Clojure during development (this repo), GraalVM or similar for a frozen release build that still talks to GPUI over the same protocol — if eval is not required at runtime.

### C. A Clojure-to-Rust compiler (ClojureDart analogue)

ClojureDart compiles Clojure forms to Dart and uses the Dart runtime. The equivalent here would compile Clojure to Rust (or to a Rust-hosted IR) and use Rust data structures / GPUI directly.

That is a compiler project, not a bridge:

* analysis of Clojure forms (see `tools.analyzer`)
* an emitter to Rust, LLVM, or a bytecode the GPUI process can JIT
* a runtime: persistent maps/vectors, keywords, protocols, multimethods, vars, atoms, namespaces
* host interop (calling GPUI without a JSON tree)
* a REPL story (jank's LLVM JIT is the closest existing native one)

Do not start this until the interactive GPUI loop is proven, which is the point of this repository.

### D. Existing compiler infrastructure

Useful starting points if (C) is ever attempted:

* **Clojure JVM compiler** — `clojure.lang.Compiler`, host-specific.
* **[tools.analyzer](https://github.com/clojure/tools.analyzer)** plus `tools.analyzer.jvm` — AST that other emitters already consume.
* **ClojureScript compiler** — example of a non-JVM backend, but it assumes a JS runtime.
* **ClojureDart (`cljd.compiler`)** — the closest "Clojure as a language, different host" playbook.
* **[jank](https://jank-lang.org/)** — native Clojure dialect on LLVM with a REPL. C++ interop, not Rust/GPUI. Watching it is more useful than forking it into this tree right now.
* **SCI / Babashka** — interpreters. Convenient, not the real compiler, not the goal.
* **clojure-rs and similar** — incomplete reimplementations. They are the thing this project refuses to become.

The recommended next step after this proof of concept is not a compiler. It is: more widgets, a stable EDN/Transit schema, JNI as an optional transport, and a developer workflow that feels like ClojureScript's figwheel/shadow-cljs but for a GPUI window.

## License

MIT, unless a later commit says otherwise. GPUI is Apache-2.0.
