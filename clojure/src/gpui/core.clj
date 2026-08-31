(ns gpui.core
  "Clojure-facing GPUI primitives.

  These functions return ordinary Clojure maps. The Rust GPUI host
  translates that data into native elements. Application logic stays
  in Clojure: atoms, functions, sequences, macros, and namespaces
  are the real Clojure runtime, not a reimplementation.")

(defonce ^:private request-render-impl (clojure.core/atom nil))

(defn set-request-render!
  "Used by the GPUI runtime to install the host notification hook.
  Application code should call `request-render!` instead."
  [f]
  (reset! request-render-impl f)
  f)

(defn request-render!
  "Ask the native GPUI window to rerender from the current Clojure UI tree.

  Safe to call from a REPL after redefining functions. Changes to
  `gpui.ratom/atom` (and atoms passed through `watch!`) already call
  this automatically."
  []
  (when-let [f @request-render-impl]
    (f)))

(defn watch!
  "Attach GPUI rerendering to an existing Clojure atom.

  Prefer `gpui.ratom/atom` (typically required as `r/atom`) for new
  state. Use this when you already have a `clojure.core/atom`."
  [a]
  (add-watch a ::gpui-render
             (fn [_ _ _ _]
               (request-render!)))
  a)

(defn ratom
  "Like `clojure.core/atom`, but GPUI rerenders when the value changes.

  Prefer requiring `[gpui.ratom :as r]` and writing `(r/atom 0)`, the
  same shape as Reagent. `swap!`, `reset!`, and `@` are unchanged
  because the value is a real Clojure atom."
  ([x]
   (watch! (clojure.core/atom x)))
  ([x & options]
   (watch! (apply clojure.core/atom x options))))

(defn ui-node?
  "True when `x` is a GPUI element map produced by this namespace."
  [x]
  (and (map? x) (contains? x :type)))

(defn- split-style-children
  [args]
  (if (and (seq args)
           (map? (first args))
           (not (ui-node? (first args))))
    [(first args) (rest args)]
    [{} args]))

(defn flatten-children
  "Normalize children so ordinary Clojure sequences work inside layouts.

  This is what makes `(map item-view items)`, `for`, `when`, and nested
  vectors compose without a custom list language."
  [xs]
  (into []
        (mapcat
         (fn [x]
           (cond
             (nil? x) []
             (false? x) []
             (ui-node? x) [x]
             (string? x) [{:type :label :text x}]
             (sequential? x) (flatten-children x)
             :else [{:type :label :text (str x)}]))
         xs)))

(defn label
  "A text label. Optional style map uses GPUI-oriented keys, not CSS.

  (ui/label \"Hello\")
  (ui/label \"Hello\" {:font-size 20 :font-weight :bold})"
  ([text]
   {:type :label :text (str text)})
  ([text style]
   (merge {:type :label :text (str text)} style)))

(defn button
  "A clickable button. `on-click` is a real Clojure function (often `#()`).

  (ui/button \"+\" #(swap! count inc))
  (ui/button \"Save\" save! {:font-weight :bold})"
  ([text]
   {:type :button :text (str text)})
  ([text on-click]
   (if (map? on-click)
     (merge {:type :button :text (str text)} on-click)
     {:type :button :text (str text) :on-click on-click}))
  ([text on-click style]
   (merge {:type :button :text (str text) :on-click on-click} style)))

(defn vstack
  "Vertical stack. An optional leading map is treated as layout/style.

  (ui/vstack
    {:gap 8 :padding 16}
    (ui/label \"Hello\")
    (map item-view items))"
  [& args]
  (let [[style children] (split-style-children args)]
    (assoc style :type :vstack :children (flatten-children children))))

(defn hstack
  "Horizontal stack. Same optional style map convention as `vstack`."
  [& args]
  (let [[style children] (split-style-children args)]
    (assoc style :type :hstack :children (flatten-children children))))

(defn spacer
  "Flexible space, or a gap of `size` pixels when given a number."
  ([]
   {:type :spacer :flex 1})
  ([size]
   (if (map? size)
     (merge {:type :spacer :flex 1} size)
     {:type :spacer :size size})))

(defn checkbox
  "A checkbox. `on-click` is a 0-arg Clojure function; toggle the atom yourself.

  (ui/checkbox (:done item) #(swap! state update-in [:items i :done] not) \"Done\")"
  ([checked on-click]
   {:type :checkbox :checked (boolean checked) :on-click on-click})
  ([checked on-click label-or-style]
   (if (map? label-or-style)
     (merge {:type :checkbox :checked (boolean checked) :on-click on-click}
            label-or-style)
     {:type :checkbox
      :checked (boolean checked)
      :text (some-> label-or-style str)
      :on-click on-click}))
  ([checked on-click label style]
   (merge {:type :checkbox
           :checked (boolean checked)
           :text (some-> label str)
           :on-click on-click}
          style)))

(defn scroll
  "Scroll container. Give it a `:height` (pixels) if it should not flex."
  [& args]
  (let [[style children] (split-style-children args)]
    (assoc style :type :scroll :children (flatten-children children))))
