# Catppuccin Violet (custom ThemeSet)

This example registers a **gpui-component ThemeSet from Clojure maps**, then
selects a member with `:theme` — a string, same as `"Tokyo Night"`.

The palette is adapted from
[`utility_belt_gpui` `src/theme.rs`](https://github.com/gitwyrm/utility_belt_gpui/blob/main/src/theme.rs)
(MIT OR Apache-2.0). See `NOTICE`. clj-gpui does not depend on that crate.

## Run

```bash
cd examples/themes/catppuccin-violet
clj -M:dev
```

Buttons switch between:

* **System pair** — ThemeSet name `Catppuccin Violet` (host picks Light or Dark from OS appearance)
* **Light** — `Catppuccin Violet Light`
* **Dark** — `Catppuccin Violet Dark`

nREPL can do the same:

```clojure
(in-ns 'catppuccin-violet.app)
(swap! !state assoc :choice "Catppuccin Violet Light")
```

## JSON

`themes/catppuccin-violet.json` is the same ThemeSet in gpui-component's file
format. Put that file in `./themes` or `$CLJ_GPUI_THEMES` if you want the host
to load it without `theme/register!`. Clojure registration still wins on a
duplicate name.
