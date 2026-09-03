(ns gpui.ui-test
  (:require [clojure.data.json :as json]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [clojure.test :refer [deftest is testing]]
            [gpui.runtime :as runtime]
            [gpui.ui :as ui]))

(deftest kit-06-names-are-not-aliased
  (is (nil? (ns-resolve 'gpui.ui 'text-field)))
  (is (nil? (ns-resolve 'gpui.ui 'divider)))
  (is (some? (ns-resolve 'gpui.ui 'table)))
  (is (some? (ns-resolve 'gpui.ui 'input)))
  (is (some? (ns-resolve 'gpui.ui 'separator)))
  (is (some? (ns-resolve 'gpui.ui 'data-table)))
  (is (some? (ns-resolve 'gpui.ui 'textarea)))
  (is (some? (ns-resolve 'gpui.ui 'alert-dialog)))
  (is (some? (ns-resolve 'gpui.ui 'combobox)))
  (is (some? (ns-resolve 'gpui.ui 'rating)))
  (is (some? (ns-resolve 'gpui.ui 'stepper)))
  (is (some? (ns-resolve 'gpui.ui 'table-header)))
  (is (some? (ns-resolve 'gpui.ui 'table-body)))
  (is (some? (ns-resolve 'gpui.ui 'table-footer)))
  (is (some? (ns-resolve 'gpui.ui 'table-row)))
  (is (some? (ns-resolve 'gpui.ui 'table-head)))
  (is (some? (ns-resolve 'gpui.ui 'table-cell)))
  (is (some? (ns-resolve 'gpui.ui 'table-caption))))

(deftest window-title
  (is (= "clj-gpui" ui/window-title)))

(deftest named-themes-match-vendored-json
  (let [dir (io/file "host/themes")
        from-json (->> (or (.listFiles dir) (into-array java.io.File []))
                       (filter #(str/ends-with? (.getName ^java.io.File %) ".json"))
                       (mapcat (fn [f]
                                 (get (json/read-str (slurp f)) "themes")))
                       (map #(get % "name"))
                       set)]
    (is (seq from-json) "expected vendored gpui-component theme JSON under host/themes")
    (is (= from-json (set (remove #{"Default Light" "Default Dark"} ui/named-themes))))
    (is (some #{"Tokyo Night"} ui/named-themes))
    (is (some #{"Default Light"} ui/named-themes))
    (is (= ["system" "light" "dark"] (take 3 ui/themes)))))

(deftest primitive-nodes
  (testing "label"
    (is (= {:type :label :text "Hi"}
           (ui/label "Hi")))
    (is (= :bold (:font-weight (ui/label "Hi" {:font-weight :bold})))))
  (testing "button sanitizes on-click"
    (let [n (ui/button "+" (fn [] :ok))]
      (is (= :button (:type n)))
      (is (fn? (:on-click n)))
      (is (= "+" (:text n)))))
  (testing "checkbox"
    (let [n (ui/checkbox true (fn []) "Done")]
      (is (true? (:checked n)))
      (is (= "Done" (:text n)))
      (is (fn? (:on-click n)))))
  (testing "layouts"
    (is (= :vstack (:type (ui/vstack {} (ui/label "a")))))
    (is (= :hstack (:type (ui/hstack {}))))
    (is (= :scroll (:type (ui/scroll {} (ui/label "x"))))))
  (testing "input"
    (is (= {:type :input :text "hi"}
           (ui/input "hi")))
    (let [n (ui/input "x" {:placeholder "Todo" :id "new-todo"})]
      (is (= :input (:type n)))
      (is (= "Todo" (:placeholder n)))
      (is (= "new-todo" (:id n))))
    (is (fn? (:on-change (ui/input "" (fn [s] s)))))
    (is (= {:type :textarea :text "notes"}
           (ui/textarea "notes")))
    (is (= 6 (:rows (ui/textarea "x" {:rows 6})))))
  (testing "style keys pass through"
    (is (true? (:strikethrough (ui/label "x" {:strikethrough true}))))
    (is (= :ghost (:variant (ui/button "All" (fn []) {:variant :ghost}))))
    (is (= :circle (:shape (ui/checkbox false (fn []) {:shape :circle}))))
    (is (fn? (:on-double-click (ui/label "x" {:on-double-click (fn [])}))))
    (is (fn? (:on-click (ui/label "row" {:on-click (fn [])}))))
    (is (fn? (:on-click (ui/hstack {:on-click (fn [])}))))
    (is (true? (:focus (ui/input "" {:focus true}))))))

(deftest named-control-size-does-not-use-pixel-size
  (let [n (ui/spinner {:size :small})]
    (is (= "small" (:control-size n)))
    (is (nil? (:size n))))
  (let [n (ui/icon :check {:size :large})]
    (is (= "large" (:control-size n)))
    (is (= "check" (:icon n)))))

(deftest option-item-normalization
  (is (= {:id "light" :label "light"} (ui/option-item :light)))
  (is (= {:id "Rust" :label "Rust"} (ui/option-item "Rust")))
  (is (= {:id "clj" :label "Clojure"}
         (ui/option-item {:id :clj :label "Clojure"})))
  (is (= 10 (:value (ui/option-item {:id :a :label "A" :value 10}))))
  (is (true? (:checked (ui/option-item {:id :notify :label "N" :checked true}))))
  (is (= "left" (:side (ui/option-item {:id :files :side :left :label "Files"}))))
  (is (= ["a" "b"] (mapv :id (ui/option-items [:a nil :b]))))
  (is (nil? (ui/option-item nil)))
  (is (= "ui/dark" (ui/wire-id :ui/dark)))
  (is (= "light" (ui/wire-id :light))))

(deftest option-ids-preserve-original-clojure-identity
  (is (= :dark (ui/option-identity :dark)))
  (is (= :dark (ui/option-identity {:id :dark :label "Dark"})))
  (is (= "custom-id" (ui/option-identity {:id "custom-id"})))
  (is (= :dark (ui/resolve-option-id (ui/option-id-map [:dark :light]) "dark")))
  (is (= "custom-id" (ui/resolve-option-id (ui/option-id-map ["custom-id"]) "custom-id")))
  (is (= :ui/dark (ui/resolve-option-id (ui/option-id-map [:ui/dark]) "ui/dark")))
  (is (= [:a :b] (ui/resolve-option-id (ui/option-id-map [:a :b]) ["a" "b"])))
  (is (nil? (ui/resolve-option-id (ui/option-id-map [:a]) nil)))
  (testing "first option wins when wire ids collide"
    (is (= :dark (ui/resolve-option-id (ui/option-id-map [:dark "dark"]) "dark")))
    (is (= "dark" (ui/resolve-option-id (ui/option-id-map ["dark" :dark]) "dark")))))

(deftest switch-slider-select-nodes
  (testing "switch"
    (let [n (ui/switch true {:on-change (fn [_]) :text "On"})]
      (is (= :switch (:type n)))
      (is (true? (:checked n)))
      (is (= "On" (:text n)))
      (is (fn? (:on-change n))))
    (is (fn? (:on-change (ui/switch false (fn [_]))))))
  (testing "slider defaults"
    (let [n (ui/slider 40 {:min 0 :max 100 :step 5})]
      (is (= :slider (:type n)))
      (is (= 40 (:value n)))
      (is (= 0 (:min n)))
      (is (= 100 (:max n)))
      (is (= 5 (:step n))))
    (is (= 42 (:value (ui/slider 42 {:min 0 :max 100 :step 5})))))
  (testing "select options"
    (let [n (ui/select :clj {:options [{:id :clj :label "Clojure"} "Rust"]
                             :placeholder "Lang"})]
      (is (= :select (:type n)))
      (is (= "clj" (:value n)))
      (is (= [{:id "clj" :label "Clojure"} {:id "Rust" :label "Rust"}]
             (:options n)))
      (is (= "Lang" (:placeholder n)))))
  (testing "radio-group"
    (let [n (ui/radio-group :dark {:options [:light :dark]
                                   :orientation :horizontal})]
      (is (= :radio-group (:type n)))
      (is (= "dark" (:value n)))
      (is (= :horizontal (:orientation n)))
      (is (= ["light" "dark"] (mapv :id (:options n))))))
  (testing "tabs"
    (let [n (ui/tabs :advanced {:items [{:id :general :label "General"}
                                        {:id :advanced :label "Advanced"}]
                                :variant :underline})]
      (is (= :tabs (:type n)))
      (is (= "advanced" (:value n)))
      (is (= :underline (:variant n)))))
  (testing "progress clamps in host; Clojure passes the number"
    (is (= 45 (:value (ui/progress 45))))
    (is (= 0 (:value (ui/progress nil)))))
  (testing "separator"
    (is (= :separator (:type (ui/separator))))
    (is (= "or" (:text (ui/separator "or"))))
    (is (true? (:dashed (ui/separator {:dashed true})))))
  (testing "tag alert kbd link"
    (is (= :danger (:variant (ui/tag "Err" {:variant :danger}))))
    (is (= "Saved" (:text (ui/alert "Saved" {:variant :success}))))
    (is (= "ctrl-s" (:text (ui/kbd "ctrl-s"))))
    (is (= "https://clojure.org" (:href (ui/link "https://clojure.org" "Clojure"))))
    (is (= "Clojure" (:text (ui/link "https://clojure.org" "Clojure")))))
  (testing "badge wraps children"
    (let [n (ui/badge 4 (ui/icon :bell))]
      (is (= 4 (:count n)))
      (is (= :icon (get-in n [:children 0 :type]))))
    (is (true? (:dot (ui/badge {:dot true} (ui/label "x"))))))
  (testing "group-box flattens children"
    (let [n (ui/group-box {:title "Audio" :variant :outline}
                          (ui/label "a")
                          [(ui/label "b")])]
      (is (= :group-box (:type n)))
      (is (= "Audio" (:title n)))
      (is (= 2 (count (:children n))))))
  (testing "breadcrumb clipboard avatar"
    (is (= ["Home" "Proj"]
           (mapv :label (:items (ui/breadcrumb [{:id :home :label "Home"} "Proj"])))))
    (is (= "copy-me" (:text (ui/clipboard "copy-me"))))
    (is (= "Ada" (:text (ui/avatar "Ada")))))
  (testing "accordion content stays a node"
    (let [n (ui/accordion :a {:items [{:id :a :title "One" :content (ui/label "hi")}
                                      {:id :b :title "Two" :content (ui/label "there")}]})]
      (is (= "a" (:value n)))
      (is (= "One" (get-in n [:items 0 :label])))
      (is (= :label (get-in n [:items 0 :content :type]))))
    (is (= ["a" "b"] (:value (ui/accordion [:a :b]
                                           {:multiple true
                                            :items [{:id :a :title "A" :content (ui/label "x")}
                                                    {:id :b :title "B" :content (ui/label "y")}]}))))
    (is (= ["audio,advanced"]
           (:value (ui/accordion ["audio,advanced"]
                                 {:multiple true
                                  :items [{:id "audio,advanced"
                                           :title "Mixed"
                                           :content (ui/label "x")}]})))))
  (testing "accordion multiple keeps constructor item order on the node"
    (let [n (ui/accordion [:display :audio]
                          {:multiple true
                           :items [{:id :audio :title "Audio" :content (ui/label "a")}
                                   {:id :display :title "Display" :content (ui/label "b")}]})]
      (is (= ["display" "audio"] (:value n)))
      (is (= ["audio" "display"] (mapv :id (:items n))))))
  (testing "description-list"
    (let [n (ui/description-list [{:label "Host" :value "GPUI"}
                                  {:label "UI" :value "clj-gpui"}])]
      (is (= :description-list (:type n)))
      (is (= :vertical (:orientation n)))
      (is (= ["Host" "UI"] (mapv :label (:items n))))
      (is (= ["GPUI" "clj-gpui"] (mapv :text (:items n)))))
    (let [h (ui/description-list [{:label "A" :value "1" :span 2}]
                                 {:orientation :horizontal :columns 3})]
      (is (= :horizontal (:orientation h)))
      (is (= 3 (:columns h)))
      (is (= 2 (get-in h [:items 0 :span]))))))

(deftest export-new-widget-callbacks
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/vstack
                   (ui/switch true {:on-change #(reset! got %)})
                   (ui/slider 10 {:on-change #(reset! got %)})
                   (ui/select "a" {:options ["a" "b"]
                                   :on-change #(reset! got %)})
                   (ui/alert "x" {:on-close #(reset! got :closed)})
                   (ui/clipboard "z" {:on-copied #(reset! got %)})))
        children (:children exported)
        switch-id (get-in children [0 :on-change])
        slider-id (get-in children [1 :on-change])
        select-id (get-in children [2 :on-change])
        close-id (get-in children [3 :on-close])
        copied-id (get-in children [4 :on-copied])]
    (is (string? switch-id))
    (is (= {:ok true :id switch-id} (runtime/invoke-callback! switch-id true)))
    (is (true? @got))
    (is (= {:ok true :id slider-id} (runtime/invoke-callback! slider-id 33.5)))
    (is (= 33.5 @got))
    (is (= {:ok true :id select-id} (runtime/invoke-callback! select-id "b")))
    (is (= "b" @got))
    (is (string? close-id))
    (is (= {:ok true :id close-id} (runtime/invoke-callback! close-id)))
    (is (= :closed @got))
    (is (string? copied-id))
    (is (= {:ok true :id copied-id} (runtime/invoke-callback! copied-id "z")))
    (is (= "z" @got))))

(deftest option-callbacks-restore-original-clojure-ids
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/vstack
                   (ui/tabs :general
                            {:items [{:id :general :label "General"}
                                     {:id :chrome :label "Chrome"}]
                             :on-change #(reset! got %)})
                   (ui/radio-group :light
                                   {:options [{:id :light :label "Light"}
                                              {:id :dark :label "Dark"}]
                                    :on-change #(reset! got %)})
                   (ui/select :clj
                              {:options [{:id :clj :label "Clojure"}
                                         {:id :rs :label "Rust"}]
                               :on-change #(reset! got %)})
                   (ui/breadcrumb [{:id :home :label "Home"} {:id :project :label "Project"}]
                                  {:on-change #(reset! got %)})
                   (ui/accordion :audio
                                 {:on-change #(reset! got %)
                                  :items [{:id :audio :title "Audio" :content (ui/label "a")}
                                          {:id :display :title "Display" :content (ui/label "b")}]})
                   (ui/select "custom-id"
                              {:options [{:id "custom-id" :label "Custom"}
                                         {:id "other" :label "Other"}]
                               :on-change #(reset! got %)})))
        children (:children exported)]
    (is (= "general" (get-in children [0 :value])))
    (is (= {:ok true :id (get-in children [0 :on-change])}
           (runtime/invoke-callback! (get-in children [0 :on-change]) "chrome")))
    (is (= :chrome @got))
    (runtime/invoke-callback! (get-in children [1 :on-change]) "dark")
    (is (= :dark @got))
    (runtime/invoke-callback! (get-in children [2 :on-change]) "rs")
    (is (= :rs @got))
    (runtime/invoke-callback! (get-in children [3 :on-change]) "home")
    (is (= :home @got))
    (runtime/invoke-callback! (get-in children [4 :on-change]) "display")
    (is (= :display @got))
    (runtime/invoke-callback! (get-in children [4 :on-change]) ["audio" "display"])
    (is (= [:audio :display] @got))
    (runtime/invoke-callback! (get-in children [5 :on-change]) "other")
    (is (= "other" @got))))

(deftest select-nil-value-and-searchable-stay-on-the-node
  (let [cleared (ui/select nil {:options [{:id :clj :label "Clojure"}]
                                :searchable true})]
    (is (nil? (:value cleared)))
    (is (true? (:searchable cleared))))
  (runtime/reset-callbacks!)
  (let [exported (runtime/export-tree
                  (ui/select nil {:options [{:id :clj :label "Clojure"}]
                                  :searchable true}))]
    (is (nil? (:value exported)))
    (is (true? (:searchable exported)))))

(deftest callback-json-socket-decoding
  (runtime/reset-callbacks!)
  (let [got (atom :unset)
        exported (runtime/export-tree
                  (ui/vstack
                   (ui/button "Go" #(reset! got :zero))
                   (ui/switch false {:on-change #(reset! got %)})
                   (ui/input "" {:on-change #(reset! got %)})))
        children (:children exported)
        zero-id (get-in children [0 :on-click])
        switch-id (get-in children [1 :on-change])
        field-id (get-in children [2 :on-change])
        through (fn [m]
                  (runtime/handle (json/read-str (json/write-str m) :key-fn keyword)))]
    (through {:op "callback" :id 1 :callback-id zero-id})
    (is (= :zero @got))
    (through {:op "callback" :id 2 :callback-id switch-id :value false})
    (is (false? @got))
    (through {:op "callback" :id 3 :callback-id switch-id :value 0})
    (is (zero? @got))
    (through {:op "callback" :id 4 :callback-id field-id :value ""})
    (is (= "" @got))
    (through {:op "callback" :id 5 :callback-id switch-id :value nil})
    (is (nil? @got))
    (through {:op "callback" :id 6 :callback-id switch-id :value ["a" "b"]})
    (is (= ["a" "b"] @got))
    (through {:op "callback" :id 7 :callback-id switch-id :value {:k 1}})
    (is (= {:k 1} @got))))

(deftest category-b-layout-keys-stay-on-the-node
  (is (= 24 (:width (ui/spinner {:width 24}))))
  (is (= 1 (:flex (ui/spinner {:flex 1}))))
  (is (= 32 (:size (ui/badge {:size 32 :dot true} (ui/icon :bell)))))
  (is (= 48 (:width (ui/clipboard "x" {:width 48}))))
  (let [acc (ui/accordion :audio
                          {:width 240 :height 80 :flex 1
                           :items [{:id :audio :title "Audio" :content (ui/label "a")}]})]
    (is (= 240 (:width acc)))
    (is (= 80 (:height acc)))
    (is (= 1 (:flex acc))))
  (let [dl (ui/description-list [{:label "Host" :value "GPUI"}]
                                {:width 300 :height 40})]
    (is (= 300 (:width dl)))
    (is (= 40 (:height dl)))))

(deftest invoke-callback-false-zero-and-null
  (runtime/reset-callbacks!)
  (let [got (atom :unset)
        exported (runtime/export-tree (ui/switch false {:on-change #(reset! got %)}))
        id (:on-change exported)]
    (is (= {:ok true :id id} (runtime/invoke-callback! id false true)))
    (is (false? @got))
    (is (= {:ok true :id id} (runtime/invoke-callback! id 0 true)))
    (is (zero? @got))
    (is (= {:ok true :id id} (runtime/invoke-callback! id nil true)))
    (is (nil? @got))))

(deftest scroll-viewport-style-keys
  (testing "flex scroll with no height"
    (let [n (ui/scroll {:flex 1} (ui/label "x"))]
      (is (= :scroll (:type n)))
      (is (= 1 (:flex n)))
      (is (nil? (:height n)))
      (is (nil? (:width n)))))
  (testing "fixed height"
    (let [n (ui/scroll {:height 220} (ui/label "x"))]
      (is (= 220 (:height n)))
      (is (nil? (:width n)))))
  (testing "explicit width"
    (let [n (ui/scroll {:width 300} (ui/label "x"))]
      (is (= 300 (:width n)))
      (is (nil? (:height n)))))
  (testing "explicit width and height"
    (let [n (ui/scroll {:width 300 :height 220} (ui/label "x"))]
      (is (= 300 (:width n)))
      (is (= 220 (:height n)))))
  (testing "size is a square, same as other nodes"
    (let [n (ui/scroll {:size 180 :width 300 :height 220} (ui/label "x"))]
      (is (= 180 (:size n)))
      (is (= 300 (:width n)))
      (is (= 220 (:height n)))))
  (testing "visual styles stay on the node for the inner body"
    (let [n (ui/scroll {:width 300 :padding 8 :bg "#111111"} (ui/label "x"))]
      (is (= 8 (:padding n)))
      (is (= "#111111" (:bg n))))))

(deftest sequences-flatten-inside-stacks
  (let [tree (ui/vstack
              (ui/label "Todos")
              (map ui/label ["Clojure" "Rust" "GPUI"])
              (when false (ui/label "hidden"))
              (when true (ui/label "visible")))]
    (is (= ["Todos" "Clojure" "Rust" "GPUI" "visible"]
           (mapv :text (:children tree))))))

(deftest export-tree-assigns-callback-ids
  (runtime/reset-callbacks!)
  (let [tree (ui/vstack
              {:gap 8}
              (ui/button "Go" (fn [] :fired)))
        exported (runtime/export-tree tree)]
    (is (= "vstack" (:type exported)))
    (is (string? (get-in exported [:children 0 :on-click])))
    (is (fn? (runtime/lookup-callback (get-in exported [:children 0 :on-click]))))))

(deftest theme-on-root-serializes
  (runtime/reset-callbacks!)
  (let [exported (runtime/export-tree
                  (ui/vstack {:theme :light :gap 8} (ui/label "x")))]
    (is (= "vstack" (:type exported)))
    (is (= "light" (:theme exported))))
  (let [exported (runtime/export-tree
                  (ui/vstack {:theme :dark} (ui/label "x")))]
    (is (= "dark" (:theme exported))))
  (let [exported (runtime/export-tree
                  (ui/vstack {:theme :system} (ui/label "x")))]
    (is (= "system" (:theme exported))))
  (let [exported (runtime/export-tree
                  (ui/vstack {:theme "Tokyo Night"} (ui/label "x")))]
    (is (= "Tokyo Night" (:theme exported))))
  (let [exported (runtime/export-tree
                  (ui/vstack {:theme :tokyo-night} (ui/label "x")))]
    (is (= "tokyo-night" (:theme exported)))))

(deftest window-maps-chrome-and-size
  (let [n (ui/window {:title "Todos"
                      :chrome :app
                      :width 640
                      :height 820
                      :theme :light}
                     (ui/label "x"))]
    (is (= :window (:type n)))
    (is (= "Todos" (:title n)))
    (is (= :app (:chrome n)))
    (is (= 640 (:window-width n)))
    (is (= 820 (:window-height n)))
    (is (= :light (:theme n)))
    (is (nil? (:width n)))
    (is (nil? (:height n)))))

(deftest window-chrome-serializes
  (runtime/reset-callbacks!)
  (let [exported (runtime/export-tree
                  (ui/window {:title "Todos"
                              :chrome :app
                              :width 640
                              :height 820}
                             (ui/vstack {:theme :dark} (ui/label "x"))))]
    (is (= "window" (:type exported)))
    (is (= "Todos" (:title exported)))
    (is (= "app" (:chrome exported)))
    (is (= 640 (:window-width exported)))
    (is (= 820 (:window-height exported)))
    (is (= "dark" (get-in exported [:children 0 :theme])))))

(deftest theme-on-nested-node-serializes
  (runtime/reset-callbacks!)
  (let [exported (runtime/export-tree
                  (ui/hstack
                   (ui/vstack {:theme :dark} (ui/label "nav"))
                   (ui/vstack {:theme :light} (ui/label "main"))))]
    (is (= "dark" (get-in exported [:children 0 :theme])))
    (is (= "light" (get-in exported [:children 1 :theme])))))

(deftest export-double-click-and-edit-callbacks
  (runtime/reset-callbacks!)
  (let [exported (runtime/export-tree
                  (ui/vstack
                   (ui/label "n" {:on-double-click (fn [])})
                   (ui/input "hi" {:on-blur (fn [_])
                                   :on-escape (fn [])})))]
    (is (string? (get-in exported [:children 0 :on-double-click])))
    (is (fn? (runtime/lookup-callback (get-in exported [:children 0 :on-double-click]))))
    (is (string? (get-in exported [:children 1 :on-blur])))
    (is (string? (get-in exported [:children 1 :on-escape])))))

(deftest invoke-callback-passes-text-value
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/input "" {:on-change #(reset! got %)
                                :on-submit #(reset! got (str "go:" %))}))
        change-id (:on-change exported)
        submit-id (:on-submit exported)]
    (is (string? change-id))
    (is (= {:ok true :id change-id} (runtime/invoke-callback! change-id "typed")))
    (is (= "typed" @got))
    (is (= {:ok true :id submit-id} (runtime/invoke-callback! submit-id "done")))
    (is (= "go:done" @got))))

(deftest overlay-and-row-constructors
  (testing "dialog rewrites :open? and keeps children"
    (let [n (ui/dialog true
                       {:title "Delete?" :variant :confirm :on-ok (fn []) :on-close (fn [])}
                       (ui/label "Undo?"))]
      (is (= :dialog (:type n)))
      (is (true? (:open n)))
      (is (nil? (:open? n)))
      (is (= "Delete?" (:title n)))
      (is (= :confirm (:variant n)))
      (is (= :label (get-in n [:children 0 :type])))))
  (testing "overlay-closable passes through"
    (let [n (ui/dialog true {:overlay-closable false} (ui/label "x"))]
      (is (false? (:overlay-closable n)))))
  (testing "dialog map-first form"
    (let [n (ui/dialog {:open? true :title "Hi"} (ui/label "x"))]
      (is (true? (:open n)))
      (is (= "Hi" (:title n)))))
  (testing "alert-dialog uses :alert-dialog"
    (let [n (ui/alert-dialog true
                             {:title "Delete?" :variant :confirm :on-ok (fn [])}
                             (ui/label "Undo?"))]
      (is (= :alert-dialog (:type n)))
      (is (true? (:open n)))
      (is (= "Delete?" (:title n)))))
  (testing "popover trigger and open"
    (let [n (ui/popover false
                        {:trigger (ui/button "More") :on-open-change (fn [_])}
                        (ui/label "Hint"))]
      (is (= :popover (:type n)))
      (is (false? (:open n)))
      (is (= :button (get-in n [:trigger :type])))
      (is (= "More" (get-in n [:trigger :text])))))
  (testing "dropdown and context menus nest and separate"
    (let [items [{:id :copy :label "Copy"} :- {:id :more :label "More"
                                               :items [{:id :paste :label "Paste"}]}]
          drop (ui/dropdown-menu items {:on-change (fn [_])} (ui/button "Edit"))
          ctx (ui/context-menu items (ui/label "Right-click"))]
      (is (= :dropdown-menu (:type drop)))
      (is (true? (get-in drop [:items 1 :separator])))
      (is (= "paste" (get-in drop [:items 2 :items 0 :id])))
      (is (= :button (get-in drop [:trigger :type])))
      (is (= :context-menu (:type ctx)))
      (is (= :label (get-in ctx [:children 0 :type])))))
  (testing "context-menu wraps a flex data-table"
    (let [tbl (ui/data-table {:columns [{:id :n :label "N"}]
                              :rows [{:id :a :cells ["A"]}]
                              :flex 1})
          ctx (ui/context-menu [{:id :copy :label "Copy"}] {:flex 1} tbl)]
      (is (= 1 (:flex ctx)))
      (is (= :data-table (get-in ctx [:children 0 :type])))
      (is (= 1 (get-in ctx [:children 0 :flex])))))
  (testing "list selected alias and searchable"
    (let [n (ui/list [{:id :alpha :label "Alpha"} :beta]
                     {:selected :alpha :searchable true :height 180})]
      (is (= :list (:type n)))
      (is (= "alpha" (:value n)))
      (is (true? (:searchable n)))
      (is (= 180 (:height n)))
      (is (nil? (:selected n)))
      (is (= ["alpha" "beta"] (mapv :id (:items n))))))
  (testing "data-table columns live in :options not :columns"
    (let [n (ui/data-table {:columns [{:id :name :label "Name" :width 120}
                                      {:id :lang :label "Lang"}]
                            :rows [{:id :ada :cells ["Ada" "Clojure"]}]
                            :selected :ada})]
      (is (= :data-table (:type n)))
      (is (= "ada" (:value n)))
      (is (nil? (:columns n)))
      (is (= "name" (get-in n [:options 0 :id])))
      (is (= 120 (get-in n [:options 0 :width])))
      (is (= ["Ada" "Clojure"] (get-in n [:items 0 :cells])))))
  (testing "declarative table shorthand expands to Kit primitives"
    (let [n (ui/table {:columns [{:label "Name" :span 2}
                                 {:label "Amount" :align :end :width 80}]
                       :rows [["Ada" "$250"] {:id :rich :cells ["Rich" "$150"]}]
                       :footer [{:span 2 :align :end :text "Total $400"}]
                       :caption "Invoices"})
          header-row (get-in n [:children 0 :children 0])
          body-row (get-in n [:children 1 :children 0])
          foot-row (get-in n [:children 2 :children 0])]
      (is (= :table (:type n)))
      (is (nil? (:options n)))
      (is (nil? (:items n)))
      (is (nil? (:text n)))
      (is (nil? (:caption n)))
      (is (nil? (:columns n)))
      (is (= :table-header (get-in n [:children 0 :type])))
      (is (= :table-body (get-in n [:children 1 :type])))
      (is (= :table-footer (get-in n [:children 2 :type])))
      (is (= :table-caption (get-in n [:children 3 :type])))
      (is (= :table-head (get-in header-row [:children 0 :type])))
      (is (= 2 (get-in header-row [:children 0 :span])))
      (is (= "end" (get-in header-row [:children 1 :align])))
      (is (= 80 (get-in header-row [:children 1 :width])))
      (is (= :table-cell (get-in body-row [:children 0 :type])))
      (is (nil? (get-in body-row [:children 0 :span]))
          "column :span must not copy onto every body cell")
      (is (= "end" (get-in body-row [:children 1 :align])))
      (is (= 80 (get-in body-row [:children 1 :width])))
      (is (= "Ada" (get-in body-row [:children 0 :children 0 :text])))
      (is (= "Invoices" (get-in n [:children 3 :children 0 :text])))
      (is (= 2 (get-in foot-row [:children 0 :span]))
          "footer cell span is independent of the body")
      (is (= "end" (get-in foot-row [:children 0 :align])))))
  (testing "declarative table primitives accept widget children"
    (let [n (ui/table
             (ui/table-header
              (ui/table-row
               (ui/table-head "Person")
               (ui/table-head {:align :end} "Role")))
             (ui/table-body
              (ui/table-row
               (ui/table-cell (ui/avatar "Ada") (ui/label "Lovelace"))
               (ui/table-cell {:align :end} (ui/badge 1 (ui/label "Math")))))
             (ui/table-footer
              (ui/table-row
               (ui/table-cell {:span 2 :align :end} "One pioneer")))
             (ui/table-caption "Staff"))]
      (is (= :table (:type n)))
      (is (= :avatar (get-in n [:children 1 :children 0 :children 0 :children 0 :type])))
      (is (= :badge (get-in n [:children 1 :children 0 :children 1 :children 0 :type])))
      (is (= 2 (get-in n [:children 2 :children 0 :children 0 :span])))
      (is (= "end" (get-in n [:children 2 :children 0 :children 0 :align])))
      (is (= "Staff" (get-in n [:children 3 :children 0 :text])))))
  (testing "combobox defaults searchable and restores ids"
    (let [n (ui/combobox :clj {:options [{:id :clj :label "Clojure"} :rs]})]
      (is (= :combobox (:type n)))
      (is (true? (:searchable n)))
      (is (= "clj" (:value n)))
      (is (= ["clj" "rs"] (mapv :id (:options n)))))
    (let [n (ui/combobox [:clj :rs]
                         {:options [{:id :clj :label "Clojure"} {:id :rs :label "Rust"}]
                          :multiple true
                          :searchable false})]
      (is (true? (:multiple n)))
      (is (false? (:searchable n)))
      (is (= ["clj" "rs"] (:value n)))))
  (testing "rating and stepper"
    (let [n (ui/rating 3 {:max 5})]
      (is (= :rating (:type n)))
      (is (= 3 (:value n)))
      (is (= 5 (:max n))))
    (let [n (ui/stepper :pay {:items [{:id :cart :label "Cart"} {:id :pay :label "Pay"}]})]
      (is (= :stepper (:type n)))
      (is (= "pay" (:value n)))
      (is (= ["cart" "pay"] (mapv :id (:items n))))))
  (testing "tree nested items and expanded"
    (let [n (ui/tree [{:id :src :label "src" :expanded true
                       :items [{:id :lib :label "lib.rs"}]}]
                     {:selected :lib})]
      (is (= :tree (:type n)))
      (is (= "lib" (:value n)))
      (is (true? (get-in n [:items 0 :expanded])))
      (is (= "lib" (get-in n [:items 0 :items 0 :id]))))))

(deftest product-widget-constructors
  (testing "sheet rewrites :open? and keeps footer"
    (let [n (ui/sheet true
                      {:title "Inspect" :placement :right
                       :footer (ui/button "Done" (fn []))}
                      (ui/label "Body"))]
      (is (= :sheet (:type n)))
      (is (true? (:open n)))
      (is (nil? (:open? n)))
      (is (= :right (:placement n)))
      (is (= :button (get-in n [:footer :type])))
      (is (= :label (get-in n [:children 0 :type])))))
  (testing "notification presence and variant"
    (let [n (ui/notification {:variant :success :title "Saved" :message "ok" :autohide false})]
      (is (= :notification (:type n)))
      (is (= :success (:variant n)))
      (is (false? (:autohide n)))
      (is (nil? (:open n)))))
  (testing "number otp color date editor"
    (let [n (ui/number-input 42 {:min 0 :max 100 :step 1})]
      (is (= :number-input (:type n)))
      (is (= 42 (:value n)))
      (is (= "42" (:text n))))
    (let [n (ui/otp-input "123" {:count 6 :masked true})]
      (is (= :otp-input (:type n)))
      (is (= "123" (:value n)))
      (is (true? (:masked n)))
      (is (= 6 (:count n))))
    (is (= "#3366ff" (:value (ui/color-picker "#3366ff"))))
    (let [n (ui/date-picker ["2026-01-01" "2026-01-31"] {:range true})]
      (is (= :date-picker (:type n)))
      (is (true? (:range n))))
    (is (= "rust" (:language (ui/editor "fn" {:language "rust"})))))
  (testing "virtual-list chart markdown chrome"
    (let [n (ui/virtual-list [{:id :a :label "A" :height 40}] {:selected :a :height 200})]
      (is (= :virtual-list (:type n)))
      (is (= "a" (:value n)))
      (is (= 40 (get-in n [:items 0 :height]))))
    (let [n (ui/chart :bar [{:id :a :label "A" :value 3.5}] {:height 180})]
      (is (= :chart (:type n)))
      (is (= "bar" (:variant n)))
      (is (= 3.5 (get-in n [:items 0 :value]))))
    (is (= :markdown (:type (ui/markdown "# Hi"))))
    (is (= :html (:type (ui/html "<p>x</p>"))))
    (let [n (ui/sidebar [{:id :home :label "Home" :icon :check}]
                        {:selected :home :collapsed true :side :right})]
      (is (= :sidebar (:type n)))
      (is (= "home" (:value n)))
      (is (true? (:collapsed n)))
      (is (= :right (:side n))))
    (let [n (ui/settings [{:id :general :label "General"
                           :items [{:id :notify :label "N" :checked true :variant :switch}]}]
                         {:on-change (fn [_])})]
      (is (= :settings (:type n)))
      (is (true? (get-in n [:items 0 :items 0 :checked])))
      (is (= "switch" (get-in n [:items 0 :items 0 :variant]))))
    (let [n (ui/dock {:items [{:id :files :side :left :label "Files"
                               :content (ui/markdown "hi")}]})]
      (is (= :dock (:type n)))
      (is (= "left" (get-in n [:items 0 :side])))
      (is (= :markdown (get-in n [:items 0 :content :type]))))
    (let [n (ui/resizable {:orientation :vertical}
                          (ui/label "a")
                          (ui/label "b"))]
      (is (= :resizable (:type n)))
      (is (= :vertical (:orientation n)))
      (is (= 2 (count (:children n)))))))

(deftest overlay-callbacks-sanitize-and-restore-ids
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/vstack
                   (ui/dialog true {:on-close #(reset! got :closed)
                                    :on-ok #(reset! got :ok)
                                    :on-open-change #(reset! got %)}
                              (ui/label "x"))
                   (ui/popover true {:on-open-change #(reset! got %)}
                               (ui/button "Go" #(reset! got :go)))
                   (ui/dropdown-menu [{:id :copy :label "Copy"}]
                                     {:on-change #(reset! got %)}
                                     (ui/button "Edit"))
                   (ui/list [{:id :alpha :label "Alpha"} {:id :beta :label "Beta"}]
                            {:on-change #(reset! got %)
                             :on-confirm #(reset! got [:confirm %])})
                   (ui/data-table {:columns [{:id :name :label "Name"}]
                                   :rows [{:id :ada :cells ["Ada"]}]
                                   :on-change #(reset! got %)})
                   (ui/tree [{:id :src :label "src"
                              :items [{:id :lib :label "lib.rs"}]}]
                            {:on-change #(reset! got %)})))
        children (:children exported)]
    (is (string? (get-in children [0 :on-close])))
    (is (string? (get-in children [0 :on-ok])))
    (is (string? (get-in children [1 :on-open-change])))
    (is (string? (get-in children [1 :children 0 :on-click])))
    (is (string? (get-in children [2 :on-change])))
    (is (string? (get-in children [2 :trigger :text])))
    (is (string? (get-in children [3 :on-confirm])))
    (is (= {:ok true :id (get-in children [0 :on-close])}
           (runtime/invoke-callback! (get-in children [0 :on-close]))))
    (is (= :closed @got))
    (is (= {:ok true :id (get-in children [1 :on-open-change])}
           (runtime/invoke-callback! (get-in children [1 :on-open-change]) false)))
    (is (false? @got))
    (is (= {:ok true :id (get-in children [2 :on-change])}
           (runtime/invoke-callback! (get-in children [2 :on-change]) "copy")))
    (is (= :copy @got))
    (is (= {:ok true :id (get-in children [3 :on-change])}
           (runtime/invoke-callback! (get-in children [3 :on-change]) "beta")))
    (is (= :beta @got))
    (is (= {:ok true :id (get-in children [3 :on-confirm])}
           (runtime/invoke-callback! (get-in children [3 :on-confirm]) "alpha")))
    (is (= [:confirm :alpha] @got))
    (is (= {:ok true :id (get-in children [4 :on-change])}
           (runtime/invoke-callback! (get-in children [4 :on-change]) "ada")))
    (is (= :ada @got))
    (is (= {:ok true :id (get-in children [5 :on-change])}
           (runtime/invoke-callback! (get-in children [5 :on-change]) "lib")))
    (is (= :lib @got))))

(deftest product-widget-callbacks-sanitize-and-restore-ids
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/vstack
                   (ui/sheet true {:on-close #(reset! got :sheet)
                                   :footer (ui/button "Done" #(reset! got :done))}
                             (ui/label "Body"))
                   (ui/notification {:message "ok" :on-close #(reset! got :note)})
                   (ui/virtual-list [{:id :alpha :label "Alpha"}]
                                    {:on-change #(reset! got %)})
                   (ui/settings [{:id :general
                                  :items [{:id :notify :label "N" :checked true}]}]
                                {:on-change #(reset! got %)})))
        children (:children exported)]
    (is (string? (get-in children [0 :on-close])))
    (is (string? (get-in children [0 :footer :on-click])))
    (is (string? (get-in children [1 :on-close])))
    (is (string? (get-in children [2 :on-change])))
    (is (string? (get-in children [3 :on-change])))
    (is (= {:ok true :id (get-in children [0 :footer :on-click])}
           (runtime/invoke-callback! (get-in children [0 :footer :on-click]))))
    (is (= :done @got))
    (is (= {:ok true :id (get-in children [2 :on-change])}
           (runtime/invoke-callback! (get-in children [2 :on-change]) "alpha")))
    (is (= :alpha @got))
    (is (= {:ok true :id (get-in children [3 :on-change])}
           (runtime/invoke-callback! (get-in children [3 :on-change])
                                     {:id "notify" :value true})))
    (is (= {:id :notify :value true} @got))))

(deftest settings-flat-dropdown-restores-field-and-option-ids
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/settings
                   [{:id :general
                     :label "General"
                     :items [{:id :theme
                              :label "Theme"
                              :variant :dropdown
                              :value :dark
                              :items [{:id :dark :label "Dark"}
                                      {:id :light :label "Light"}]}]}]
                   {:on-change #(reset! got %)}))]
    (is (= {:ok true :id (:on-change exported)}
           (runtime/invoke-callback! (:on-change exported)
                                     {:id "theme" :value "light"})))
    (is (= {:id :theme :value :light} @got))))

(deftest settings-grouped-dropdown-restores-ids
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/settings
                   [{:id :general
                     :label "General"
                     :items [{:label "Appearance"
                              :items [{:id :theme
                                       :label "Theme"
                                       :variant :dropdown
                                       :value :dark
                                       :items [{:id :dark :label "Dark"}
                                               {:id :light :label "Light"}]}]}
                             {:label "Advanced"
                              :items [{:id :debug
                                       :label "Debug"
                                       :variant :switch
                                       :checked false}]}]}]
                   {:on-change #(reset! got %)}))]
    (is (= {:ok true :id (:on-change exported)}
           (runtime/invoke-callback! (:on-change exported)
                                     {:id "theme" :value "light"})))
    (is (= {:id :theme :value :light} @got))
    (is (= {:ok true :id (:on-change exported)}
           (runtime/invoke-callback! (:on-change exported)
                                     {:id "debug" :value true})))
    (is (= {:id :debug :value true} @got))))

(deftest later-export-invalidates-prior-callback-ids
  (runtime/reset-callbacks!)
  (let [gen1 (atom 0)
        exported-1 (runtime/export-tree
                    (ui/dialog true {:on-ok #(swap! gen1 inc)}
                               (ui/label "one")))
        id-1 (:on-ok exported-1)
        gen2 (atom 0)
        exported-2 (runtime/export-tree
                    (ui/dialog true {:on-ok #(swap! gen2 inc)}
                               (ui/label "two")))
        id-2 (:on-ok exported-2)]
    (is (string? id-1))
    (is (string? id-2))
    (is (not= id-1 id-2) "callback ids are not reused across exports")
    (is (= {:ok false :error (str "unknown callback " id-1)}
           (runtime/invoke-callback! id-1)))
    (is (zero? @gen1) "stale id must not run the prior handler")
    (is (= {:ok true :id id-2} (runtime/invoke-callback! id-2)))
    (is (= 1 @gen2) "current id still runs the new handler")))
