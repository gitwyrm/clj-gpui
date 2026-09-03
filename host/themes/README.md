# GPUI Kit themes

JSON palettes from [GPUI Kit 0.6](https://github.com/longbridge/gpui-kit/tree/v0.6.0/themes),
embedded in the host and selected from Clojure with `:theme "Tokyo Night"` (or `:tokyo-night`).

These files are part of GPUI Kit and are Apache-2.0. See https://gpui-kit.com

Matrix was dropped with Kit 0.6. Aurora and Asciinema were added. Adventure Time
remains a variant inside `adventure.json`.

Put your own `*.json` theme sets in a `themes/` directory next to the app,
or in the directory named by `CLJ_GPUI_THEMES`. Those files are the full
Kit ThemeSet schema (including `highlight`). JSON files are read
once per directory fingerprint (path + mtime); an edit on disk is picked up
on the next lookup.

Clojure `gpui.theme/register!` validates names, modes, and color hex values
and passes other ThemeConfig fields through. Registered sets override these
JSON files. See `examples/themes/catppuccin-violet`.
