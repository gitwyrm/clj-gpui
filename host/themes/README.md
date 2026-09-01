# gpui-component themes

JSON palettes from [gpui-component 0.5.1](https://github.com/longbridge/gpui-component/tree/v0.5.1/themes),
embedded in the host and selected from Clojure with `:theme "Tokyo Night"` (or `:tokyo-night`).

These files are part of gpui-component and are Apache-2.0. See
https://longbridge.github.io/gpui-component/docs/theme

Put your own `*.json` theme sets in a `themes/` directory next to the app,
or in the directory named by `CLJ_GPUI_THEMES`. Those files are the full
gpui-component ThemeSet schema (including `highlight`). JSON files are read
once per directory fingerprint (path + mtime); an edit on disk is picked up
on the next lookup.

Clojure `gpui.theme/register!` validates names, modes, and color hex values
and passes other ThemeConfig fields through. Registered sets override these
JSON files. See `examples/themes/catppuccin-violet`.
