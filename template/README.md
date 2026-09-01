# ClojureGPUI app template

Copy this directory to start a native GPUI app driven by JVM Clojure.

## Run

From this directory (next to the library checkout):

```bash
clj -M:dev
```

Requires a Rust toolchain. The first run builds the native host (`cargo build --release` in the library's `host/`).

Edit `src/my/app.clj` and save. The window rerenders; `defonce` / `r/atom` state is kept. nREPL prints on startup (default port 7888, also written to `.nrepl-port`).

## Git dependency

When you are not sitting next to this checkout, replace the local root in `deps.edn`:

```clojure
{:deps {clj-gpui/clj-gpui {:git/url "https://github.com/YOUR/clj-gpui.git"
                           :git/sha "REPLACE_WITH_SHA"}}}
```

Use the git URL of the library you actually cloned. There is no Clojars release yet.

You can also point at a built host binary with `CLOJUREGPUI_BIN`, or at a library checkout with `CLOJUREGPUI_ROOT`.
