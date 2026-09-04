(ns gpui.ui
  "Public widget constructors for clj-gpui.

  These functions return ordinary Clojure maps. The native host
  translates that data into GPUI elements. Application logic stays
  in Clojure: atoms, functions, sequences, macros, and namespaces
  are the real Clojure runtime."
  (:refer-clojure :exclude [list]))

(def protocol-version
  "Version of the Clojure↔host UI-tree protocol. Bump when the schema changes."
  10)

(def window-title
  "Default native window title when `ui/window` omits `:title`."
  "clj-gpui")

(def named-themes
  "GPUI Kit palette names the host ships (plus Default Light/Dark).

  Use the display string (`\"Tokyo Night\"`) or a kebab/underscore spelling
  (`:tokyo-night`) as `:theme`. See https://gpui-kit.com"
  ["Adventure"
   "Adventure Time"
   "Alduin"
   "Asciinema"
   "Aurora Light"
   "Ayu Dark"
   "Ayu Light"
   "Catppuccin Frappe"
   "Catppuccin Latte"
   "Catppuccin Macchiato"
   "Catppuccin Mocha"
   "Default Dark"
   "Default Light"
   "Everforest Dark"
   "Everforest Light"
   "Fahrenheit"
   "Flexoki Dark"
   "Flexoki Light"
   "Gruvbox Dark"
   "Gruvbox Light"
   "Harper"
   "Hybrid Dark"
   "Hybrid Light"
   "Jellybeans"
   "Kibble"
   "macOS Classic Dark"
   "macOS Classic Light"
   "Mellifluous Dark"
   "Mellifluous Light"
   "Molokai Dark"
   "Molokai Light"
   "Solarized Dark"
   "Solarized Light"
   "Spaceduck"
   "Tokyo Moon"
   "Tokyo Night"
   "Tokyo Storm"
   "Twilight"])

(def themes
  "Appearance keywords plus palettes *shipped* with clj-gpui.

  Custom ThemeSets from `(gpui.theme/register!)` or JSON directories are
  selected by the same `:theme` string, but they are not listed here."
  (into ["system" "light" "dark"] named-themes))

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

(defn wire-id
  "JSON-compatible identity for selected values.

  Keywords become their name (`:light` → `\"light\"`). Namespaced
  keywords keep the namespace (`:ui/dark` → `\"ui/dark\"`)."
  [x]
  (cond
    (nil? x) nil
    (keyword? x) (if-let [ns (namespace x)]
                   (str ns "/" (name x))
                   (name x))
    :else (str x)))

(defn- wire-selected
  "Single selected id, or a vector of ids for `:multiple` widgets."
  [value]
  (cond
    (nil? value) nil
    (and (sequential? value) (not (string? value))) (mapv wire-id value)
    :else (wire-id value)))

(defn option-identity
  "Original Clojure identity for an option, before `wire-id`."
  [x]
  (cond
    (nil? x) nil
    (map? x) (or (:id x) (:value x) (:label x) (:text x) (:title x))
    :else x))

(defn option-id-map
  "Map of wire id → original Clojure id.

  First option wins when two identities share a wire id (`:dark` and
  `\"dark\"` both become `\"dark\"`)."
  [xs]
  (reduce
   (fn [m x]
     (let [orig (option-identity x)
           wire (wire-id orig)]
       (if (or (nil? wire) (contains? m wire))
         m
         (assoc m wire orig))))
   {}
   (remove nil? xs)))

(defn resolve-option-id
  "Restore the original Clojure id for a host callback value.

  Vectors (accordion `:multiple`) are mapped element-wise. Unknown
  wire ids are returned as received. `nil` stays `nil`."
  [id-map wire-value]
  (cond
    (nil? wire-value) nil
    (sequential? wire-value) (mapv #(resolve-option-id id-map %) wire-value)
    :else (get id-map
               (if (string? wire-value)
                 wire-value
                 (wire-id wire-value))
               wire-value)))

(defn- wrap-option-callback
  [on-change xs]
  (if (fn? on-change)
    (let [id-map (option-id-map xs)]
      (fn [wire-value]
        (on-change (resolve-option-id id-map wire-value))))
    on-change))

(defn- with-id-callbacks
  "Restore original Clojure ids for the given option callbacks."
  [opts xs ks]
  (reduce (fn [m k]
            (let [f (get m k)]
              (cond-> m
                (fn? f) (assoc k (wrap-option-callback f xs)))))
          opts
          ks))

(defn- with-option-callback
  [opts xs]
  (with-id-callbacks opts xs [:on-change]))

(def ^:private settings-field-variants
  #{:switch :checkbox :number :dropdown :select :input
    "switch" "checkbox" "number" "dropdown" "select" "input"})

(defn- settings-field-row?
  [row]
  (and (map? row) (contains? settings-field-variants (:variant row))))

(defn- settings-group-row?
  "Nested `:items` without a field `:variant` is a group wrapper.

  A `:variant :dropdown` / `:select` field also has option `:items`."
  [row]
  (and (map? row) (seq (:items row)) (not (settings-field-row? row))))

(defn- flatten-settings-fields
  "Pages may be a flat field list or groups with nested `:items`."
  [pages]
  (mapcat
   (fn [page]
     (let [rows (or (and (map? page) (:items page)) [])]
       (mapcat
        (fn [row]
          (if (settings-group-row? row)
            (or (:items row) [])
            [row]))
        rows)))
   pages))

(defn- settings-callback-identities
  "Page ids, field ids, and dropdown option ids for callback restore."
  [pages]
  (let [fields (flatten-settings-fields pages)
        options (mapcat #(when (map? %) (or (:items %) [])) fields)]
    (concat pages fields options)))

(defn- wrap-settings-callback
  "Restore original Clojure field ids from `{:id … :value …}` payloads."
  [on-change pages]
  (if (fn? on-change)
    (let [id-map (option-id-map (settings-callback-identities pages))]
      (fn [payload]
        (let [wire (if (map? payload) (:id payload) payload)
              value (if (map? payload) (:value payload) payload)]
          (on-change {:id (resolve-option-id id-map wire)
                      :value (resolve-option-id id-map value)}))))
    on-change))

(declare option-items)

(defn- chart-label-line
  [line]
  (if (map? line)
    (cond-> {:text (str (or (:text line) (:label line) ""))}
      (some? (:color line)) (assoc :color (str (:color line)))
      (some? (:font-size line)) (assoc :font-size (:font-size line)))
    {:text (str line)}))

(defn option-item
  "Normalize a select/radio/tab/breadcrumb/accordion item to a map.

  Strings and keywords become `{:id … :label …}`. Maps keep `:id`,
  `:label` / `:text`, `:disabled`, `:on-click`, and `:content`. Chart
  items also keep `:fill`, `:stroke`, `:stroke-style`, `:inner-radius`,
  `:outer-radius`, and `:label-lines`."
  [x]
  (cond
    (nil? x) nil
    (map? x)
    (let [value (:value x)
          scalar-value? (or (nil? value)
                            (string? value)
                            (not (sequential? value)))
          id (or (:id x) (when scalar-value? value) (:label x) (:text x))
          label (or (:label x) (:text x) (:id x) (when scalar-value? value))
          content (:content x)]
      (cond-> {:id (wire-id id)
               :label (when (some? label) (str label))}
        (and (some? value) scalar-value?) (assoc :text (str value))
        (contains? x :text) (assoc :text (str (:text x)))
        (true? (:disabled x)) (assoc :disabled true)
        (fn? (:on-click x)) (assoc :on-click (:on-click x))
        (ui-node? content) (assoc :content content)
        (and (some? content) (not (ui-node? content)))
        (assoc :content (first (flatten-children [content])))
        (contains? x :checked) (assoc :checked (boolean (:checked x)))
        (some? (:height x)) (assoc :height (:height x))
        (some? (:side x)) (assoc :side (if (keyword? (:side x)) (name (:side x)) (str (:side x))))
        (some? (:variant x)) (assoc :variant (if (keyword? (:variant x))
                                               (name (:variant x))
                                               (str (:variant x))))
        (some? (:min x)) (assoc :min (:min x))
        (some? (:max x)) (assoc :max (:max x))
        (some? (:step x)) (assoc :step (:step x))
        (some? (:color x)) (assoc :color (str (:color x)))
        (some? (:stroke x)) (assoc :stroke (str (:stroke x)))
        (some? (:fill x)) (assoc :fill (str (:fill x)))
        (some? (:inner-radius x)) (assoc :inner-radius (:inner-radius x))
        (some? (:outer-radius x)) (assoc :outer-radius (:outer-radius x))
        (some? (:stroke-style x)) (assoc :stroke-style (if (keyword? (:stroke-style x))
                                                         (name (:stroke-style x))
                                                         (str (:stroke-style x))))
        (seq (:label-lines x)) (assoc :label-lines (mapv chart-label-line (:label-lines x)))
        (sequential? (:values x)) (assoc :values (vec (:values x)))
        (some? (:open x)) (assoc :open (:open x))
        (some? (:high x)) (assoc :high (:high x))
        (some? (:low x)) (assoc :low (:low x))
        (some? (:close x)) (assoc :close (:close x))
        (some? (:source x)) (assoc :source (wire-id (:source x)))
        (some? (:target x)) (assoc :target (wire-id (:target x)))
        (some? (:icon x)) (assoc :icon (wire-id (:icon x)))
        (seq (:items x)) (assoc :items (option-items (:items x)))
        (number? value) (assoc :value value)
        (and (sequential? value) (not (string? value))) (assoc :value (vec value))
        (and (contains? x :id) (contains? x :value)
             (not (number? value))
             (not (and (sequential? value) (not (string? value)))))
        (assoc :value value)))
    (keyword? x) {:id (wire-id x) :label (name x)}
    :else {:id (str x) :label (str x)}))

(defn option-items
  "Normalize a collection of option/item values, dropping nils."
  [xs]
  (into [] (keep option-item) xs))

(defn- named-size?
  [size]
  (or (keyword? size) (string? size)))

(defn- apply-control-size
  "Keyword/string `:size` becomes wire `:control-size` so pixel `:size` stays numeric."
  [opts]
  (let [size (:size opts)]
    (if (named-size? size)
      (-> opts
          (dissoc :size)
          (assoc :control-size (name size)))
      opts)))

(defn- rewrite-open
  "Clojure `:open?` becomes wire `:open` (boolean)."
  [opts]
  (if (contains? opts :open?)
    (-> opts (dissoc :open?) (assoc :open (boolean (:open? opts))))
    opts))

(defn- rewrite-selected
  "`:selected` is an alias for `:value` on list/table/tree."
  [opts]
  (cond-> (or opts {})
    (and (contains? opts :selected) (not (contains? opts :value)))
    (-> (dissoc :selected) (assoc :value (:selected opts)))
    (contains? opts :selected) (dissoc :selected)))

(defn flatten-tree-items
  "Depth-first flattening of nested `:items` (menus, trees) for id maps."
  [xs]
  (into []
        (mapcat (fn [x]
                  (let [kids (when (map? x) (:items x))]
                    (cons x (flatten-tree-items (or kids []))))))
        (remove nil? xs)))

(defn- with-nested-option-callback
  [opts xs]
  (with-option-callback opts (flatten-tree-items xs)))

(defn- merge-widget
  [base opts]
  (merge base (apply-control-size (or opts {}))))

(defn label
  "A text label. Optional style map uses GPUI-oriented keys, not CSS.

  (ui/label \"Hello\")
  (ui/label \"Hello\" {:font-size 20 :font-weight :bold})
  (ui/label \"todos\" {:font-family \".SystemUIFont\" :font-weight :light})
  (ui/label title {:on-click #(enter item) :on-double-click #(start-edit item)})"
  ([text]
   {:type :label :text (str text)})
  ([text style]
   (merge {:type :label :text (str text)} style)))

(defn button
  "A clickable button. `on-click` is a real Clojure function (often `#()`).
  Named `:size` becomes `:control-size`. `:selected` is Kit Selectable
  chrome (not list selection). `:variant` is Kit ButtonVariants:
  `:primary`, `:secondary`, `:danger`, `:warning`, `:success`, `:info`,
  `:ghost`, `:link`, `:text`. `:outline` is a separate look.

  (ui/button \"+\" #(swap! count inc))
  (ui/button \"Save\" save! {:primary true})
  (ui/button \"Warn\" {:variant :warning :size :small})"
  ([text]
   {:type :button :text (str text)})
  ([text on-click]
   (if (map? on-click)
     (merge {:type :button :text (str text)} (apply-control-size on-click))
     {:type :button :text (str text) :on-click on-click}))
  ([text on-click style]
   (merge {:type :button :text (str text) :on-click on-click}
          (apply-control-size (or style {})))))

(defn window
  "Native window. Return this from `app`. Only one makes sense.

  `:title` is the OS window title (default `clj-gpui`).
  `:chrome :dev` (default) shows the nREPL footer and the `gpui-fps`
  HUD; `:chrome :app` hides host chrome.
  `:width` / `:height` are the native window size in pixels
  (`:window-width` / `:window-height` are accepted as aliases).
  Those size keys are not layout: children fill the window.

  `:theme` may live here (default for the window and the footer) or on
  any nested node, so different parts of the app can use different themes.
  Appearance is `:system` (follow the OS), `:light`, or `:dark`. A named
  GPUI Kit palette is a string such as `\"Tokyo Night\"` (kebab
  `:tokyo-night` is the same name). Custom ThemeSets registered with
  `gpui.theme/register!` are also names. See `themes` / `named-themes`.

  (ui/window
    {:title \"Todos\" :chrome :app :width 640 :height 820 :theme \"Tokyo Night\"}
    (ui/vstack {:gap 8 :padding 16}
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

  `:theme :system`, `:light`, or `:dark` is appearance, not window chrome.
  A named GPUI Kit palette (`\"Tokyo Night\"`, `:ayu-light`) is also
  a style, as is a custom ThemeSet registered with `gpui.theme/register!`.
  It can sit on this stack, on `ui/window`, or on any other node.
  `:system` (the default when omitted) follows the OS appearance.

  (ui/vstack {:theme \"Tokyo Night\" :gap 8 :padding 16}
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

  `:shape :circle` paints a round toggle instead of Kit's square.

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
  "Scroll container. Fills leftover height in a flex parent.

  Give it a `:height` (pixels) for a fixed viewport instead. `:flex 1`
  is implied when `:height` is omitted; passing it is still fine.
  `:width` constrains the viewport (not the overflowing content).
  `:size` is a square viewport, same as on other nodes."
  [& args]
  (let [[style children] (split-style-children args)]
    (assoc style :type :scroll :children (flatten-children children))))

(defn input
  "Single-line text input rendered with GPUI Kit's Input.

  `on-change` and `:on-submit` receive the current string. Prefer a
  stable `:id` so typed text survives layout shifts. `:focus true`
  requests keyboard focus. `:on-blur` gets the string; `:on-escape`
  is 0-arg.

  (ui/input draft
            {:id \"new-todo\"
             :placeholder \"What needs to be done?\"
             :on-change #(swap! !state assoc :draft %)
             :on-submit add-todo})
  (ui/input draft
            {:id \"edit-1\"
             :focus true
             :on-submit save
             :on-blur save
             :on-escape cancel})"
  ([value]
   {:type :input :text (str (or value ""))})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge {:type :input :text (str (or value ""))} on-change-or-opts)
     {:type :input
      :text (str (or value ""))
      :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge {:type :input
           :text (str (or value ""))
           :on-change on-change}
          opts)))

(defn textarea
  "Multi-line text input (`Textarea` / `TextareaState`).

  Same string callbacks as `input`. `:rows` is the visible height
  (default 3). Prefer a stable `:id`. When `:on-submit` is set, Enter
  submits and Shift+Enter inserts a newline.

  (ui/textarea notes {:id \"notes\" :rows 6 :on-change set!})"
  ([value]
   {:type :textarea :text (str (or value ""))})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge {:type :textarea :text (str (or value ""))} on-change-or-opts)
     {:type :textarea
      :text (str (or value ""))
      :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge {:type :textarea
           :text (str (or value ""))
           :on-change on-change}
          opts)))

(defn switch
  "Toggle switch. `on-change` receives the new boolean.

  (ui/switch on? {:on-change #(swap! !state assoc :on %)})
  (ui/switch on? on-change \"Notifications\")"
  ([checked]
   {:type :switch :checked (boolean checked)})
  ([checked on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :switch :checked (boolean checked)} on-change-or-opts)
     {:type :switch :checked (boolean checked) :on-change on-change-or-opts}))
  ([checked on-change label-or-opts]
   (if (map? label-or-opts)
     (merge-widget {:type :switch
                    :checked (boolean checked)
                    :on-change on-change}
                   label-or-opts)
     {:type :switch
      :checked (boolean checked)
      :text (some-> label-or-opts str)
      :on-change on-change}))
  ([checked on-change label opts]
   (merge-widget {:type :switch
                  :checked (boolean checked)
                  :text (some-> label str)
                  :on-change on-change}
                 opts)))

(defn toggle
  "Button-style toggle (Kit Toggle), distinct from `switch`.

  `on-change` receives the new boolean.

  (ui/toggle bold? {:on-change #(swap! !state assoc :bold %) :text \"Bold\"})"
  ([checked]
   {:type :toggle :checked (boolean checked)})
  ([checked on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :toggle :checked (boolean checked)} on-change-or-opts)
     {:type :toggle :checked (boolean checked) :on-change on-change-or-opts}))
  ([checked on-change opts]
   (merge-widget {:type :toggle
                  :checked (boolean checked)
                  :on-change on-change}
                 opts)))

(defn radio-group
  "Radio group. `value` is the selected option id; `on-change` receives that id.

  Keyword ids round-trip as keywords (`:dark` not `\"dark\"`). String
  ids stay strings.

  (ui/radio-group selected
    {:options [{:id :light :label \"Light\"} {:id :dark :label \"Dark\"}]
     :on-change #(swap! !state assoc :mode %)
     :orientation :horizontal})"
  ([value]
   {:type :radio-group :value (wire-id value) :options []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})
         raw (or (:options opts) (:items opts))
         opts (with-option-callback (dissoc opts :options :items) raw)]
     (merge-widget {:type :radio-group
                    :value (wire-id value)
                    :options (option-items raw)}
                   opts))))

(defn- slider-value
  [value]
  (cond
    (and (sequential? value) (not (string? value))) (vec value)
    (some? value) value
    :else 0))

(defn- slider-opts
  [opts]
  (let [opts (apply-control-size (or opts {}))]
    (cond-> opts
      (keyword? (:scale opts)) (update :scale name))))

(defn slider
  "Numeric slider. A single value sends a number; a two-number vector is
  Kit range thumbs and `:on-change` / `:on-release` receive `[start end]`.
  `:range true` with a scalar is `min`..value. `:scale :logarithmic`
  (`:log`) needs `min > 0` (otherwise the host keeps linear so Kit does
  not assert, and warns). `:reverse` fills from the thumb to max on a
  single slider (ignored for range). Named `:size` becomes `:control-size`.
  `:on-change` fires while dragging; `:on-release` fires once after a
  real click/drag. Programmatic controlled values emit neither.

  (ui/slider volume {:min 0 :max 100 :on-change #(swap! !state assoc :vol %)})
  (ui/slider [20 70] {:min 0 :max 100 :on-change set-span! :on-release commit!})
  (ui/slider zoom {:min 0.25 :max 4 :step 0.05 :scale :logarithmic})"
  ([value]
   {:type :slider :value (slider-value value)})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge {:type :slider :value (slider-value value)} (slider-opts on-change-or-opts))
     {:type :slider :value (slider-value value) :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge {:type :slider :value (slider-value value) :on-change on-change}
          (slider-opts opts))))

(defn progress
  "Determinate progress bar. `value` is 0–100.

  (ui/progress 45)
  (ui/progress 45 {:width 240})"
  ([value]
   {:type :progress :value (or value 0)})
  ([value opts]
   (merge {:type :progress :value (or value 0)} opts)))

(defn progress-circle
  "Circular progress. `value` is 0–100. `:loading true` is Kit's
  indeterminate animation (value is ignored). Optional `:color` hex,
  `:accessibility-label`, and children painted inside the ring.

  Kit clamps `.value()` to 0..=100. Named `:size` becomes `:control-size`.

  (ui/progress-circle 45)
  (ui/progress-circle 45 {:size :large :color \"#3366ff\"} (ui/label \"45\"))
  (ui/progress-circle nil {:loading true})"
  ([value]
   {:type :progress-circle :value (or value 0)})
  ([value opts-or-child]
   (cond
     (ui-node? opts-or-child)
     {:type :progress-circle
      :value (or value 0)
      :children (flatten-children [opts-or-child])}
     (map? opts-or-child)
     (merge-widget {:type :progress-circle :value (or value 0)} opts-or-child)
     :else {:type :progress-circle
            :value (or value 0)
            :children (flatten-children [opts-or-child])}))
  ([value opts & children]
   (if (ui-node? opts)
     {:type :progress-circle
      :value (or value 0)
      :children (flatten-children (cons opts children))}
     (merge-widget {:type :progress-circle
                    :value (or value 0)
                    :children (flatten-children children)}
                   (if (map? opts) opts {})))))

(defn separator
  "Horizontal (default) or vertical rule. Optional label.

  (ui/separator)
  (ui/separator \"or\")
  (ui/separator {:orientation :vertical})
  (ui/separator \"or\" {:dashed true})"
  ([]
   {:type :separator})
  ([label-or-opts]
   (cond
     (map? label-or-opts) (merge {:type :separator} label-or-opts)
     (nil? label-or-opts) {:type :separator}
     :else {:type :separator :text (str label-or-opts)}))
  ([label opts]
   (merge {:type :separator :text (str label)} opts)))

(defn spinner
  "Loading spinner.

  (ui/spinner)
  (ui/spinner {:size :small})"
  ([]
   {:type :spinner})
  ([opts]
   (merge-widget {:type :spinner} opts)))

(defn tag
  "Small status tag. `:variant` is `:primary`, `:secondary` (default),
  `:danger`, `:success`, `:warning`, or `:info`.

  (ui/tag \"Beta\")
  (ui/tag \"Error\" {:variant :danger})"
  ([text]
   {:type :tag :text (str text)})
  ([text opts]
   (merge-widget {:type :tag :text (str text)} opts)))

(defn alert
  "Inline alert. `:variant` is `:info`, `:success`, `:warning`, `:error`,
  or omitted for secondary. `:on-close` is a 0-arg callback.

  (ui/alert \"Saved\" {:variant :success :title \"Done\"})"
  ([message]
   {:type :alert :text (str message)})
  ([message opts]
   (merge-widget {:type :alert :text (str message)} opts)))

(defn skeleton
  "Loading placeholder bar.

  (ui/skeleton)
  (ui/skeleton {:width 200 :height 16})"
  ([]
   {:type :skeleton})
  ([opts]
   (merge {:type :skeleton} opts)))

(defn shimmer
  "Animated loading text (Kit `ShimmerText`). Omitted options keep Kit
  defaults (2s sweep, relative spread 0.3, looping left-to-right).
  `:duration` is seconds. `:spread` is a fraction of the text width;
  `:spread-px` is an absolute half-width and wins when both are set.
  `:highlight-color` is the sweep hex (layout `:color` is still text).

  (ui/shimmer \"Thinking…\")
  (ui/shimmer \"Indexing…\" {:duration 1 :spread 0.4 :reverse true})"
  ([text]
   {:type :shimmer :text (str text)})
  ([text opts]
   (merge-widget {:type :shimmer :text (str text)} opts)))

(defn kbd
  "Keyboard shortcut chip. `stroke` is a GPUI keystroke such as `\"ctrl-s\"`.

  (ui/kbd \"ctrl-s\")"
  ([stroke]
   {:type :kbd :text (str stroke)})
  ([stroke opts]
   (merge {:type :kbd :text (str stroke)} opts)))

(defn link
  "Themed link. Opens `href` in the system handler when set.
  `:on-click` is 0-arg and runs in addition to opening the URL.

  (ui/link \"https://clojure.org\" \"Clojure\")
  (ui/link href label {:on-click track!})"
  ([href]
   {:type :link :href (str href) :text (str href)})
  ([href label-or-opts]
   (if (map? label-or-opts)
     (merge {:type :link :href (str href) :text (str href)} label-or-opts)
     {:type :link :href (str href) :text (str label-or-opts)}))
  ([href label opts]
   (merge {:type :link :href (str href) :text (str label)} opts)))

(defn group-box
  "Titled group. `:variant` is `:normal` (default), `:fill`, or `:outline`.

  (ui/group-box {:title \"Audio\" :variant :outline}
    (ui/label \"Volume\"))"
  [& args]
  (let [[style children] (split-style-children args)]
    (assoc (apply-control-size style)
           :type :group-box
           :children (flatten-children children))))

(defn badge
  "Count or dot overlay around a child.

  (ui/badge 3 (ui/icon :bell))
  (ui/badge {:dot true} (ui/button \"Alerts\" …))"
  ([count-or-opts child]
   (if (map? count-or-opts)
     (merge-widget {:type :badge :children (flatten-children [child])}
                   count-or-opts)
     {:type :badge
      :count (long (or count-or-opts 0))
      :children (flatten-children [child])}))
  ([count child opts]
   (merge-widget {:type :badge
                  :count (long (or count 0))
                  :children (flatten-children [child])}
                 opts)))

(defn tabs
  "Tab bar. `value` is the selected tab id; `on-change` receives that id.

  Content is not included — render the selected panel in Clojure.
  Keyword ids round-trip as keywords.

  (ui/tabs selected
    {:items [{:id :general :label \"General\"}
             {:id :advanced :label \"Advanced\"}]
     :on-change #(swap! !state assoc :tab %)
     :variant :underline})"
  ([value]
   {:type :tabs :value (wire-id value) :items []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})
         raw (or (:items opts) (:options opts))
         opts (with-option-callback (dissoc opts :items :options) raw)]
     (merge-widget {:type :tabs
                    :value (wire-id value)
                    :items (option-items raw)}
                   opts))))

(defn select
  "Dropdown select. `value` is the selected option id; `on-change` receives that id.

  `nil` clears the selection. `:searchable true` filters options by
  label as the user types. Keyword ids round-trip as keywords.

  (ui/select selected
    {:options [{:id :clj :label \"Clojure\"} {:id :rs :label \"Rust\"}]
     :placeholder \"Language\"
     :searchable true
     :on-change #(swap! !state assoc :lang %)})"
  ([value]
   {:type :select :value (wire-id value) :options []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})
         raw (or (:options opts) (:items opts))
         opts (with-option-callback (dissoc opts :options :items) raw)]
     (merge-widget {:type :select
                    :value (wire-id value)
                    :options (option-items raw)}
                   opts))))

(defn combobox
  "Searchable dropdown. Kit `Combobox`. `value` is the selected option
  id, or a vector of ids when `:multiple true`. `:on-change` receives
  that id (or vector). `:on-confirm` fires when the menu closes.
  Search is on by default.

  A single-select pick can emit Kit `Change` then `Confirm` for one
  user action. The host sends `:on-change` then `:on-confirm` as one
  batch against the same callback generation, then fetches one tree.
  Confirm without Change (dismiss) is `:on-confirm` only.

  Controlled selection is pushed to Kit when the ids change *or* the
  option collection changes (`set_items` does not rebuild Kit's cloned
  selection, so a renamed or removed selected option would otherwise
  stick). Kit `set_selected_values` clears the search query, so an
  unrelated atom rerender with the same options and ids must not wipe
  in-progress typing. A native `Change` updates the host cache first:
  Clojure echoing those same ids is a no-op; a different Clojure value
  still overrides native state.

  (ui/combobox selected
    {:options [{:id :clj :label \"Clojure\"} {:id :rs :label \"Rust\"}]
     :placeholder \"Language\"
     :on-change set-lang!})
  (ui/combobox picked
    {:options langs :multiple true :on-change set-picked!})"
  ([value]
   {:type :combobox :searchable true :value (wire-selected value) :options []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})
         raw (or (:options opts) (:items opts))
         searchable (if (contains? opts :searchable)
                      (boolean (:searchable opts))
                      true)
         opts (with-id-callbacks
                (-> opts
                    (dissoc :options :items)
                    (assoc :searchable searchable))
                raw
                [:on-change :on-confirm])]
     (merge-widget {:type :combobox
                    :value (wire-selected value)
                    :options (option-items raw)}
                   opts))))

(defn icon
  "Bundled GPUI Kit icon. `name` is a kebab keyword such as `:check`.

  (ui/icon :star)
  (ui/icon :loader {:size :small})"
  ([name]
   {:type :icon :icon (wire-id name)})
  ([name opts]
   (merge-widget {:type :icon :icon (wire-id name)} opts)))

(defn clipboard
  "Copy-to-clipboard button. `:on-copied` receives the copied string.

  (ui/clipboard \"https://example.com\")"
  ([value]
   {:type :clipboard :text (str (or value ""))})
  ([value opts]
   (merge {:type :clipboard :text (str (or value ""))} opts)))

(defn breadcrumb
  "Breadcrumb trail. Non-last items with `:on-click` are links.
  Group `:on-change` receives the clicked item id (original Clojure id).

  (ui/breadcrumb [{:id :home :label \"Home\" :on-click go-home!}
                  {:label \"Project\"}])"
  ([items]
   {:type :breadcrumb :items (option-items items)})
  ([items opts]
   (merge {:type :breadcrumb :items (option-items items)}
          (with-option-callback opts items))))

(defn avatar
  "Avatar. Initials from `:name` or a string. `:src` is a Kit image
  source (http URL or file path). Remote http URLs load through the
  host HTTP client. `:icon` is the placeholder when there is no image
  (Kit default User).

  (ui/avatar \"Ada Lovelace\")
  (ui/avatar {:name \"Ada Lovelace\" :src \"https://example.com/ada.png\"})
  (ui/avatar \"Ada\" {:src \"https://example.com/ada.png\" :size :large})"
  ([name-or-opts]
   (if (map? name-or-opts)
     (avatar (or (:name name-or-opts) (:text name-or-opts)) name-or-opts)
     {:type :avatar :text (str name-or-opts)}))
  ([name opts]
   (let [opts (or opts {})
         src (when-let [s (:src opts)]
               (let [text (str s)]
                 (when (seq text) text)))
         icon (:icon opts)]
     (cond-> (merge-widget {:type :avatar
                            :text (when (some? name) (str name))}
                           (dissoc opts :name :text :src :icon))
       (some? src) (assoc :src src)
       (some? icon) (assoc :icon (wire-id icon))))))

(defn- avatar-child
  [n]
  (cond
    (= (:type n) :avatar) n
    (and (= (:type n) :label) (seq (:text n))) (avatar (:text n))
    :else nil))

(defn avatar-group
  "Overlapping avatars. Kit `AvatarGroup`. Omitted `:limit` keeps Kit's
  3. `:ellipsis true` adds a ⋯ overflow avatar when there are more
  than the limit.

  (ui/avatar-group {:limit 5 :ellipsis true :size :small}
    (ui/avatar \"Ada\")
    (ui/avatar {:name \"Grace\" :src \"https://example.com/grace.png\"}))
  (ui/avatar-group (map ui/avatar names))"
  [& args]
  (let [[style children] (split-style-children args)]
    (merge-widget {:type :avatar-group
                   :children (into [] (keep avatar-child) (flatten-children children))}
                  style)))

(defn accordion
  "Accordion. `value` is the open item id (`nil` when all closed).
  `on-change` receives that id, or a vector of ids when `:multiple true`.

  Keyword ids round-trip as keywords. Multiple open ids are a JSON
  array on the wire, not a comma-joined string.

  (ui/accordion open-id
    {:on-change set-open!
     :items [{:id :a :title \"One\" :content (ui/label \"…\")}
             {:id :b :title \"Two\" :content (ui/label \"…\")}]})"
  ([value]
   {:type :accordion :value (wire-id value) :items []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})
         raw (or (:items opts) [])
         items (mapv (fn [item]
                       (let [n (option-item
                                (cond-> item
                                  (and (map? item) (contains? item :title)
                                       (not (contains? item :label)))
                                  (assoc :label (:title item))))]
                         (cond-> n
                           (and (map? item) (contains? item :content))
                           (assoc :content (let [c (:content item)]
                                             (if (ui-node? c)
                                               c
                                               (first (flatten-children [c]))))))))
                     raw)
         opts (with-option-callback (dissoc opts :items) raw)]
     (merge-widget {:type :accordion
                    :value (if (sequential? value)
                             (mapv wire-id value)
                             (wire-id value))
                    :items items}
                   opts))))

(defn- description-item [item]
  (if (map? item)
    (cond-> {:id (str (or (:label item) ""))
             :label (str (or (:label item) ""))
             :text (str (or (:value item) (:text item) ""))}
      (some? (:span item)) (assoc :span (:span item)))
    {:id (str item) :label (str item) :text ""}))

(defn description-list
  "Key/value description list. Defaults to a vertical stack (one pair per row).

  (ui/description-list [{:label \"Name\" :value \"Ada\"}
                        {:label \"Lang\" :value \"Clojure\"}])
  (ui/description-list items {:orientation :horizontal :columns 2})"
  ([items]
   (description-list items nil))
  ([items opts]
   (merge-widget {:type :description-list
                  :orientation :vertical
                  :items (mapv description-item items)}
                 (dissoc opts :items))))

(defn- leading-opts
  "Split a trailing args seq into an optional style map and children."
  [args]
  (if (and (seq args)
           (map? (first args))
           (not (ui-node? (first args))))
    [(first args) (rest args)]
    [{} args]))

(defn menu-item
  "Normalize a popup-menu row. `-` / `:-` / `{:separator true}` is a rule.
  Nested `:items` become a submenu."
  [x]
  (cond
    (nil? x) nil
    (or (= x :-) (= x "-") (= x :separator)) {:separator true}
    (and (map? x) (or (true? (:separator x))
                      (= (:id x) :-)
                      (= (:id x) "-")))
    {:separator true}
    (map? x)
    (let [n (option-item x)]
      (cond-> n
        (true? (:checked x)) (assoc :checked true)
        (some? (:icon x)) (assoc :icon (wire-id (:icon x)))
        (seq (:items x)) (assoc :items (into [] (keep menu-item) (:items x)))))
    :else (option-item x)))

(defn menu-items
  "Normalize popup-menu / context-menu / dropdown-menu / dropdown-button rows."
  [xs]
  (into [] (keep menu-item) xs))

(defn tree-item
  "Normalize a tree row. Nested `:items` are children; `:expanded` is initial."
  [x]
  (let [n (option-item x)]
    (when n
      (cond-> n
        (and (map? x) (true? (:expanded x))) (assoc :expanded true)
        (and (map? x) (seq (:items x)))
        (assoc :items (into [] (keep tree-item) (:items x)))))))

(defn- table-column [x]
  (let [n (option-item x)]
    (cond-> n
      (and (map? x) (some? (:width x))) (assoc :width (:width x))
      (and (map? x) (some? (:span x))) (assoc :span (:span x))
      (and (map? x) (some? (:align x)))
      (assoc :align (if (keyword? (:align x)) (name (:align x)) (str (:align x)))))))

(defn- data-table-row [x]
  (cond
    (nil? x) nil
    (and (sequential? x) (not (string? x)))
    (data-table-row {:cells (mapv str x)})
    (map? x)
    (let [cells (mapv str (or (:cells x) []))
          id (or (:id x) (:value x) (first cells) (:label x))
          label (or (:label x) (first cells) id)]
      (cond-> {:id (wire-id id)
               :label (when (some? label) (str label))
               :cells cells}
        (empty? cells) (assoc :cells [(str (or label id))])))
    :else {:id (str x) :label (str x) :cells [(str x)]}))

(defn dialog
  "Modal dialog on the overlay layer. Controlled by `open?` (or `:open?`).

  Not painted inline — the host opens it through GPUI Kit `Root`.
  The open dialog always uses the latest Clojure tree: callback ids,
  title, and body update on the next paint without closing. `:on-close`
  is 0-arg. `:on-ok` / `:on-cancel` are 0-arg. Crate order per action:

  * OK → `:on-ok`, then `:on-close` (and `:on-open-change false`)
  * Cancel, Escape, close button, overlay click → `:on-cancel`, then
    `:on-close` (and `:on-open-change false`)

  The host sends that whole chain as one batch against the same callback
  generation, then fetches one tree. `:on-ok` cannot re-export and rewire
  `:on-close`. Each handler runs at most once per action. `:variant` is
  `:confirm` (OK+Cancel) or omitted (content + close button). Alerts that
  must not dismiss on backdrop use `ui/alert-dialog`. Clicking the dimmed
  overlay dismisses a generic dialog unless `:overlay-closable false`.
  Confirm dialogs follow Kit (not overlay-closable unless you set it).

  (ui/dialog open?
    {:title \"Delete?\" :variant :confirm :on-ok delete! :on-close hide!}
    (ui/label \"This cannot be undone.\"))"
  [open?-or-opts & args]
  (let [[open? opts children]
        (if (or (boolean? open?-or-opts) (nil? open?-or-opts))
          (let [[opts children] (leading-opts args)]
            [open?-or-opts opts children])
          (let [[opts children] (leading-opts (cons open?-or-opts args))]
            [(or (:open? opts) (:open opts) false) opts children]))
        opts (-> opts rewrite-open (dissoc :open?) apply-control-size)]
    (merge {:type :dialog
            :open (boolean open?)
            :children (flatten-children children)}
           opts
           {:open (boolean open?)})))

(defn alert-dialog
  "Alert dialog overlay. Kit `AlertDialog`: not backdrop-dismissible.

  Same controlled `open?` / `:on-ok` / `:on-cancel` / `:on-close`
  contract as `ui/dialog`. Confirm still closes unless
  `:overlay-closable` is true.

  (ui/alert-dialog open?
    {:title \"Delete?\" :variant :confirm :on-ok delete! :on-close hide!}
    (ui/label \"This cannot be undone.\"))"
  [open?-or-opts & args]
  (let [[open? opts children]
        (if (or (boolean? open?-or-opts) (nil? open?-or-opts))
          (let [[opts children] (leading-opts args)]
            [open?-or-opts opts children])
          (let [[opts children] (leading-opts (cons open?-or-opts args))]
            [(or (:open? opts) (:open opts) false) opts children]))
        opts (-> opts rewrite-open (dissoc :open?) apply-control-size)]
    (merge {:type :alert-dialog
            :open (boolean open?)
            :children (flatten-children children)}
           opts
           {:open (boolean open?)})))

(defn popover
  "Anchored popover. Controlled by `open?`. `:trigger` is a button (or
  label wrapped as a button). `:on-open-change` receives the new boolean.

  Content is rebuilt each paint from the child nodes (label, button,
  stacks, separator).

  (ui/popover open?
    {:trigger (ui/button \"More\") :on-open-change set-open!}
    (ui/label \"Hint\")
    (ui/button \"Do it\" do!))"
  [open? & args]
  (let [[opts children] (leading-opts args)
        trigger (:trigger opts)
        opts (-> opts
                 (dissoc :trigger)
                 rewrite-open
                 apply-control-size)]
    (cond-> (merge {:type :popover
                    :open (boolean open?)
                    :children (flatten-children children)}
                   opts
                   {:open (boolean open?)})
      (ui-node? trigger) (assoc :trigger trigger)
      (and (some? trigger) (not (ui-node? trigger)))
      (assoc :trigger (button (str trigger))))))

(defn- rewrite-anchor
  "Clojure `:anchor` becomes wire `:placement` when placement was omitted."
  [opts]
  (cond-> opts
    (and (contains? opts :anchor) (not (contains? opts :placement)))
    (assoc :placement (wire-id (:anchor opts)))
    (contains? opts :anchor) (dissoc :anchor)))

(defn hover-card
  "Hover-triggered card. Kit `HoverCard`. Not click-controlled like
  `ui/popover`. Omitted `:open-delay` / `:close-delay` keep Kit's 0.6s
  / 0.3s. `:placement` (or `:anchor`) is a Kit Anchor (`:top-center`
  default). `:trigger` is any widget, not only a button.
  `:on-open-change` receives the new boolean. Children are the card
  body. `:appearance false` drops Kit's default popover chrome.

  (ui/hover-card {:trigger (ui/link \"https://example.com\" \"@ada\")
                  :open-delay 0.2}
    (ui/label \"Ada Lovelace\")
    (ui/avatar {:name \"Ada\" :src \"https://example.com/ada.png\"}))"
  [& args]
  (let [[opts children] (leading-opts args)
        trigger (:trigger opts)
        opts (-> opts
                 (dissoc :trigger)
                 rewrite-anchor
                 apply-control-size)
        trigger (cond
                  (ui-node? trigger) trigger
                  (string? trigger) (label trigger)
                  (some? trigger) (label (str trigger))
                  :else nil)]
    (cond-> (merge {:type :hover-card
                    :children (flatten-children children)}
                   opts)
      (some? trigger) (assoc :trigger trigger))))

(defn dropdown-menu
  "Button that opens a popup menu. `on-change` receives the original item id.

  Item `:on-click` (0-arg) runs before the menu `:on-change` for the same
  selection. Both use the same callback generation; the host then fetches
  one tree.

  (ui/dropdown-menu [{:id :copy :label \"Copy\"} :- {:id :paste :label \"Paste\"}]
                    {:on-change handle!}
                    (ui/button \"Edit\"))"
  ([items trigger]
   (dropdown-menu items nil trigger))
  ([items opts trigger]
   (let [raw (or items [])
         opts (with-nested-option-callback
                (-> (or opts {})
                    (dissoc :items :trigger)
                    rewrite-open
                    apply-control-size)
                raw)
         trigger (cond
                   (ui-node? trigger) trigger
                   (string? trigger) (button trigger)
                   :else (button (str (or trigger "Menu"))))]
     (merge {:type :dropdown-menu
             :items (menu-items raw)
             :trigger trigger}
            opts))))

(defn dropdown-button
  "Split action button plus menu. Kit `DropdownButton`. `on-change`
  receives the original item id, same batch as `ui/dropdown-menu`
  (item `:on-click` then menu `:on-change`). The action half is `:trigger`
  (a button, or a string wrapped as one). Its `:on-click` fires when that
  half is pressed. Omit the trigger for a menu-only split (Kit-valid).
  Outer `:variant` / `:size` / `:outline` / `:selected` / `:disabled`
  apply to both halves. Unset outer size/variant inherit from the inner
  action Button. `:variant` is Kit ButtonVariants (`:primary`,
  `:secondary`, `:danger`, `:warning`, `:success`, `:info`, `:ghost`,
  `:link`, `:text`). `:placement` (or `:anchor`) is the menu `Anchor`
  (Kit default `top-right`).

  (ui/dropdown-button [{:id :csv :label \"CSV\"} {:id :pdf :label \"PDF\"}]
                      {:on-change handle! :variant :primary :selected true}
                      (ui/button \"Export\" export!))"
  ([items]
   (dropdown-button items nil nil))
  ([items trigger]
   (dropdown-button items nil trigger))
  ([items opts trigger]
   (let [raw (or items [])
         opts (with-nested-option-callback
                (-> (or opts {})
                    (dissoc :items :trigger)
                    rewrite-anchor
                    apply-control-size)
                raw)
         trigger (cond
                   (ui-node? trigger) trigger
                   (string? trigger) (button trigger)
                   (some? trigger) (button (str trigger))
                   :else nil)]
     (cond-> (merge {:type :dropdown-button
                     :items (menu-items raw)}
                    opts)
       (some? trigger) (assoc :trigger trigger)))))

(defn context-menu
  "Right-click menu around a child. `on-change` receives the original item id.

  Same selection batch as `dropdown-menu`: item `:on-click` then menu
  `:on-change`, one tree fetch.

  The host is a flex column. Wrapping a `:flex 1` list/table/tree keeps
  leftover height (a block wrapper would collapse those viewports). Put
  `:flex 1` on the child, the menu, or both.

  (ui/context-menu [{:id :copy :label \"Copy\"} {:id :paste :label \"Paste\"}]
                   {:on-change handle!}
                   (ui/data-table {:columns cols :rows rows :flex 1}))"
  ([items child]
   (context-menu items nil child))
  ([items opts child]
   (let [raw (or items [])
         opts (with-nested-option-callback
                (-> (or opts {})
                    (dissoc :items)
                    apply-control-size)
                raw)]
     (merge {:type :context-menu
             :items (menu-items raw)
             :children (flatten-children [child])}
            opts))))

(defn list
  "Virtualized list of `{id, label}` rows. `value` / `:selected` is the
  selected id; `on-change` receives that original Clojure id.

  `:on-change` fires when selection changes (arrow keys, and also the
  selection implied by a confirm). `:on-confirm` fires when the item is
  activated (mouse click or Enter). Kit emits Select for
  arrows and Confirm only for click/Enter; the host maps those to this
  contract. Click/Enter is one batch: `:on-change` then `:on-confirm`
  against the same callback generation, then one tree fetch. Escape /
  Cancel sends `on-change` with `nil`. `:searchable true` filters by label
  and keeps that query when Clojure replaces the rows.

  (ui/list items {:selected sel :on-change set-sel! :searchable true :height 200})"
  ([items]
   (list items nil))
  ([items opts]
   (let [raw (or items [])
         opts (-> (or opts {})
                  rewrite-selected
                  apply-control-size)
         selected (:value opts)
         opts (with-id-callbacks
                (dissoc opts :items :options :value)
                raw
                [:on-change :on-confirm])]
     (merge-widget {:type :list
                    :value (wire-id selected)
                    :items (option-items raw)}
                   opts))))

(defn data-table
  "Virtualized data table (Kit DataTable). `:columns` are `{id, label, width}`
  maps (not the description-list `:columns` count). `:rows` are
  `{id, cells [...]}`. `on-change` receives the selected row's original
  id. `:on-confirm` (or `:on-double-click`) fires on double-click with
  that same original id. Kit `on_row_left_click` always emits
  `SelectRow`; when `click_count` is 2 it then emits `DoubleClickedRow`
  from that same call. A count-1 click is only `:on-change` (deferred to
  the end of the GPUI effect cycle). A count-2 click is `:on-change`
  then `:on-confirm` as one batch against one callback generation, then
  one tree fetch. Programmatic `:selected` does not emit `:on-change`.

  `ui/table` is Kit's declarative (non-virtualized) Table.

  (ui/data-table {:columns [{:id :name :label \"Name\"} {:id :lang :label \"Lang\"}]
                  :rows [{:id :ada :cells [\"Ada\" \"Clojure\"]}]
                  :selected :ada
                  :on-change set-row!})"
  [opts]
  (let [opts (if (map? opts) opts {})
        columns (or (:columns opts) (:options opts) [])
        rows (or (:rows opts) (:items opts) [])
        opts (-> opts
                 (dissoc :columns :rows :items :options)
                 rewrite-selected
                 apply-control-size)
        selected (:value opts)
        opts (with-id-callbacks
               (dissoc opts :value)
               rows
               [:on-change :on-confirm :on-double-click])]
    (merge-widget {:type :data-table
                   :value (wire-id selected)
                   :options (into [] (keep table-column) columns)
                   :items (into [] (keep data-table-row) rows)}
                  opts)))

(defn- table-type? [x expected]
  (and (ui-node? x) (= (name (:type x)) expected)))

(defn- table-align-name [x]
  (when (some? x)
    (if (keyword? x) (name x) (str x))))

(defn- table-section-opts
  "Wire opts for table sections/cells. `:span` 0/1 is omitted (Kit default)."
  [opts]
  (let [opts (apply-control-size (or opts {}))
        align (:align opts)
        span (:span opts)
        a11y (:accessibility-label opts)]
    (cond-> opts
      (some? align) (assoc :align (table-align-name align))
      (or (nil? span) (not (number? span)) (<= span 1)) (dissoc :span)
      (some? a11y) (assoc :accessibility-label
                          (if (keyword? a11y) (name a11y) (str a11y))))))

(defn table-caption
  "Visible caption below a `ui/table`. Children may be text or widgets.

  (ui/table-caption \"Recent invoices\")"
  [& args]
  (let [[opts children] (leading-opts args)]
    (assoc (table-section-opts opts)
           :type :table-caption
           :children (flatten-children children))))

(defn table-head
  "Header cell. `:span` is Kit `col_span` for this cell only. `:align`
  is `:start` / `:center` / `:end`. Children may be any clj-gpui nodes.

  (ui/table-head {:span 2 :align :end} \"Amount\")"
  [& args]
  (let [[opts children] (leading-opts args)]
    (assoc (table-section-opts opts)
           :type :table-head
           :children (flatten-children children))))

(defn table-cell
  "Body or footer cell. Same `:span` / `:align` / `:width` as `table-head`.
  Children may be any clj-gpui nodes (avatar, button, stack, badge, …).

  (ui/table-cell {:span 2 :align :end} \"Total\")
  (ui/table-cell (ui/badge 1 (ui/label \"Ada\")))"
  [& args]
  (let [[opts children] (leading-opts args)]
    (assoc (table-section-opts opts)
           :type :table-cell
           :children (flatten-children children))))

(defn table-row
  "A row of `table-head` and/or `table-cell` children.

  (ui/table-row (ui/table-head \"Name\") (ui/table-head \"Lang\"))"
  [& args]
  (let [[opts children] (leading-opts args)]
    (assoc (table-section-opts opts)
           :type :table-row
           :children (flatten-children children))))

(defn table-header
  "Header section wrapping `table-row`s.

  (ui/table-header (ui/table-row (ui/table-head \"Name\")))"
  [& args]
  (let [[opts children] (leading-opts args)]
    (assoc (table-section-opts opts)
           :type :table-header
           :children (flatten-children children))))

(defn table-body
  "Body section wrapping `table-row`s."
  [& args]
  (let [[opts children] (leading-opts args)]
    (assoc (table-section-opts opts)
           :type :table-body
           :children (flatten-children children))))

(defn table-footer
  "Footer section wrapping `table-row`s. A footer cell may span columns
  independently of the body."
  [& args]
  (let [[opts children] (leading-opts args)]
    (assoc (table-section-opts opts)
           :type :table-footer
           :children (flatten-children children))))

(defn- table-head-from-col [col]
  (cond
    (nil? col) nil
    (table-type? col "table-head") col
    (ui-node? col) (table-head col)
    (map? col)
    (table-head (select-keys col [:span :align :width :id])
                (str (or (:label col) (:text col) (:id col) "")))
    :else (table-head (str col))))

(defn- wrap-table-cell [cell align width]
  (let [inherited (cond-> {}
                    (some? align) (assoc :align align)
                    (some? width) (assoc :width width))]
    (cond
      (nil? cell) (table-cell inherited "")
      (table-type? cell "table-cell") cell
      (ui-node? cell) (table-cell inherited cell)
      (map? cell)
      (let [content (if (contains? cell :content)
                      (:content cell)
                      (or (:text cell) (:label cell) ""))
            opts (-> cell
                     (dissoc :content :text :label :cells)
                     (cond-> (nil? (:align cell)) (merge (select-keys inherited [:align]))
                             (nil? (:width cell)) (merge (select-keys inherited [:width]))))]
        (table-cell opts content))
      :else (table-cell inherited cell))))

(defn- cells-of-row [row]
  (cond
    (nil? row) []
    (and (sequential? row) (not (string? row))) (vec row)
    (map? row) (vec (or (:cells row) []))
    :else [row]))

(defn- table-row-from-data [row aligns widths]
  (if (table-type? row "table-row")
    row
    (apply table-row
           (map-indexed
            (fn [i cell]
              (wrap-table-cell cell (get aligns i) (get widths i)))
            (cells-of-row row)))))

(defn- expand-table-shorthand [opts]
  (let [cols (or (:columns opts) (:options opts) [])
        rows (or (:rows opts) (:items opts) [])
        footer (:footer opts)
        caption (or (:caption opts) (:text opts))
        rest-opts (dissoc opts :columns :rows :items :options :footer :caption :text)
        aligns (mapv #(when (map? %) (table-align-name (:align %))) cols)
        widths (mapv #(when (map? %) (:width %)) cols)
        header (when (seq cols)
                 (table-header (apply table-row (keep table-head-from-col cols))))
        body (when (seq rows)
               (apply table-body (map #(table-row-from-data % aligns widths) rows)))
        foot (when (some? footer)
               (table-footer (table-row-from-data footer aligns widths)))
        cap (when (some? caption)
              (if (table-type? caption "table-caption")
                caption
                (table-caption caption)))
        children (cond-> []
                   header (conj header)
                   body (conj body)
                   foot (conj foot)
                   cap (conj cap))]
    (assoc (table-section-opts rest-opts)
           :type :table
           :children children)))

(defn- table-shorthand? [opts children]
  (and (empty? children)
       (or (contains? opts :columns)
           (contains? opts :rows)
           (contains? opts :footer)
           (contains? opts :caption)
           (contains? opts :options)
           (contains? opts :items)
           (contains? opts :text))))

(defn table
  "Declarative Kit Table. Not virtualized — use `ui/data-table` for
  large row sets.

  Convenience APIs may simplify Kit, but they must not hide it.
  The Kit primitives are always available:

  (ui/table
    (ui/table-header
      (ui/table-row
        (ui/table-head \"Name\")
        (ui/table-head {:align :end} \"Amount\")))
    (ui/table-body
      (ui/table-row
        (ui/table-cell (ui/avatar \"Ada\") (ui/label \"Ada\"))
        (ui/table-cell {:align :end} \"$250\")))
    (ui/table-footer
      (ui/table-row
        (ui/table-cell {:span 2 :align :end} \"Total $250\")))
    (ui/table-caption \"Recent invoices\"))

  `table-head` and `table-cell` accept any clj-gpui children, not only
  strings. `:span` / `:align` / `:width` belong on the individual cell.
  `:accessibility-label` is Kit `Table::accessibility_label` (the name a
  screen reader announces). A visible `:caption` is not used as that name.

  `{:columns … :rows … :footer … :caption …}` remains as shorthand and
  expands into those primitives. Column `:span` applies to the header
  cell only, not every body cell. Column `:align` / `:width` copy onto
  plain string cells. A footer cell may span independently:

  (ui/table {:columns [{:label \"Name\"} {:label \"Amount\" :align :end}]
             :rows [[\"Ada\" \"$250\"] [\"Rich\" \"$150\"]]
             :footer [\"Total\" \"$400\"]
             :caption \"Recent invoices\"
             :accessibility-label \"Recent invoices\"})"
  [& args]
  (let [[opts children] (leading-opts args)]
    (if (table-shorthand? opts children)
      (expand-table-shorthand opts)
      (assoc (table-section-opts (dissoc opts :columns :rows :items :options
                                         :footer :caption))
             :type :table
             :children (flatten-children children)))))

(defn tree
  "Tree of nested `{id, label, items}` rows. `:expanded true` is the initial
  fold; later expand/collapse stays host-local until item identity changes.
  `:selected` is controlled. Nested ids apply when their ancestors are
  expanded (visible). `on-change` receives the clicked node's original id.

  (ui/tree [{:id :src :label \"src\" :expanded true
             :items [{:id :lib :label \"lib.rs\"}]}]
           {:selected :lib :on-change set-node!})"
  ([items]
   (tree items nil))
  ([items opts]
   (let [raw (or items [])
         opts (-> (or opts {})
                  (dissoc :items)
                  rewrite-selected
                  apply-control-size)
         selected (:value opts)
         opts (with-nested-option-callback (dissoc opts :value) raw)]
     (merge-widget {:type :tree
                    :value (wire-id selected)
                    :items (into [] (keep tree-item) raw)}
                   opts))))

(defn sheet
  "Slide-over sheet on the overlay layer. Controlled by `open?`.

  Kit holds one active sheet. The last open sheet in
  tree order wins. `:placement` is `:left` / `:right` / `:top` /
  `:bottom` (default `:right`). `:footer` is a child node. Overlay
  click dismisses unless `:overlay-closable false`. `:on-close` is
  0-arg; `:on-open-change` receives `false` on dismiss.

  (ui/sheet open?
    {:title \"Inspect\" :placement :right :on-close hide!}
    (ui/label \"Details\"))"
  [open?-or-opts & args]
  (let [[open? opts children]
        (if (or (boolean? open?-or-opts) (nil? open?-or-opts))
          (let [[opts children] (leading-opts args)]
            [open?-or-opts opts children])
          (let [[opts children] (leading-opts (cons open?-or-opts args))]
            [(or (:open? opts) (:open opts) false) opts children]))
        footer (:footer opts)
        opts (-> opts
                 (dissoc :footer)
                 rewrite-open
                 apply-control-size)]
    (cond-> (merge {:type :sheet
                    :open (boolean open?)
                    :children (flatten-children children)}
                   opts
                   {:open (boolean open?)})
      (ui-node? footer) (assoc :footer footer)
      (and (some? footer) (not (ui-node? footer)))
      (assoc :footer (first (flatten-children [footer]))))))

(defn notification
  "Toast on the overlay stack. Presence in the tree shows it unless
  `:open? false`. `:variant` is `:info` (default), `:success`,
  `:warning`, or `:error`. `:autohide` defaults true. Unchanged
  title/message/variant/autohide is not re-pushed (that would reset
  the hide timer). Dismiss fires 0-arg `:on-close`. Click fires
  0-arg `:on-click`.

  (ui/notification {:variant :success :title \"Saved\" :message \"ok\"})"
  ([message-or-opts]
   (if (map? message-or-opts)
     (let [opts (rewrite-open (apply-control-size message-or-opts))]
       (merge {:type :notification} opts))
     {:type :notification :message (str message-or-opts)}))
  ([message opts]
   (merge {:type :notification :message (str message)}
          (rewrite-open (apply-control-size (or opts {}))))))

(defn number-input
  "Numeric field with step buttons. `on-change` receives a number.
  `:min` / `:max` / `:step` clamp stepper clicks. Typed values emit
  when they parse as a number.

  (ui/number-input 42 {:min 0 :max 100 :step 1 :on-change set!})"
  ([value]
   {:type :number-input :value (or value 0) :text (str (or value 0))})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :number-input
                    :value (or value 0)
                    :text (str (or value 0))}
                   on-change-or-opts)
     {:type :number-input
      :value (or value 0)
      :text (str (or value 0))
      :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge-widget {:type :number-input
                  :value (or value 0)
                  :text (str (or value 0))
                  :on-change on-change}
                 opts)))

(defn rating
  "Star rating. `value` is 0..=`:max` (default 5). `:on-change` receives
  the new integer. Optional `:color` is a hex fill.

  Kit clamps `.value()` to the current max (default 5), so the host
  applies `:max` before `:value`. `(ui/rating 8 {:max 10})` is 8, not 5.

  (ui/rating 3 {:max 5 :on-change set!})"
  ([value]
   {:type :rating :value (or value 0)})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :rating :value (or value 0)} on-change-or-opts)
     {:type :rating :value (or value 0) :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge-widget {:type :rating :value (or value 0) :on-change on-change}
                 opts)))

(defn stepper
  "Step progress. `value` is the selected item id. `:on-change` receives
  that original id. `:orientation :vertical` stacks the steps.

  (ui/stepper :pay
    {:items [{:id :cart :label \"Cart\"}
             {:id :pay :label \"Pay\"}
             {:id :done :label \"Done\"}]
     :on-change set-step!})"
  ([value]
   {:type :stepper :value (wire-id value) :items []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})
         raw (or (:items opts) (:options opts) [])
         opts (with-id-callbacks (dissoc opts :items :options) raw [:on-change])]
     (merge-widget {:type :stepper
                    :value (wire-id value)
                    :items (option-items raw)}
                   opts))))

(defn pagination
  "Page navigation. `page` is 1-based (Kit default 1). `:total` is the
  page count (Kit default 1; Kit clamps both to ≥1). `:on-change`
  receives the new page number. `:compact true` is prev/next only.
  `:visible-pages` is the max numbered buttons (Kit default 5).

  (ui/pagination 3 {:total 10 :on-change set-page!})
  (ui/pagination 3 {:total 10 :compact true})"
  ([page]
   {:type :pagination :value (or page 1)})
  ([page on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :pagination :value (or page 1)} on-change-or-opts)
     {:type :pagination :value (or page 1) :on-change on-change-or-opts}))
  ([page on-change opts]
   (merge-widget {:type :pagination :value (or page 1) :on-change on-change}
                 opts)))

(defn otp-input
  "Fixed-length digit cells. `:on-change` fires when every cell is
  filled (crate complete-only). `:count` defaults to 6 (clamped 1–12).
  `:masked true` hides digits. `:on-blur` receives the current string.

  (ui/otp-input code {:count 6 :masked true :on-change set!})"
  ([value]
   {:type :otp-input :value (str (or value "")) :text (str (or value ""))})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :otp-input
                    :value (str (or value ""))
                    :text (str (or value ""))}
                   on-change-or-opts)
     {:type :otp-input
      :value (str (or value ""))
      :text (str (or value ""))
      :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge-widget {:type :otp-input
                  :value (str (or value ""))
                  :text (str (or value ""))
                  :on-change on-change}
                 opts)))

(defn color-picker
  "Hex color (`\"#3366ff\"`). `on-change` receives a hex string or `nil`.

  (ui/color-picker \"#3366ff\" {:on-change set!})"
  ([value]
   {:type :color-picker :value value})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :color-picker :value value} on-change-or-opts)
     {:type :color-picker :value value :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge-widget {:type :color-picker :value value :on-change on-change}
                 opts)))

(defn date-picker
  "ISO date `\"YYYY-MM-DD\"`. `:range true` (or `:multiple`) uses
  `[start end]` (missing bounds are JSON `null`). `on-change` receives
  that same JSON shape. Display format is `%Y-%m-%d`.

  (ui/date-picker \"2026-09-02\" {:on-change set!})
  (ui/date-picker [\"2026-01-01\" \"2026-01-31\"] {:range true})"
  ([value]
   {:type :date-picker :value value})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :date-picker :value value} on-change-or-opts)
     {:type :date-picker :value value :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge-widget {:type :date-picker :value value :on-change on-change}
                 opts)))

(defn editor
  "Code editor wrapping Kit `Editor` / `EditorState`. Not an LSP
  editor. `:language` is a highlighter name (`\"rust\"`, `\"json\"`,
  `\"markdown\"`; omitted is `\"text\"`). Kit's
  `tree-sitter-languages` bundle is enabled; there is no Clojure
  grammar, so `:language \"clojure\"` is plain text. `on-change`
  receives the string.

  (ui/editor src {:language \"rust\" :height 200 :on-change set!})"
  ([value]
   {:type :editor :text (str (or value ""))})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :editor :text (str (or value ""))} on-change-or-opts)
     {:type :editor :text (str (or value "")) :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge-widget {:type :editor :text (str (or value "")) :on-change on-change}
                 opts)))

(defn virtual-list
  "Variable-height virtualized rows `{id, label, height?}`. Default
  row height is 36px. Rows stack vertically unless `:orientation
  :horizontal`. `:selected` / `on-change` restore original ids.

  (ui/virtual-list items {:selected id :on-change set! :height 200})"
  ([items]
   (virtual-list items nil))
  ([items opts]
   (let [raw (or items [])
         opts (-> (or opts {})
                  rewrite-selected
                  apply-control-size)
         selected (:value opts)
         opts (with-option-callback (dissoc opts :items :options :value) raw)]
     (merge-widget {:type :virtual-list
                    :value (wire-id selected)
                    :items (option-items raw)}
                   opts))))

(defn- chart-opts
  "Stringify Kit alignment keys and normalize `:links` / `:series` items."
  [opts]
  (let [opts (or opts {})]
    (cond-> opts
      (keyword? (:alignment opts)) (update :alignment name)
      (keyword? (:node-align opts)) (update :node-align name)
      (keyword? (:value-scale opts)) (update :value-scale name)
      (keyword? (:stroke-style opts)) (update :stroke-style name)
      (keyword? (:fill-gradient opts)) (update :fill-gradient name)
      (keyword? (:fill-gradient-mode opts)) (update :fill-gradient-mode name)
      (seq (:links opts)) (update :links option-items)
      (seq (:series opts)) (update :series option-items))))

(defn chart
  "Series chart. `kind` is `:line` (default), `:bar`, `:area`, `:pie`,
  `:radar`, `:candlestick`, or `:sankey`.

  Convenience helpers (`horizontal-bar-chart`, `radar-chart`, …) stay;
  `ui/chart` itself does not hide Kit 0.6 builders. Points are
  `{id, label, value}` maps (`:values` for multi-series area/radar).
  Bar charts take Kit `:alignment` (`:bottom` default, `:left` for
  horizontal bars growing right). `:labels true` paints values on bars
  or pie slice labels. Radar dimensions may use `:values [a b]` (or
  `:value [a b]`) with `:series` names/colors/fills. Candlesticks use
  `:open` / `:high` / `:low` / `:close`. Sankey nodes are `points`;
  flows are `:links [{:source :target :value}]`.

  Kit-named options include `:name`, `:stroke`, `:stroke-style`
  (`:natural` / `:linear` / `:step-after`), `:dot`, `:tick-margin`
  (clamped to ≥1 on the host), `:x-axis`, `:grid`, `:corner-radii`,
  `:fill-gradient` (stop `at` is unclamped), `:inner-radius` (donut),
  `:outer-radius`, `:pad-angle`, `:label-color`, `:label-gap`,
  `:grid-levels`, `:body-width-ratio` (unclamped), `:interactive`
  (Kit hover tooltip; default off), and Sankey `:node-width` /
  `:node-padding` / `:iterations` / `:node-corner-radius` /
  `:link-opacity` / `:min-link-width` / `:label-lines`. Pie slices may
  set per-item `:inner-radius` / `:outer-radius` and `:color` (Kit
  `chart_2` when omitted). Radar `:content` is any clj-gpui widget.

  (ui/chart :line [{:id :a :label \"A\" :value 10}] {:height 180})
  (ui/chart :bar dirs {:alignment :left :labels true :value-axis true})"
  ([kind points]
   (chart kind points nil))
  ([kind points opts]
   (merge-widget {:type :chart
                  :variant (if (keyword? kind) (name kind) (str (or kind "line")))
                  :items (option-items (or points []))}
                 (chart-opts opts))))

(defn line-chart
  "See `chart` with `:line`. Kit default has no dots; pass `:dot true` to show them."
  ([points] (chart :line points nil))
  ([points opts] (chart :line points opts)))

(defn bar-chart
  "See `chart` with `:bar`."
  ([points] (chart :bar points nil))
  ([points opts] (chart :bar points opts)))

(defn horizontal-bar-chart
  "Bar chart with Kit `:alignment :left` (bars grow right).

  Same points as `bar-chart`. Intended for category lists such as
  directory sizes in cljdu.

  (ui/horizontal-bar-chart [{:id :src :label \"src\" :value 412}]
                           {:labels true :value-axis true})"
  ([points] (chart :bar points {:alignment :left}))
  ([points opts] (chart :bar points (merge {:alignment :left} opts))))

(defn area-chart
  "See `chart` with `:area`. Multiple `:values` plus `:series` overlay Kit `y()` series."
  ([points] (chart :area points nil))
  ([points opts] (chart :area points opts)))

(defn pie-chart
  "See `chart` with `:pie`. `:inner-radius` makes a donut; `:labels true` draws slice labels.
  Per-slice `:inner-radius` / `:outer-radius` map to Kit `inner_radius_fn` / `outer_radius_fn`.
  Omit slice `:color` to keep Kit `chart_2`. Omitted `:outer-radius` uses the chart height × 0.4 so the ring paints (Kit's layout default; Kit's paint path does not)."
  ([points] (chart :pie points nil))
  ([points opts] (chart :pie points opts)))

(defn radar-chart
  "See `chart` with `:radar`. Dimension `:content` is a Kit `RadarLabel::Element`
  (badge, avatar, and other clj-gpui widgets, not only the static overlay subset)."
  ([points] (chart :radar points nil))
  ([points opts] (chart :radar points opts)))

(defn candlestick-chart
  "See `chart` with `:candlestick`. Points need `:open` `:high` `:low` `:close`.
  `:body-width-ratio` and `:x-axis` map to Kit (ratio is not clamped)."
  ([points] (chart :candlestick points nil))
  ([points opts] (chart :candlestick points opts)))

(defn sankey-chart
  "See `chart` with `:sankey`. `points` are nodes; pass `:links` in `opts`.
  Node `:label-lines` is Kit custom `SankeyLabel`s."
  ([points] (chart :sankey points nil))
  ([points opts] (chart :sankey points opts)))

(defn markdown
  "Selectable markdown `TextView`. `:height` or `:flex 1` makes it scroll.

  (ui/markdown \"# Hello\")"
  ([text]
   {:type :markdown :text (str (or text ""))})
  ([text opts]
   (merge {:type :markdown :text (str (or text ""))} (or opts {}))))

(defn html
  "Selectable HTML `TextView`. Same layout notes as `markdown`.

  (ui/html \"<p>Hi</p>\")"
  ([text]
   {:type :html :text (str (or text ""))})
  ([text opts]
   (merge {:type :html :text (str (or text ""))} (or opts {}))))

(defn sidebar
  "App sidebar of `{id, label, icon?}` rows. `:side` is `:left` (default)
  or `:right`. `:collapsed` shrinks chrome. `:selected` / `on-change`
  restore original ids. `:title` is a header string.

  (ui/sidebar items {:selected id :side :left :on-change set!})"
  ([items]
   (sidebar items nil))
  ([items opts]
   (let [raw (or items [])
         opts (-> (or opts {})
                  (dissoc :items)
                  rewrite-selected
                  apply-control-size)
         selected (:value opts)
         opts (with-option-callback (dissoc opts :value) raw)]
     (merge-widget {:type :sidebar
                    :value (wire-id selected)
                    :items (option-items raw)}
                   opts))))

(defn settings
  "Settings pages. Each page is `{id, label, items}` where `items` are
  fields or groups (`{:label \"Alerts\" :items [fields]}`). Field
  `:variant` is `:switch`, `:checkbox`, `:number`, `:dropdown`,
  `:select`, or `:input`. A dropdown's option `:items` do **not** make
  it a group — set `:variant :dropdown` (or `:select`). Groups are
  wrappers without a field variant. `:on-change` receives
  `{:id field-id :value …}` with original field ids and dropdown
  option ids.

  (ui/settings pages {:on-change (fn [{:keys [id value]}])})"
  ([pages]
   (settings pages nil))
  ([pages opts]
   (let [raw (or pages [])
         opts (or opts {})
         opts (assoc opts :on-change (wrap-settings-callback (:on-change opts) raw))]
     (merge-widget {:type :settings :items (option-items raw)}
                   (dissoc opts :items :pages)))))

(defn dock
  "Dock area. Items are `{id, label, side, content}` maps. `:side` is
  `:left`, `:right`, `:bottom`, or `:center` (default). Panel bodies
  are the static overlay subset (label / button / stack / separator)
  plus `markdown` and `chart` — not list/data-table/editor.

  (ui/dock {:items [{:id :files :side :left :label \"Files\"
                     :content (ui/markdown \"…\")}]})"
  ([opts]
   (let [opts (if (map? opts) opts {})
         raw (or (:items opts) [])]
     (merge-widget {:type :dock :items (option-items raw)}
                   (dissoc opts :items)))))

(defn resizable
  "Split panes. `:orientation` is `:horizontal` (default) or
  `:vertical`. Child `:width` / `:height` / `:size` is the initial
  panel size in px. `:on-change` receives a vector of px sizes.

  (ui/resizable {:orientation :horizontal :on-change (fn [sizes])}
    child1 child2)"
  [opts-or-child & children]
  (let [[opts kids]
        (if (and (map? opts-or-child) (not (ui-node? opts-or-child)))
          [opts-or-child children]
          [{} (cons opts-or-child children)])]
    (merge-widget {:type :resizable
                   :children (flatten-children kids)}
                  (apply-control-size opts))))
