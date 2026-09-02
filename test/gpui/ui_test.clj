(ns gpui.ui-test
  (:require [clojure.data.json :as json]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [clojure.test :refer [deftest is testing]]
            [gpui.runtime :as runtime]
            [gpui.ui :as ui]))

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
  (testing "text-field"
    (is (= {:type :text-field :text "hi"}
           (ui/text-field "hi")))
    (let [n (ui/text-field "x" {:placeholder "Todo" :id "new-todo"})]
      (is (= :text-field (:type n)))
      (is (= "Todo" (:placeholder n)))
      (is (= "new-todo" (:id n))))
    (is (fn? (:on-change (ui/text-field "" (fn [s] s))))))
  (testing "style keys pass through"
    (is (true? (:strikethrough (ui/label "x" {:strikethrough true}))))
    (is (= :ghost (:variant (ui/button "All" (fn []) {:variant :ghost}))))
    (is (= :circle (:shape (ui/checkbox false (fn []) {:shape :circle}))))
    (is (fn? (:on-double-click (ui/label "x" {:on-double-click (fn [])}))))
    (is (fn? (:on-click (ui/label "row" {:on-click (fn [])}))))
    (is (fn? (:on-click (ui/hstack {:on-click (fn [])}))))
    (is (true? (:focus (ui/text-field "" {:focus true}))))))

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
  (testing "divider"
    (is (= :divider (:type (ui/divider))))
    (is (= "or" (:text (ui/divider "or"))))
    (is (true? (:dashed (ui/divider {:dashed true})))))
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
                   (ui/text-field "" {:on-change #(reset! got %)})))
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
                   (ui/text-field "hi" {:on-blur (fn [_])
                                        :on-escape (fn [])})))]
    (is (string? (get-in exported [:children 0 :on-double-click])))
    (is (fn? (runtime/lookup-callback (get-in exported [:children 0 :on-double-click]))))
    (is (string? (get-in exported [:children 1 :on-blur])))
    (is (string? (get-in exported [:children 1 :on-escape])))))

(deftest invoke-callback-passes-text-value
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/text-field "" {:on-change #(reset! got %)
                                     :on-submit #(reset! got (str "go:" %))}))
        change-id (:on-change exported)
        submit-id (:on-submit exported)]
    (is (string? change-id))
    (is (= {:ok true :id change-id} (runtime/invoke-callback! change-id "typed")))
    (is (= "typed" @got))
    (is (= {:ok true :id submit-id} (runtime/invoke-callback! submit-id "done")))
    (is (= "go:done" @got))))

(deftest export-tree-error-overlay
  (runtime/reset-callbacks!)
  (let [exported (runtime/export-tree (fn [] (throw (ex-info "boom" {}))))]
    (is (some #(= "Clojure error" (:text %))
              (tree-seq :children :children exported)))))
