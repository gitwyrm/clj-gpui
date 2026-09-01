# gpui-component themes

JSON palettes from [gpui-component 0.5.1](https://github.com/longbridge/gpui-component/tree/v0.5.1/themes),
embedded in the host and selected from Clojure with `:theme "Tokyo Night"` (or `:tokyo-night`).

These files are part of gpui-component and are Apache-2.0. See
https://longbridge.github.io/gpui-component/docs/theme

Put your own `*.json` theme sets in a `themes/` directory next to the app,
or in the directory named by `CLJ_GPUI_THEMES`. Same schema as these files:
a top-level `"themes"` array of objects with `"name"`, `"mode"`, and `"colors"`.
