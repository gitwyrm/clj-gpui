(ns gpui.ui
  "Public widget constructors for clj-gpui.

  These functions return ordinary Clojure maps. The native host
  translates that data into GPUI elements. Application logic stays
  in Clojure: atoms, functions, sequences, macros, and namespaces
  are the real Clojure runtime."
  (:require [clojure.string :as str])
  (:refer-clojure :exclude []))

(def protocol-version
  "Version of the Clojure↔host UI-tree protocol. Bump when the schema changes."
  4)

(def window-title
  "Default native window title when `ui/window` omits `:title`."
  "clj-gpui")

(def named-themes
  "gpui-component palette names the host ships (plus Default Light/Dark).

  Use the display string (`\"Tokyo Night\"`) or a kebab/underscore spelling
  (`:tokyo-night`) as `:theme`. See https://longbridge.github.io/gpui-component/docs/theme"
  ["Adventure"
   "Adventure Time"
   "Alduin"
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
   "Matrix"
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

(defn option-item
  "Normalize a select/radio/tab/breadcrumb/accordion item to a map.

  Strings and keywords become `{:id … :label …}`. Maps keep `:id`,
  `:label` / `:text`, `:disabled`, `:on-click`, and `:content`."
  [x]
  (cond
    (nil? x) nil
    (map? x)
    (let [id (or (:id x) (:value x) (:label x) (:text x))
          label (or (:label x) (:text x) (:id x) (:value x))
          value (:value x)
          content (:content x)]
      (cond-> {:id (wire-id id)
               :label (when (some? label) (str label))}
        (some? value) (assoc :text (str value))
        (contains? x :text) (assoc :text (str (:text x)))
        (true? (:disabled x)) (assoc :disabled true)
        (fn? (:on-click x)) (assoc :on-click (:on-click x))
        (ui-node? content) (assoc :content content)
        (and (some? content) (not (ui-node? content)))
        (assoc :content (first (flatten-children [content])))))
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
  Appearance is `:system` (follow the OS), `:light`, or `:dark`. A named
  gpui-component palette is a string such as `\"Tokyo Night\"` (kebab
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
  A named gpui-component palette (`\"Tokyo Night\"`, `:ayu-light`) is also
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
  "Scroll container. Fills leftover height in a flex parent.

  Give it a `:height` (pixels) for a fixed viewport instead. `:flex 1`
  is implied when `:height` is omitted; passing it is still fine.
  `:width` constrains the viewport (not the overflowing content).
  `:size` is a square viewport, same as on other nodes."
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
  "Button-style toggle (gpui-component Toggle), distinct from `switch`.

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

  (ui/radio-group selected
    {:options [{:id :light :label \"Light\"} {:id :dark :label \"Dark\"}]
     :on-change #(swap! !state assoc :mode %)
     :orientation :horizontal})"
  ([value]
   {:type :radio-group :value (wire-id value) :options []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})
         options (option-items (or (:options opts) (:items opts)))]
     (merge-widget {:type :radio-group
                    :value (wire-id value)
                    :options options}
                   (dissoc opts :options :items)))))

(defn slider
  "Numeric slider. `on-change` receives a number.

  (ui/slider volume {:min 0 :max 100 :on-change #(swap! !state assoc :vol %)})"
  ([value]
   {:type :slider :value (or value 0)})
  ([value on-change-or-opts]
   (if (map? on-change-or-opts)
     (merge-widget {:type :slider :value (or value 0)} on-change-or-opts)
     {:type :slider :value (or value 0) :on-change on-change-or-opts}))
  ([value on-change opts]
   (merge-widget {:type :slider :value (or value 0) :on-change on-change}
                 opts)))

(defn progress
  "Determinate progress bar. `value` is 0–100.

  (ui/progress 45)
  (ui/progress 45 {:width 240})"
  ([value]
   {:type :progress :value (or value 0)})
  ([value opts]
   (merge {:type :progress :value (or value 0)} opts)))

(defn divider
  "Horizontal (default) or vertical rule. Optional label.

  (ui/divider)
  (ui/divider \"or\")
  (ui/divider {:orientation :vertical})
  (ui/divider \"or\" {:dashed true})"
  ([]
   {:type :divider})
  ([label-or-opts]
   (cond
     (map? label-or-opts) (merge {:type :divider} label-or-opts)
     (nil? label-or-opts) {:type :divider}
     :else {:type :divider :text (str label-or-opts)}))
  ([label opts]
   (merge {:type :divider :text (str label)} opts)))

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

  (ui/tabs selected
    {:items [{:id :general :label \"General\"}
             {:id :advanced :label \"Advanced\"}]
     :on-change #(swap! !state assoc :tab %)
     :variant :underline})"
  ([value]
   {:type :tabs :value (wire-id value) :items []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})]
     (merge-widget {:type :tabs
                    :value (wire-id value)
                    :items (option-items (or (:items opts) (:options opts)))}
                   (dissoc opts :items :options)))))

(defn select
  "Dropdown select. `value` is the selected option id; `on-change` receives that id.

  (ui/select selected
    {:options [{:id :clj :label \"Clojure\"} {:id :rs :label \"Rust\"}]
     :placeholder \"Language\"
     :on-change #(swap! !state assoc :lang %)})"
  ([value]
   {:type :select :value (wire-id value) :options []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})]
     (merge-widget {:type :select
                    :value (wire-id value)
                    :options (option-items (or (:options opts) (:items opts)))}
                   (dissoc opts :options :items)))))

(defn icon
  "Bundled gpui-component icon. `name` is a kebab keyword such as `:check`.

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
  Group `:on-change` receives the clicked item id.

  (ui/breadcrumb [{:id :home :label \"Home\" :on-click go-home!}
                  {:label \"Project\"}])"
  ([items]
   {:type :breadcrumb :items (option-items items)})
  ([items opts]
   (merge {:type :breadcrumb :items (option-items items)} opts)))

(defn avatar
  "Initials avatar from a display name.

  (ui/avatar {:name \"Ada Lovelace\"})
  (ui/avatar \"Ada Lovelace\")"
  ([name-or-opts]
   (if (map? name-or-opts)
     (let [opts name-or-opts
           name (or (:name opts) (:text opts))]
       (merge-widget {:type :avatar :text (when (some? name) (str name))}
                     (dissoc opts :name :text)))
     {:type :avatar :text (str name-or-opts)}))
  ([name opts]
   (merge-widget {:type :avatar :text (str name)} opts)))

(defn accordion
  "Exclusive accordion. `value` is the open item id (`nil` when all closed).
  `on-change` receives that id, or a vector of ids when `:multiple true`.

  (ui/accordion open-id
    {:on-change set-open!
     :items [{:id :a :title \"One\" :content (ui/label \"…\")}
             {:id :b :title \"Two\" :content (ui/label \"…\")}]})"
  ([value]
   {:type :accordion :value (wire-id value) :items []})
  ([value opts]
   (let [opts (if (map? opts) opts {:on-change opts})
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
                     (or (:items opts) []))]
     (merge-widget {:type :accordion
                    :value (if (sequential? value)
                             (str/join "," (map wire-id value))
                             (wire-id value))
                    :items items}
                   (dissoc opts :items)))))

(defn description-list
  "Key/value description list.

  (ui/description-list [{:label \"Name\" :value \"Ada\"}
                        {:label \"Lang\" :value \"Clojure\"}])"
  ([items]
   {:type :description-list
    :items (mapv (fn [item]
                   (if (map? item)
                     {:id (str (or (:label item) ""))
                      :label (str (or (:label item) ""))
                      :text (str (or (:value item) (:text item) ""))}
                     {:id (str item) :label (str item) :text ""}))
                 items)})
  ([items opts]
   (merge {:type :description-list
           :items (:items (description-list items))}
          opts)))
