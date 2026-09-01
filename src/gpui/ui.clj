(ns gpui.ui
  "Public widget constructors for clj-gpui.

  These functions return ordinary Clojure maps. The native host
  translates that data into GPUI elements. Application logic stays
  in Clojure: atoms, functions, sequences, macros, and namespaces
  are the real Clojure runtime."
  (:refer-clojure :exclude []))

(def protocol-version
  "Version of the Clojure↔host UI-tree protocol. Bump when the schema changes."
  1)

(def window-title
  "Default native window title when `ui/window` omits `:title`."
  "clj-gpui")

(def ^:const ratom-watch-key
  "Watch key installed by `watch!` / `ratom`. Stable even if this ns is renamed."
  :gpui.ratom/watch)

(defonce ^:private request-render-impl (clojure.core/atom nil))

(defn set-request-render!
  "Used by the runtime to install the host notification hook.
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
  (add-watch a ratom-watch-key
             (fn [_ _ _ _]
               (request-render!)))
  a)

(defn ratom
  "Like `clojure.core/atom`, but GPUI rerenders when the value changes.

  Prefer requiring `[gpui.ratom :as r]` and writing `(r/atom 0)`."
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
  (ui/label \"Hello\" {:font-size 20 :font-weight :bold})
  (ui/label \"todos\" {:font-family \".SystemUIFont\" :font-weight :light})
  (ui/label title {:on-double-click #(start-edit item)})"
  ([text]
   {:type :label :text (str text)})
  ([text style]
   (merge {:type :label :text (str text)} style)))

(defn button
  "A clickable button. `on-click` is a real Clojure function (often `#()`).

  (ui/button \"+\" #(swap! count inc))
  (ui/button \"Save\" save! {:primary true})"
  ([text]
   {:type :button :text (str text)})
  ([text on-click]
   (if (map? on-click)
     (merge {:type :button :text (str text)} on-click)
     {:type :button :text (str text) :on-click on-click}))
  ([text on-click style]
   (merge {:type :button :text (str text) :on-click on-click} style)))

(defn window
  "Native window. Return this from `app`. Only one makes sense.

  `:title` is the OS window title (default `clj-gpui`).
  `:chrome :dev` (default) shows the nREPL footer; `:chrome :app` hides it.
  `:width` / `:height` are the native window size in pixels
  (`:window-width` / `:window-height` are accepted as aliases).
  Those size keys are not layout: children fill the window.

  `:theme` may live here (default for the window and the footer) or on
  any nested node, so different parts of the app can use different themes.

  (ui/window
    {:title \"Todos\" :chrome :app :width 640 :height 820}
    (ui/vstack {:theme :light :flex 1 :gap 8 :padding 16}
      (ui/label \"Hello\")
      (map item-view items)))"
  [& args]
  (let [[style children] (split-style-children args)
        title (:title style)
        chrome (:chrome style)
        width (or (:window-width style) (:width style))
        height (or (:window-height style) (:height style))
        node (-> style
                 (dissoc :title :chrome :width :height :window-width :window-height)
                 (assoc :type :window :children (flatten-children children)))]
    (cond-> node
      title (assoc :title (str title))
      (some? chrome) (assoc :chrome chrome)
      (some? width) (assoc :window-width width)
      (some? height) (assoc :window-height height))))

(defn vstack
  "Vertical stack. An optional leading map is treated as layout/style.

  `:theme :system`, `:light`, or `:dark` is a style, not window chrome.
  It can sit on this stack, on `ui/window`, or on any other node.
  `:system` (the default when omitted) follows the OS appearance.

  (ui/vstack {:theme :light :gap 8 :padding 16}
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

  `:shape :circle` paints a round toggle instead of gpui-component's square.

  (ui/checkbox (:done item) #(swap! state update-in [:items i :done] not) \"Done\")
  (ui/checkbox done toggle {:shape :circle})"
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

(defn text-field
  "Single-line text input rendered with gpui-component's Input.

  `on-change` and `:on-submit` receive the current string. Prefer a
  stable `:id` so typed text survives layout shifts. `:focus true`
  requests keyboard focus. `:on-blur` gets the string; `:on-escape`
  is 0-arg.

  (ui/text-field draft
                 {:id \"new-todo\"
                  :placeholder \"What needs to be done?\"
                  :on-change #(swap! !state assoc :draft %)
                  :on-submit add-todo})
  (ui/text-field draft
                 {:id \"edit-1\"
                  :focus true
                  :on-submit save
                  :on-blur save
                  :on-escape cancel})"
  ([value]
   {:type :text-field :text (str (or value ""))})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge {:type :text-field :text (str (or value ""))} on-change-or-opts)
     {:type :text-field
      :text (str (or value ""))
      :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge {:type :text-field
           :text (str (or value ""))
           :on-change on-change}
          opts)))
