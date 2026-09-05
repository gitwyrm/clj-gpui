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
  (is (some? (ns-resolve 'gpui.ui 'pagination)))
  (is (some? (ns-resolve 'gpui.ui 'progress-circle)))
  (is (some? (ns-resolve 'gpui.ui 'shimmer)))
  (is (some? (ns-resolve 'gpui.ui 'hover-card)))
  (is (some? (ns-resolve 'gpui.ui 'avatar-group)))
  (is (some? (ns-resolve 'gpui.ui 'dropdown-button)))
  (is (some? (ns-resolve 'gpui.ui 'table-header)))
  (is (some? (ns-resolve 'gpui.ui 'table-body)))
  (is (some? (ns-resolve 'gpui.ui 'table-footer)))
  (is (some? (ns-resolve 'gpui.ui 'table-row)))
  (is (some? (ns-resolve 'gpui.ui 'table-head)))
  (is (some? (ns-resolve 'gpui.ui 'table-cell)))
  (is (some? (ns-resolve 'gpui.ui 'message)))
  (is (some? (ns-resolve 'gpui.ui 'message-group)))
  (is (some? (ns-resolve 'gpui.ui 'bubble)))
  (is (some? (ns-resolve 'gpui.ui 'bubble-reactions)))
  (is (some? (ns-resolve 'gpui.ui 'attachment)))
  (is (some? (ns-resolve 'gpui.ui 'attachment-media-overlay)))
  (is (some? (ns-resolve 'gpui.ui 'marker)))
  (is (some? (ns-resolve 'gpui.ui 'message-scroller)))
  (is (some? (ns-resolve 'gpui.ui 'nav-stack)))
  (is (some? (ns-resolve 'gpui.ui 'nav-page)))
  (is (some? (ns-resolve 'gpui.ui 'native-menu)))
  (is (some? (ns-resolve 'gpui.ui 'command)))
  (is (some? (ns-resolve 'gpui.ui 'status-bar)))
  (is (some? (ns-resolve 'gpui.ui 'horizontal-bar-chart)))
  (is (some? (ns-resolve 'gpui.ui 'radar-chart)))
  (is (some? (ns-resolve 'gpui.ui 'candlestick-chart)))
  (is (some? (ns-resolve 'gpui.ui 'sankey-chart))))

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
    (is (true? (:truncate (ui/label "path" {:truncate true}))))
    (is (= :nowrap (:whitespace (ui/label "path" {:whitespace :nowrap}))))
    (is (= :ellipsis-middle (:text-overflow (ui/label "path" {:text-overflow :ellipsis-middle}))))
    (is (= 2 (:line-clamp (ui/label "wrap" {:line-clamp 2}))))
    (is (= "Lovelace" (:secondary (ui/label "Ada" {:secondary "Lovelace"}))))
    (is (true? (:masked (ui/label "secret" {:masked true}))))
    (is (= "Hel" (:highlights (ui/label "Hello" {:highlights "Hel" :highlights-match :prefix}))))
    (is (= :prefix (:highlights-match (ui/label "Hello" {:highlights "Hel" :highlights-match :prefix}))))
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
    (is (= "check" (:icon n))))
  (let [n (ui/button "Go" {:size :small})]
    (is (= "small" (:control-size n)))
    (is (nil? (:size n))))
  (let [n (ui/button "Go" (fn []) {:size :large})]
    (is (= "large" (:control-size n)))
    (is (nil? (:size n)))))

(deftest option-item-normalization
  (is (= {:id "light" :label "light"} (ui/option-item :light)))
  (is (= {:id "Rust" :label "Rust"} (ui/option-item "Rust")))
  (is (= {:id "clj" :label "Clojure"}
         (ui/option-item {:id :clj :label "Clojure"})))
  (is (= 10 (:value (ui/option-item {:id :a :label "A" :value 10}))))
  (is (= "#ff0000" (:stroke (ui/option-item {:id :desk :stroke "#ff0000"}))))
  (is (= "#aabbcc" (:fill (ui/option-item {:id :s :fill "#aabbcc"}))))
  (is (= "3 units" (:display (ui/option-item {:id :a :label "A" :value 3 :display "3 units"}))))
  (is (= {:stops [{:color "#111111" :at 0} {:color "#ffffff" :at 1}]
          :space "bar"}
         (:fill (ui/option-item {:id :a :value 1
                                 :fill {:stops [{:color "#111111" :at 0}
                                                {:color "#ffffff" :at 1}]
                                        :space :bar}}))))
  (is (= 20 (:inner-radius (ui/option-item {:id :a :value 2 :inner-radius 20 :outer-radius 80}))))
  (is (= [80 60] (:values (ui/option-item {:id :s :label "Speed" :values [80 60]}))))
  (is (= [80 60] (:value (ui/option-item {:id :s :label "Speed" :value [80 60]}))))
  (is (= "rev" (:source (ui/option-item {:source :rev :target :cost :value 55}))))
  (is (= 100 (:open (ui/option-item {:id :mon :label "Mon" :open 100 :close 105}))))
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
    (is (= "dark" (ui/resolve-option-id (ui/option-id-map ["dark" :dark]) "dark"))))
  (testing "format-option-id accepts flat ids and grouped path vectors"
    (is (nil? (ui/format-option-id nil)))
    (is (= "copy" (ui/format-option-id :copy)))
    (is (= "copy" (ui/format-option-id "copy")))
    (is (= "edit/find" (ui/format-option-id [:edit :find])))
    (is (= "edit/find" (ui/format-option-id ["edit" "find"])))
    (is (= "Ready" (or (ui/format-option-id nil) "Ready")))
    (is (= "edit/find" (or (ui/format-option-id [:edit :find]) "Ready")))
    (let [bar (ui/status-bar {:left (ui/label "Ln 1")
                              :right [(ui/kbd "ctrl-k") (ui/label "UTF-8")]}
                             (ui/label (or (ui/format-option-id [:edit :find]) "Ready")))
          exported (runtime/export-tree bar)]
      (is (= "edit/find" (get-in exported [:children 0 :text]))
          "export-tree of a grouped Command pick must not throw"))))

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
    (is (= 42 (:value (ui/slider 42 {:min 0 :max 100 :step 5}))))
    (let [n (ui/slider [20 70] {:min 0 :max 100 :scale :logarithmic :reverse true})]
      (is (= [20 70] (:value n)))
      (is (= "logarithmic" (:scale n)))
      (is (true? (:reverse n))))
    (is (true? (:range (ui/slider 40 {:range true}))))
    (is (fn? (:on-release (ui/slider 40 {:on-release (fn [_])})))))
  (testing "select options"
    (let [n (ui/select :clj {:options [{:id :clj :label "Clojure"} "Rust"]
                             :placeholder "Lang"})]
      (is (= :select (:type n)))
      (is (= "clj" (:value n)))
      (is (= [{:id "clj" :label "Clojure"} {:id "Rust" :label "Rust"}]
             (:options n)))
      (is (= "Lang" (:placeholder n))))
    (let [n (ui/select :rs {:options [{:label "Lisp"
                                       :items [{:id :clj :label "Clojure"}
                                               {:id :cljs :label "ClojureScript"
                                                :display "ClojureScript (cljs)"}]}
                                      {:label "Systems"
                                       :items [{:id :rs :label "Rust"}
                                               {:id :go :label "Go" :disabled true}]}]
                            :searchable true
                            :cleanable true
                            :title-prefix "Lang: "
                            :menu-width 280
                            :empty "No languages"
                            :focus-ring false})]
      (is (true? (:searchable n)))
      (is (true? (:cleanable n)))
      (is (= "Lang: " (:title-prefix n)))
      (is (= 280 (:menu-width n)))
      (is (= "No languages" (:empty n)))
      (is (false? (:focus-ring n)))
      (is (= "Lisp" (get-in n [:options 0 :label])))
      (is (= "clj" (get-in n [:options 0 :items 0 :id])))
      (is (= "ClojureScript (cljs)" (get-in n [:options 0 :items 1 :display])))
      (is (true? (get-in n [:options 1 :items 1 :disabled]))))
    (let [!got (atom nil)
          n (ui/select :clj
                       {:options [{:label "clj"
                                   :items [{:id :clj :label "Clojure"}]}]
                        :on-change #(reset! !got %)})]
      ((:on-change n) "clj")
      (is (= :clj @!got))))
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
  (testing "progress-circle pagination shimmer"
    (let [n (ui/progress-circle 45 {:size :large :loading true :color "#3366ff"}
                                (ui/label "45"))]
      (is (= :progress-circle (:type n)))
      (is (= 45 (:value n)))
      (is (true? (:loading n)))
      (is (= "large" (:control-size n)))
      (is (nil? (:size n)))
      (is (= :label (get-in n [:children 0 :type]))))
    (let [n (ui/progress-circle 10 (ui/label "a") (ui/label "b"))]
      (is (= ["a" "b"] (mapv :text (:children n)))))
    (is (= 0 (:value (ui/progress-circle nil))))
    (let [n (ui/pagination 3 {:total 12 :compact true :visible-pages 7 :size :small})]
      (is (= :pagination (:type n)))
      (is (= 3 (:value n)))
      (is (= 12 (:total n)))
      (is (true? (:compact n)))
      (is (= 7 (:visible-pages n)))
      (is (= "small" (:control-size n))))
    (is (= 1 (:value (ui/pagination nil))))
    (let [n (ui/shimmer "Thinking…" {:duration 1 :spread 0.4 :spread-px 48
                                     :reverse true :once true
                                     :highlight-color "#ffffff"
                                     :truncate true :flex 1})]
      (is (= :shimmer (:type n)))
      (is (= "Thinking…" (:text n)))
      (is (= 1 (:duration n)))
      (is (= 0.4 (:spread n)))
      (is (= 48 (:spread-px n)))
      (is (true? (:reverse n)))
      (is (true? (:once n)))
      (is (= "#ffffff" (:highlight-color n)))
      (is (true? (:truncate n)))
      (is (= 1 (:flex n)))))
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
    (is (= "Ada" (:text (ui/avatar "Ada"))))
    (let [n (ui/avatar {:name "Ada Lovelace"
                        :src "https://example.com/ada.png"
                        :icon :building-2
                        :size :large})]
      (is (= :avatar (:type n)))
      (is (= "Ada Lovelace" (:text n)))
      (is (= "https://example.com/ada.png" (:src n)))
      (is (= "building-2" (:icon n)))
      (is (= "large" (:control-size n))))
    (is (nil? (:src (ui/avatar {:name "Ada" :src ""}))))
    (is (nil? (:text (ui/avatar {:src "https://example.com/x.png"}))))
    (let [n (ui/avatar-group {:limit 5 :ellipsis true :size :small}
                             (ui/avatar "Ada")
                             "Grace"
                             (ui/avatar {:name "Alan" :src "https://example.com/alan.png"}))]
      (is (= :avatar-group (:type n)))
      (is (= 5 (:limit n)))
      (is (true? (:ellipsis n)))
      (is (= "small" (:control-size n)))
      (is (= ["Ada" "Grace" "Alan"] (mapv :text (:children n))))
      (is (= "https://example.com/alan.png" (get-in n [:children 2 :src]))))
    (let [n (ui/hover-card {:trigger (ui/link "https://example.com" "@ada")
                            :open-delay 0.2
                            :close-delay 0.1
                            :anchor :bottom-left
                            :appearance false
                            :on-open-change identity}
                           (ui/label "Ada Lovelace")
                           (ui/avatar "Ada"))]
      (is (= :hover-card (:type n)))
      (is (= 0.2 (:open-delay n)))
      (is (= 0.1 (:close-delay n)))
      (is (= "bottom-left" (:placement n)))
      (is (false? (:appearance n)))
      (is (fn? (:on-open-change n)))
      (is (= :link (get-in n [:trigger :type])))
      (is (= ["Ada Lovelace" "Ada"] (mapv :text (:children n)))))
    (is (= :label (get-in (ui/hover-card {:trigger "Hover"} (ui/label "Hi"))
                          [:trigger :type]))))
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
                   (ui/slider 10 {:on-change #(reset! got %)
                                  :on-release #(reset! got [:release %])})
                   (ui/select "a" {:options ["a" "b"]
                                   :on-change #(reset! got %)})
                   (ui/alert "x" {:on-close #(reset! got :closed)})
                   (ui/clipboard "z" {:on-copied #(reset! got %)})))
        children (:children exported)
        switch-id (get-in children [0 :on-change])
        slider-id (get-in children [1 :on-change])
        slider-release (get-in children [1 :on-release])
        select-id (get-in children [2 :on-change])
        close-id (get-in children [3 :on-close])
        copied-id (get-in children [4 :on-copied])]
    (is (string? switch-id))
    (is (= {:ok true :id switch-id} (runtime/invoke-callback! switch-id true)))
    (is (true? @got))
    (is (= {:ok true :id slider-id} (runtime/invoke-callback! slider-id 33.5)))
    (is (= 33.5 @got))
    (is (string? slider-release))
    (is (not= slider-id slider-release))
    (is (= {:ok true :id slider-release} (runtime/invoke-callback! slider-release [20.0 70.0])))
    (is (= [:release [20.0 70.0]] @got))
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
      (is (= :label (get-in ctx [:children 0 :type]))))
    (let [native (ui/native-menu
                  [{:id :copy :label "Copy"} :- {:id :wrap :label "Word wrap" :checked true}]
                  {:id "edit-menu" :open? true :position [120 40] :on-change (fn [_])})]
      (is (= :native-menu (:type native)))
      (is (true? (:open native)))
      (is (nil? (:open? native)))
      (is (= [120 40] (:position native)))
      (is (true? (get-in native [:items 1 :separator])))
      (is (true? (get-in native [:items 2 :checked])))
      (is (fn? (:on-change native))))
    (let [disabled-share (ui/native-menu
                          [{:id :copy :label "Copy"}
                           {:id :share :label "Share" :disabled true
                            :items [{:id :link :label "Copy link"}]}]
                          {:id "edit-menu"})
          exported (runtime/export-tree disabled-share)]
      (is (true? (get-in disabled-share [:items 1 :disabled]))
          "submenu wrappers keep :disabled")
      (is (= "link" (get-in disabled-share [:items 1 :items 0 :id])))
      (is (true? (get-in exported [:items 1 :disabled]))))
    (let [cmd (ui/command
               [{:id :copy :label "Copy" :icon :copy :keywords [:duplicate]}
                :-
                {:label "Edit" :items [{:id :find :label "Find"}]}]
               {:placeholder "Type a command…" :menu-max-h 220
                :selected :find :query "fi" :bordered false
                :on-change (fn [_]) :on-select (fn [_]) :on-confirm (fn [_])
                :on-query (fn [_])})]
      (is (= :command (:type cmd)))
      (is (true? (:searchable cmd)))
      (is (= "Type a command…" (:placeholder cmd)))
      (is (= 220 (:menu-max-h cmd)))
      (is (nil? (:height cmd)))
      (is (= "fi" (:query cmd)))
      (is (false? (:bordered cmd)))
      (is (= "find" (:value cmd)))
      (is (nil? (:selected cmd)))
      (is (= "copy" (get-in cmd [:items 0 :icon])))
      (is (= ["duplicate"] (get-in cmd [:items 0 :keywords])))
      (is (true? (get-in cmd [:items 1 :separator])))
      (is (= "find" (get-in cmd [:items 2 :items 0 :id])))
      (is (fn? (:on-change cmd)))
      (is (fn? (:on-select cmd)))
      (is (fn? (:on-confirm cmd))))
    (let [items [{:id :file :label "File" :items [{:id :open :label "Open file"}]}
                 {:id :project :label "Project" :items [{:id :open :label "Open project"}]}]
          !selected (atom nil)
          cmd (ui/command items {:selected @!selected
                                 :on-select #(reset! !selected %)
                                 :on-change #(reset! !selected %)
                                 :on-confirm #(reset! !selected %)})]
      ((:on-select cmd) ["project" "open"])
      (is (= [:project :open] @!selected))
      (let [echo (ui/command items {:selected @!selected
                                    :on-select #(reset! !selected %)})]
        (is (= ["project" "open"] (:value echo))
            "echoing the grouped payload as :selected must stay a path"))
      ((:on-change cmd) ["file" "open"])
      (is (= [:file :open] @!selected))
      ((:on-confirm cmd) "copy")
      (is (= "copy" @!selected)
          "unknown wire ids stay as received"))
    (let [!got (atom nil)
          n (ui/native-menu
             [{:id :file :label "File" :items [{:id :open :label "Open file"}]}
              {:id :project :label "Project" :items [{:id :open :label "Open project"}]}]
             {:on-change #(reset! !got %)})]
      ((:on-change n) ["project" "open"])
      (is (= [:project :open] @!got))
      ((:on-change n) "copy")
      (is (= "copy" @!got)))
    (let [!got (atom nil)
          n (ui/dropdown-menu
             [{:id :share :label "Share"
               :items [{:id :link :label "Copy link"}]}]
             {:on-change #(reset! !got %)}
             (ui/button "Edit"))]
      ((:on-change n) ["share" "link"])
      (is (= [:share :link] @!got))
      ((:on-change n) "copy")
      (is (= "copy" @!got)))
    (let [bar (ui/status-bar {:left (ui/label "Ln 1")
                              :right [(ui/kbd "ctrl-s") (ui/label "UTF-8")]}
                             (ui/label "Ready"))]
      (is (= :status-bar (:type bar)))
      (is (= :label (get-in bar [:left 0 :type])))
      (is (= "Ln 1" (get-in bar [:left 0 :text])))
      (is (= 2 (count (:right bar))))
      (is (= :label (get-in bar [:children 0 :type]))))
    (let [split (ui/dropdown-button [{:id :csv :label "CSV"} :- {:id :pdf :label "PDF"}]
                                    {:on-change (fn [_]) :variant :primary :size :small
                                     :anchor :bottom-left}
                                    (ui/button "Export" (fn [])))]
      (is (= :dropdown-button (:type split)))
      (is (= :button (get-in split [:trigger :type])))
      (is (= "Export" (get-in split [:trigger :text])))
      (is (fn? (get-in split [:trigger :on-click])))
      (is (= :primary (:variant split)))
      (is (= "small" (:control-size split)))
      (is (= "bottom-left" (:placement split)))
      (is (nil? (:anchor split)))
      (is (true? (get-in split [:items 1 :separator])))
      (is (fn? (:on-change split))))
    (let [split (ui/dropdown-button [{:id :csv :label "CSV"}]
                                    {:selected true :variant :warning}
                                    (ui/button "Export" {:size :small :selected true :outline true}))]
      (is (true? (:selected split)))
      (is (= :warning (:variant split)))
      (is (nil? (:control-size split)))
      (is (nil? (:value split)))
      (is (= "small" (get-in split [:trigger :control-size])))
      (is (nil? (get-in split [:trigger :size])))
      (is (true? (get-in split [:trigger :selected])))
      (is (true? (get-in split [:trigger :outline]))))
    (is (= :link (:variant (ui/dropdown-button [{:id :csv :label "CSV"}]
                                               {:variant :link}
                                               (ui/button "Go")))))
    (is (= :secondary (:variant (ui/dropdown-button [{:id :csv :label "CSV"}]
                                                    {:variant :secondary}
                                                    (ui/button "Go")))))
    (is (= :success (:variant (ui/dropdown-button [{:id :csv :label "CSV"}]
                                                  {:variant :success}
                                                  (ui/button "Go")))))
    (is (= :info (:variant (ui/dropdown-button [{:id :csv :label "CSV"}]
                                               {:variant :info}
                                               (ui/button "Go")))))
    (is (nil? (:trigger (ui/dropdown-button [{:id :csv :label "CSV"}])))))
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
  (testing "data-table cells keep widget nodes for render_td"
    (let [n (ui/data-table {:columns [{:id :name :label "Name"}
                                      {:id :done :label "Done"}
                                      {:id :status :label "Status"}]
                            :rows [{:id :ada
                                    :cells ["Ada"
                                            (ui/progress 72 {:width 120})
                                            (ui/tag "stable")]}
                                   ["Grace" (ui/progress 45) (ui/tag "beta")]]})]
      (is (= "Ada" (get-in n [:items 0 :cells 0])))
      (is (= :progress (get-in n [:items 0 :cells 1 :type])))
      (is (= 72 (get-in n [:items 0 :cells 1 :value])))
      (is (= 120 (get-in n [:items 0 :cells 1 :width])))
      (is (= :tag (get-in n [:items 0 :cells 2 :type])))
      (is (= "stable" (get-in n [:items 0 :cells 2 :text])))
      (is (= "Grace" (get-in n [:items 1 :id])))
      (is (= :progress (get-in n [:items 1 :cells 1 :type])))
      (is (= 45 (get-in n [:items 1 :cells 1 :value]))))
    (let [n (ui/data-table {:columns [{:id :n :label "N"}]
                            :rows [{:cells [(ui/progress 9)]}]})]
      (is (= "9" (:id (first (:items n)))))
      (is (= :progress (get-in n [:items 0 :cells 0 :type])))))
  (testing "data-table extras wire header groups, cell selected, and export"
    (let [n (ui/data-table {:columns [{:id :name :label "Name" :align :end :selectable false}
                                      {:id :lang :label "Lang"}]
                            :rows [{:id :ada :cells ["Ada" "Clojure"]}]
                            :header-groups [[{:label "Identity" :span 2}]]
                            :cell-selectable true
                            :row-header false
                            :row-height 40
                            :export-generation :dump-1
                            :selected {:row :ada :col :lang}})]
      (is (= {:row "ada" :col "lang"} (:value n)))
      (is (true? (:cell-selectable n)))
      (is (false? (:row-header n)))
      (is (= 40 (:row-height n)))
      (is (= "dump-1" (:export-generation n)))
      (is (= "Identity" (get-in n [:header-groups 0 0 :label])))
      (is (= 2 (get-in n [:header-groups 0 0 :span])))
      (is (= "end" (get-in n [:options 0 :align])))
      (is (false? (get-in n [:options 0 :selectable]))))
    (let [vec-sel (ui/data-table {:columns [{:id :name :label "Name"}
                                            {:id :lang :label "Lang"}]
                                  :rows [{:id :ada :cells ["Ada" "Clojure"]}]
                                  :selected [:ada :lang]})]
      (is (= ["ada" "lang"] (:value vec-sel))))
    (let [named (ui/data-table {:columns [{:id :name :label "Name"}]
                                :rows [{:id :ada :cells ["Ada"]}]
                                :size :small
                                :selected :ada})]
      (is (= "small" (:control-size named)))
      (is (nil? (:size named)))
      (is (not (contains? named :header-groups))))
    (let [padded (ui/data-table {:columns [{:id :user/name :label "Name"}]
                                 :rows [{:id :user/ada :cells ["Ada"]}]
                                 :selected {:row :user/ada :col :user/name}})]
      (is (= {:row "user/ada" :col "user/name"} (:value padded)))))
  (testing "declarative table shorthand expands to Kit primitives"
    (let [n (ui/table {:columns [{:label "Name" :span 2}
                                 {:label "Amount" :align :end :width 80}]
                       :rows [["Ada" "$250"] {:id :rich :cells ["Rich" "$150"]}]
                       :footer [{:span 2 :align :end :text "Total $400"}]
                       :caption "Invoices"
                       :accessibility-label "Invoice table"})
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
      (is (= "end" (get-in foot-row [:children 0 :align])))
      (is (= "Invoice table" (:accessibility-label n)))))
  (testing "declarative table primitives accept widget children"
    (let [n (ui/table
             {:accessibility-label "Staff"}
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
      (is (= "Staff" (:accessibility-label n)))
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
  (testing "combobox nested items restore leaf ids"
    (let [!change (atom nil)
          !confirm (atom nil)
          n (ui/combobox :rs
                         {:options [{:label "Lisp"
                                     :items [{:id :clj :label "Clojure"}
                                             {:id :cljs :label "ClojureScript"}]}
                                    {:label "Systems"
                                     :items [{:id :rs :label "Rust"}
                                             {:id :go :label "Go" :disabled true}]}]
                          :on-change #(reset! !change %)
                          :on-confirm #(reset! !confirm %)})]
      (is (= "Lisp" (get-in n [:options 0 :label])))
      (is (= "clj" (get-in n [:options 0 :items 0 :id])))
      (is (true? (get-in n [:options 1 :items 1 :disabled])))
      ((:on-change n) "clj")
      (is (= :clj @!change))
      ((:on-confirm n) "cljs")
      (is (= :cljs @!confirm)))
    (let [!got (atom nil)
          n (ui/combobox [:clj]
                         {:multiple true
                          :options [{:label "Lisp"
                                     :items [{:id :clj :label "Clojure"}
                                             {:id :rs :label "Rust"}]}]
                          :on-change #(reset! !got %)})]
      ((:on-change n) ["clj" "rs"])
      (is (= [:clj :rs] @!got))))
  (testing "combobox chrome builders stay on the node"
    (let [n (ui/combobox :clj
                         {:options [{:id :clj :label "Clojure"}]
                          :cleanable true
                          :menu-width 280
                          :menu-max-h 240
                          :search-placeholder "Filter…"
                          :empty "No languages"
                          :icon :search
                          :check-icon :check
                          :appearance false
                          :focus-ring false
                          :query "clj"})]
      (is (true? (:cleanable n)))
      (is (= 280 (:menu-width n)))
      (is (= 240 (:menu-max-h n)))
      (is (= "Filter…" (:search-placeholder n)))
      (is (= "No languages" (:empty n)))
      (is (= :search (:icon n)))
      (is (= :check (:check-icon n)))
      (is (false? (:appearance n)))
      (is (false? (:focus-ring n)))
      (is (= "clj" (:query n))))
    (let [plain (ui/combobox :clj {:options [:clj]})
          released (ui/combobox :clj {:options [:clj] :query nil})
          cleared (ui/combobox :clj {:options [:clj] :query ""})]
      (is (not (contains? plain :query))
          "omitted :query stays off the node")
      (is (contains? released :query)
          "explicit nil keeps the key (JSON null), unlike omit")
      (is (nil? (:query released)))
      (is (= "" (:query cleared))
          "empty string is a controlled clear, not a release"))
    (let [exported (runtime/export-tree
                    (ui/combobox :clj {:options [:clj] :query "rs"}))
          omitted (runtime/export-tree
                   (ui/combobox :clj {:options [:clj]}))
          released (runtime/export-tree
                    (ui/combobox :clj {:options [:clj] :query nil}))
          cleared (runtime/export-tree
                   (ui/combobox :clj {:options [:clj] :query ""}))]
      (is (= "rs" (:query exported)))
      (is (not (contains? omitted :query)))
      (is (contains? released :query))
      (is (nil? (:query released)))
      (is (= "" (:query cleared)))))
  (testing "group titles that share a wire id do not shadow leaf callbacks"
    (let [opts [{:label "clj" :items [{:id :clj :label "Clojure"}]}
                {:label "rs" :items [{:id :rs :label "Rust"}]}]
          mixed [{:id :go :label "Go"}
                 {:label "clj" :items [{:id :clj :label "Clojure"}]}]]
      (is (= [:clj :rs] (mapv ui/option-identity (ui/selectable-option-leaves opts))))
      (is (= [:go :clj] (mapv ui/option-identity (ui/selectable-option-leaves mixed))))
      (is (= ["clj" :clj "rs" :rs]
             (mapv ui/option-identity (ui/flatten-tree-items opts))))
      (is (= "clj"
             (ui/resolve-option-id (ui/option-id-map (ui/flatten-tree-items opts)) "clj")))
      (is (= :clj
             (ui/resolve-option-id (ui/option-id-map (ui/selectable-option-leaves opts)) "clj"))))
    (let [!change (atom nil)
          !confirm (atom nil)
          n (ui/combobox :clj
                         {:options [{:label "clj"
                                     :items [{:id :clj :label "Clojure"}]}
                                    {:label "rs"
                                     :items [{:id :rs :label "Rust"}]}]
                          :on-change #(reset! !change %)
                          :on-confirm #(reset! !confirm %)})]
      ((:on-change n) "clj")
      (is (= :clj @!change))
      ((:on-confirm n) "clj")
      (is (= :clj @!confirm)))
    (let [!got (atom nil)
          n (ui/combobox [:clj]
                         {:multiple true
                          :options [{:label "clj"
                                     :items [{:id :clj :label "Clojure"}]}
                                    {:label "rs"
                                     :items [{:id :rs :label "Rust"}]}]
                          :on-change #(reset! !got %)
                          :on-confirm #(reset! !got %)})]
      ((:on-change n) ["clj" "rs"])
      (is (= [:clj :rs] @!got))
      ((:on-confirm n) ["rs"])
      (is (= [:rs] @!got))))
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
    (let [n (ui/horizontal-bar-chart
             [{:id :src :label "src" :value 412 :color "#3366ff"}]
             {:labels true :value-axis true :height 220})]
      (is (= "bar" (:variant n)))
      (is (= "left" (:alignment n)))
      (is (true? (:labels n)))
      (is (true? (:value-axis n)))
      (is (= "#3366ff" (get-in n [:items 0 :color]))))
    (let [n (ui/radar-chart [{:id :speed :label "Speed" :values [80 60]
                              :content (ui/badge 1 (ui/label "Sp"))}]
                            {:series [{:id :desktop :label "Desktop"}
                                      {:id :mobile :label "Mobile"}]})]
      (is (= "radar" (:variant n)))
      (is (= [80 60] (get-in n [:items 0 :values])))
      (is (= "desktop" (get-in n [:series 0 :id])))
      (is (= :badge (get-in n [:items 0 :content :type]))))
    (let [n (ui/candlestick-chart [{:id :mon :label "Mon"
                                    :open 100 :high 110 :low 95 :close 105}]
                                  {:body-width-ratio 1.5})]
      (is (= "candlestick" (:variant n)))
      (is (= 100 (get-in n [:items 0 :open])))
      (is (= 105 (get-in n [:items 0 :close])))
      (is (= 1.5 (:body-width-ratio n))))
    (let [n (ui/sankey-chart [{:id :rev :label "Revenue"} {:id :cost :label "Cost"}]
                             {:links [{:source :rev :target :cost :value 55}]
                              :node-align :left
                              :value-scale :sqrt})]
      (is (= "sankey" (:variant n)))
      (is (= "rev" (get-in n [:links 0 :source])))
      (is (= "cost" (get-in n [:links 0 :target])))
      (is (= 55 (get-in n [:links 0 :value])))
      (is (= "left" (:node-align n)))
      (is (= "sqrt" (:value-scale n))))
    (let [n (ui/chart :bar [{:id :a :label "A" :value 1}]
                      {:name "Size"
                       :tick-margin 0
                       :fill-gradient true
                       :fill-gradient-mode :chart
                       :corner-radii 4
                       :stroke-style :linear})]
      (is (= "Size" (:name n)))
      (is (= 0 (:tick-margin n)))
      (is (true? (:fill-gradient n)))
      (is (= "chart" (:fill-gradient-mode n)))
      (is (= 4 (:corner-radii n)))
      (is (= "linear" (:stroke-style n))))
    (let [n (ui/bar-chart [{:id :a :label "A" :value 3 :display "3u"
                            :fill {:stops [{:color "#3366ff" :at 0}
                                           {:color "#88aaff" :at 1}]
                                   :space :chart
                                   :angle 45}}
                           {:id :b :label "B" :value 7
                            :fill {:stops [{:color "#22c55e" :at 0}
                                           {:color "#86efac" :at 1}]
                                   :space :bar
                                   :angle 45}}]
                          {:fill "#112233"})]
      (is (= "3u" (get-in n [:items 0 :display])))
      (is (= "chart" (get-in n [:items 0 :fill :space])))
      (is (nil? (get-in n [:items 0 :fill :angle])))
      (is (= "#3366ff" (get-in n [:items 0 :fill :stops 0 :color])))
      (is (= 45 (get-in n [:items 1 :fill :angle])))
      (is (= "bar" (get-in n [:items 1 :fill :space])))
      (is (= "#112233" (:fill n))))
    (let [n (ui/chart :line [{:id :a :label "A" :value 1}] {:interactive true})]
      (is (true? (:interactive n))))
    (let [n (ui/area-chart [{:id :mon :label "Mon" :values [4 8]}]
                           {:series [{:id :desk :label "Desktop" :stroke "#ff0000"}
                                     {:id :mob :label "Mobile"}]})]
      (is (= "area" (:variant n)))
      (is (= "#ff0000" (get-in n [:series 0 :stroke])))
      (is (nil? (get-in n [:series 1 :stroke]))))
    (let [n (ui/area-chart [{:id :mon :label "Mon" :values [4 8]}]
                           {:series [{:id :desk :label "Desktop" :fill "#3366ff"
                                      :stroke-style :step-after}
                                     {:id :mob :label "Mobile"}]})]
      (is (= "area" (:variant n)))
      (is (= [4 8] (get-in n [:items 0 :values])))
      (is (= "#3366ff" (get-in n [:series 0 :fill])))
      (is (= "step-after" (get-in n [:series 0 :stroke-style]))))
    (let [n (ui/pie-chart [{:id :a :label "A" :value 2 :inner-radius 20 :outer-radius 80}
                           {:id :b :label "B" :value 5}]
                          {:inner-radius 40 :labels true :pad-angle 0.04})]
      (is (= 40 (:inner-radius n)))
      (is (true? (:labels n)))
      (is (= 0.04 (:pad-angle n)))
      (is (= 20 (get-in n [:items 0 :inner-radius])))
      (is (= 80 (get-in n [:items 0 :outer-radius])))
      (is (nil? (get-in n [:items 1 :inner-radius]))))
    (let [n (ui/sankey-chart [{:id :rev :label "Revenue"
                               :label-lines [{:text "Rev" :font-size 11}]}]
                             {:links [{:source :rev :target :cost :value 1}]
                              :node-width 14
                              :node-label false})]
      (is (= 14 (:node-width n)))
      (is (false? (:node-label n)))
      (is (= "Rev" (get-in n [:items 0 :label-lines 0 :text]))))
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

(deftest chat-message-bubble-constructors
  (testing "named message slots expand to children, not sheet footer"
    (let [n (ui/message {:alignment :end
                         :avatar (ui/avatar "You")
                         :header (ui/message-header "You" "10:25 AM")
                         :footer (ui/message-footer "Delivered")}
                        (ui/bubble "Outgoing"))]
      (is (= :message (:type n)))
      (is (= "end" (:alignment n)))
      (is (nil? (:footer n)) "message :footer is a child, not the sheet footer field")
      (is (= :message-avatar (get-in n [:children 0 :type])))
      (is (= :avatar (get-in n [:children 0 :children 0 :type])))
      (is (= :message-header (get-in n [:children 1 :type])))
      (is (= ["You" "10:25 AM"] (mapv :text (get-in n [:children 1 :children]))))
      (is (= :message-content (get-in n [:children 2 :type])))
      (is (= :bubble (get-in n [:children 2 :children 0 :type])))
      (is (= "Outgoing" (get-in n [:children 2 :children 0 :children 0 :text])))
      (is (= :message-footer (get-in n [:children 3 :type])))))
  (testing "bubble variants and reactions"
    (let [n (ui/bubble "Incoming" {:variant :secondary
                                   :reactions (ui/bubble-reactions "👍")})]
      (is (= :bubble (:type n)))
      (is (= "secondary" (:variant n)))
      (is (= :bubble-reactions (get-in n [:children 1 :type])))
      (is (= "👍" (get-in n [:children 1 :children 0 :text]))))
    (is (= "ghost" (:variant (ui/bubble {:variant :ghost} "System")))))
  (testing "attachment status and marker separator"
    (let [n (ui/attachment {:id "file-1" :status :uploading :orientation :vertical}
                           (ui/attachment-media {:src "preview.png"})
                           (ui/attachment-content
                            (ui/attachment-title "report.pdf")
                            (ui/attachment-description "Uploading"))
                           (ui/attachment-actions (ui/button "Cancel")))]
      (is (= :attachment (:type n)))
      (is (= "uploading" (:status n)))
      (is (= "vertical" (:orientation n)))
      (is (= :attachment-media (get-in n [:children 0 :type])))
      (is (= "preview.png" (get-in n [:children 0 :src])))
      (is (= "report.pdf" (get-in n [:children 1 :children 0 :text])))
      (is (= :attachment-actions (get-in n [:children 2 :type]))))
    (let [n (ui/marker "Today" {:variant :separator :id "day" :role :status})]
      (is (= :marker (:type n)))
      (is (= "Today" (:text n)))
      (is (= "separator" (:variant n)))
      (is (= "status" (:role n)))
      (is (empty? (:children n)))))
  (testing "message-scroller keeps row ids"
    (let [n (ui/message-scroller {:id "chat" :height 400 :jump-button false}
                                 (ui/message {:id "m1"} (ui/bubble "Hi"))
                                 (ui/message {:id "m2"} (ui/bubble "There")))]
      (is (= :message-scroller (:type n)))
      (is (= "chat" (:id n)))
      (is (= 400 (:height n)))
      (is (false? (:jump-button n)))
      (is (= ["m1" "m2"] (mapv :id (:children n))))))
  (testing "attachment-media size inherit vs override, and overlay vs child"
    (let [parent (ui/attachment {:size :small}
                                (ui/attachment-media {:src "a.png" :size :lg})
                                (ui/attachment-media {:src "b.png"}))]
      (is (= "small" (:control-size parent)))
      (is (= "lg" (get-in parent [:children 0 :control-size])))
      (is (nil? (get-in parent [:children 1 :control-size]))))
    (let [with-src (ui/attachment-media {:src "preview.png"} (ui/icon :file))
          overlay (ui/attachment-media {:src "preview.png"
                                        :overlay (ui/icon :loader)}
                                       (ui/icon :file))
          named (ui/attachment-media-overlay (ui/icon :loader))]
      (is (= :icon (get-in with-src [:children 0 :type])))
      (is (= :attachment-media-overlay (get-in overlay [:children 0 :type])))
      (is (= :icon (get-in overlay [:children 1 :type])))
      (is (= :attachment-media-overlay (:type named)))))
  (testing "explicit bubble-content style is kept beside a direct child"
    (let [n (ui/bubble {}
                       (ui/bubble-content {:bg "#111111" :padding 8} "hello")
                       "extra")]
      (is (= :bubble-content (get-in n [:children 0 :type])))
      (is (= "#111111" (get-in n [:children 0 :bg])))
      (is (= 8 (get-in n [:children 0 :padding])))
      (is (= :label (get-in n [:children 1 :type])))
      (is (= "extra" (get-in n [:children 1 :text])))))
  (testing "stack, shimmer, separator, and scroller style slots"
    (let [msg (ui/message {:stack-style {:gap 8 :padding 4 :bg "#1a1b26"}}
                          (ui/bubble "Hi"))
          title (ui/attachment-title {:shimmer-style {:duration 1.5 :reverse true}}
                                     "report.pdf")
          marker (ui/marker "Today" {:variant :separator
                                     :separator-style {:color "#7aa2f7"}
                                     :shimmer-style {:spread 0.4 :once true}})
          scroller (ui/message-scroller {:id "chat"
                                         :content-style {:padding 8}
                                         :list-style {:gap 4}
                                         :row-style {:padding 2}
                                         :jump-button-style {:bg "#111111"}
                                         :jump-button-renderer {:variant :primary
                                                                :size :small
                                                                :icon :arrow-down}}
                                        (ui/message {:id "m1"} (ui/bubble "Hi")))]
      (is (= 8 (get-in msg [:stack-style :gap])))
      (is (= "#1a1b26" (get-in msg [:stack-style :bg])))
      (is (= 1.5 (get-in title [:shimmer-style :duration])))
      (is (true? (get-in title [:shimmer-style :reverse])))
      (is (= "#7aa2f7" (get-in marker [:separator-style :color])))
      (is (= 0.4 (get-in marker [:shimmer-style :spread])))
      (is (true? (get-in marker [:shimmer-style :once])))
      (is (= 8 (get-in scroller [:content-style :padding])))
      (is (= 4 (get-in scroller [:list-style :gap])))
      (is (= "primary" (get-in scroller [:jump-button-renderer :variant])))
      (is (= "small" (get-in scroller [:jump-button-renderer :control-size])))
      (is (nil? (get-in scroller [:jump-button-renderer :size])))
      (is (= "arrow-down" (get-in scroller [:jump-button-renderer :icon])))))
  (testing "jump-button-label is the tooltip; renderer :label is Button.label"
    (let [n (ui/message-scroller {:id "chat"
                                  :jump-button-label "Jump tooltip"
                                  :jump-button-renderer {:label "Latest"}})]
      (is (= "Jump tooltip" (:jump-button-label n)))
      (is (= "Latest" (get-in n [:jump-button-renderer :text])))
      (is (nil? (get-in n [:jump-button-renderer :label])))))
  (testing "message-scroller scroll-to-item and scroll-to-end"
    (let [n (ui/message-scroller {:id "chat"
                                  :scroll-to-item :m1
                                  :scroll-generation 2}
                                 (ui/message {:id "m1"} (ui/bubble "Hi")))]
      (is (= "m1" (:scroll-to-item n)))
      (is (= 2 (:scroll-generation n)))
      (is (nil? (:scroll-to-end n))))
    (let [n (ui/message-scroller {:id "chat"
                                  :scroll-to-item 0
                                  :scroll-to-end true
                                  :scroll-generation "g3"}
                                 (ui/message {:id "m1"} (ui/bubble "Hi")))]
      (is (= 0 (:scroll-to-item n)))
      (is (true? (:scroll-to-end n)))
      (is (= "g3" (:scroll-generation n))))
    (let [plain (ui/message-scroller {:id "chat"}
                                     (ui/message {:id "m1"} (ui/bubble "Hi")))]
      (is (not (contains? plain :scroll-to-item)))
      (is (not (contains? plain :scroll-to-end)))
      (is (not (contains? plain :scroll-generation))))
    (let [exported (runtime/export-tree
                    (ui/message-scroller {:id "chat"
                                          :scroll-to-item "m2"
                                          :scroll-generation 1}
                                         (ui/message {:id "m2"} (ui/bubble "Hi"))))]
      (is (= "m2" (:scroll-to-item exported)))
      (is (= 1 (:scroll-generation exported))))
    (let [n (ui/message-scroller {:id "chat"
                                  :scroll-to-item " message-1 "}
                                 (ui/message {:id " message-1 "}
                                             (ui/bubble "Hi")))]
      (is (= " message-1 " (:scroll-to-item n))
          "scroll-to-item keeps leading/trailing space like the row :id")
      (is (= " message-1 " (:id (first (:children n))))))))

(deftest nav-stack-trail-and-page-catalog
  (let [n (ui/nav-stack {:id "nav" :stack [:home :detail] :transition 0.22
                         :transition-style :slide :overflow :hidden
                         :overflow-hidden true}
                        (ui/nav-page {:id :home} (ui/label "Home"))
                        (ui/nav-page {:id :detail :gap 8}
                                     (ui/button "Back" (fn []))))]
    (is (= :nav-stack (:type n)))
    (is (= "nav" (:id n)))
    (is (= ["home" "detail"] (:value n)))
    (is (= 0.22 (:duration n)))
    (is (nil? (:transition n)))
    (is (= "slide" (:transition-style n)))
    (is (= "hidden" (:overflow n)))
    (is (true? (:overflow-hidden n)))
    (is (= :nav-page (get-in n [:children 0 :type])))
    (is (= "home" (get-in n [:children 0 :id])))
    (is (= :label (get-in n [:children 0 :children 0 :type])))
    (is (= "detail" (get-in n [:children 1 :id])))
    (is (= 8 (get-in n [:children 1 :gap]))))
  (let [omitted (ui/nav-stack {:id "nav"}
                              (ui/nav-page {:id :home} "Home")
                              (ui/nav-page {:id :detail} "Detail"))]
    (is (= "home" (:value omitted)))
    (is (nil? (:transition-style omitted)))
    (is (nil? (:overflow omitted))))
  (let [cleared (ui/nav-stack {:id "nav" :stack [] :motion :immediate}
                              (ui/nav-page {:id :home} "Home"))]
    (is (= [] (:value cleared)))
    (is (= "immediate" (:motion cleared))))
  (let [timed (ui/nav-stack {:id "nav" :stack [:home] :transition 0.22}
                            (ui/nav-page {:id :home} "Home"))]
    (is (= 0.22 (:duration timed)))
    (is (nil? (:transition-style timed)))
    (is (nil? (:overflow timed))))
  (let [fwd (ui/nav-stack {:id "nav" :stack [:home]
                           :on-forward-change (fn [_])}
                          (ui/nav-page {:id :home} "Home")
                          (ui/nav-page {:id :detail} "Detail"))]
    (is (fn? (:on-forward-change fwd)))
    (is (nil? (:reuse-forward fwd)))
    (is (nil? (:replace-generation fwd)))
    (is (= "home" (get-in fwd [:children 0 :id])))
    (is (= "detail" (get-in fwd [:children 1 :id]))))
  (let [fresh (ui/nav-stack {:id "nav" :stack [:home :detail]
                             :reuse-forward false
                             :replace-generation 2}
                            (ui/nav-page {:id :home} "Home")
                            (ui/nav-page {:id :detail} "Detail"))]
    (is (false? (:reuse-forward fresh)))
    (is (= 2 (:replace-generation fresh))))
  (let [recipe (ui/nav-stack {:id "nav" :stack [:home]
                              :item [{:phase :entering :operation [:push :replace]
                                      :left {:from 1 :to 0}
                                      :opacity {:from 0.35 :to 1}
                                      :padding 8
                                      :align :center
                                      :bg :#111111}
                                     {:phase :exiting :operation :pop
                                      :left {:from 0 :to 1}}]}
                             (ui/nav-page {:id :home} "Home"))]
    (is (= "entering" (get-in recipe [:item :match 0 :phase])))
    (is (= ["push" "replace"] (get-in recipe [:item :match 0 :operation])))
    (is (= {:from 1 :to 0} (get-in recipe [:item :match 0 :left])))
    (is (= {:from 0.35 :to 1} (get-in recipe [:item :match 0 :opacity])))
    (is (= 8 (get-in recipe [:item :match 0 :padding])))
    (is (= "center" (get-in recipe [:item :match 0 :align])))
    (is (= "#111111" (get-in recipe [:item :match 0 :bg])))
    (is (= "pop" (get-in recipe [:item :match 1 :operation]))))
  (let [named (ui/nav-stack {:id "nav" :item :slide}
                            (ui/nav-page {:id :home} "Home"))]
    (is (= "slide" (:item named))))
  (let [dropped (ui/nav-stack {:id "nav" :item (fn [_])}
                              (ui/nav-page {:id :home} "Home"))]
    (is (false? (:item dropped))))
  (let [unknown-slide (ui/nav-stack {:id "nav"
                                     :item :fade
                                     :transition-style :slide}
                                    (ui/nav-page {:id :home} "Home"))]
    (is (= "fade" (:item unknown-slide)))
    (is (= "slide" (:transition-style unknown-slide))))
  (let [fn-slide (ui/nav-stack {:id "nav"
                                :item (fn [_])
                                :transition-style :slide}
                               (ui/nav-page {:id :home} "Home"))]
    (is (false? (:item fn-slide)))
    (is (= "slide" (:transition-style fn-slide))))
  (let [bools (ui/nav-stack {:id "nav"
                             :item {:shadow true
                                    :strikethrough true
                                    :match [{:phase :present
                                             :shadow false
                                             :strikethrough false}]}}
                            (ui/nav-page {:id :home} "Home"))]
    (is (true? (get-in bools [:item :shadow])))
    (is (true? (get-in bools [:item :strikethrough])))
    (is (false? (get-in bools [:item :match 0 :shadow])))
    (is (false? (get-in bools [:item :match 0 :strikethrough])))))

(deftest nav-stack-forward-change-restores-page-ids
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/nav-stack {:id "nav" :stack [:home]
                                 :on-forward-change #(reset! got %)}
                                (ui/nav-page {:id :home} "Home")
                                (ui/nav-page {:id :detail} "Detail")
                                (ui/nav-page {:id :settings} "Settings")))]
    (is (string? (:on-forward-change exported)))
    (is (= {:ok true :id (:on-forward-change exported)}
           (runtime/invoke-callback! (:on-forward-change exported)
                                     ["detail" "settings"])))
    (is (= [:detail :settings] @got))))

(deftest nav-stack-item-recipe-is-not-a-callback
  (runtime/reset-callbacks!)
  (let [exported (runtime/export-tree
                  (ui/nav-stack {:id "nav" :stack [:home]
                                 :item [{:phase :entering :operation :push
                                         :left {:from 1 :to 0}}]}
                                (ui/nav-page {:id :home} "Home")))]
    (is (map? (:item exported)))
    (is (not (string? (:item exported))))
    (is (= "entering" (get-in exported [:item :match 0 :phase])))
    (is (= "push" (get-in exported [:item :match 0 :operation])))
    (is (= {:from 1 :to 0}
           (get-in exported [:item :match 0 :left]))))
  (let [unknown (runtime/export-tree
                 (ui/nav-stack {:id "nav" :stack [:home]
                                :item :fade
                                :transition-style :slide}
                               (ui/nav-page {:id :home} "Home")))]
    (is (= "fade" (:item unknown)))
    (is (= "slide" (:transition-style unknown)))
    (is (not (str/starts-with? (str (:item unknown)) "cb-"))))
  (let [dropped (runtime/export-tree
                 (ui/nav-stack {:id "nav" :stack [:home]
                                :item (fn [_])
                                :transition-style :slide}
                               (ui/nav-page {:id :home} "Home")))]
    (is (false? (:item dropped)))
    (is (= "slide" (:transition-style dropped)))
    (is (not (string? (:item dropped))))))

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
                            {:on-change #(reset! got %)})
                   (ui/native-menu [{:id :copy :label "Copy"}]
                                   {:id "edit-menu"
                                    :open? true
                                    :on-change #(reset! got %)
                                    :on-open-change #(reset! got %)})
                   (ui/command [{:id :find :label "Find"}]
                               {:id "palette"
                                :on-change #(reset! got %)
                                :on-select #(reset! got [:select %])
                                :on-confirm #(reset! got [:confirm %])
                                :on-query #(reset! got %)
                                :on-cancel #(reset! got :cancel)})
                   (ui/status-bar {:left (ui/button "Ln" #(reset! got :left))}
                                  (ui/label "Ready"))))
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
    (is (= :lib @got))
    (is (string? (get-in children [6 :on-change])))
    (is (string? (get-in children [6 :on-open-change])))
    (is (= {:ok true :id (get-in children [6 :on-change])}
           (runtime/invoke-callback! (get-in children [6 :on-change]) "copy")))
    (is (= :copy @got))
    (is (string? (get-in children [7 :on-change])))
    (is (string? (get-in children [7 :on-select])))
    (is (string? (get-in children [7 :on-confirm])))
    (is (string? (get-in children [7 :on-query])))
    (is (string? (get-in children [7 :on-cancel])))
    (is (= {:ok true :id (get-in children [7 :on-select])}
           (runtime/invoke-callback! (get-in children [7 :on-select]) "find")))
    (is (= [:select :find] @got))
    (is (= {:ok true :id (get-in children [7 :on-confirm])}
           (runtime/invoke-callback! (get-in children [7 :on-confirm]) "find")))
    (is (= [:confirm :find] @got))
    (is (= {:ok true :id (get-in children [7 :on-query])}
           (runtime/invoke-callback! (get-in children [7 :on-query]) "fi")))
    (is (= "fi" @got))
    (is (= {:ok true :id (get-in children [7 :on-cancel])}
           (runtime/invoke-callback! (get-in children [7 :on-cancel]))))
    (is (= :cancel @got))
    (is (string? (get-in children [8 :left 0 :on-click])))
    (is (= {:ok true :id (get-in children [8 :left 0 :on-click])}
           (runtime/invoke-callback! (get-in children [8 :left 0 :on-click]))))
    (is (= :left @got))))

(deftest data-table-cell-and-export-callbacks
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/data-table
                   {:columns [{:id :name :label "Name"} {:id :lang :label "Lang"}]
                    :rows [{:id :ada :cells ["Ada" "Clojure"]}]
                    :cell-selectable true
                    :on-change #(reset! got %)
                    :on-confirm #(reset! got [:confirm %])
                    :on-export #(reset! got %)}))]
    (is (string? (:on-change exported)))
    (is (string? (:on-export exported)))
    (is (= {:ok true :id (:on-change exported)}
           (runtime/invoke-callback! (:on-change exported) {:row "ada" :col "lang"})))
    (is (= {:row :ada :col :lang} @got))
    (is (= {:ok true :id (:on-confirm exported)}
           (runtime/invoke-callback! (:on-confirm exported) ["ada" "lang"])))
    (is (= [:confirm {:row :ada :col :lang}] @got))
    (is (= {:ok true :id (:on-export exported)}
           (runtime/invoke-callback! (:on-export exported)
                                     {:headers ["Name" "Lang"]
                                      :rows [["Ada" "Clojure"]]})))
    (is (= {:headers ["Name" "Lang"] :rows [["Ada" "Clojure"]]} @got))
    (is (= {:ok true :id (:on-change exported)}
           (runtime/invoke-callback! (:on-change exported) "ada")))
    (is (= :ada @got))))

(deftest data-table-row-and-column-ids-are-separate-namespaces
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/data-table
                   {:columns [{:id "lang" :label "Lang"}]
                    :rows [{:id :lang :cells ["Clojure"]}]
                    :cell-selectable true
                    :on-change #(reset! got %)}))]
    (is (= {:ok true :id (:on-change exported)}
           (runtime/invoke-callback! (:on-change exported)
                                     {:row "lang" :col "lang"})))
    (is (= {:row :lang :col "lang"} @got))
    (is (= {:ok true :id (:on-change exported)}
           (runtime/invoke-callback! (:on-change exported) ["lang" "lang"])))
    (is (= {:row :lang :col "lang"} @got))
    (is (= {:ok true :id (:on-change exported)}
           (runtime/invoke-callback! (:on-change exported) "lang")))
    (is (= :lang @got))))

(deftest data-table-widget-cell-callbacks-sanitize
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        exported (runtime/export-tree
                  (ui/data-table
                   {:columns [{:id :name :label "Name"} {:id :act :label "Act"}]
                    :rows [{:id :ada
                            :cells ["Ada" (ui/button "Ping" #(reset! got :ping))]}]}))
        click (get-in exported [:items 0 :cells 1 :on-click])]
    (is (= "Ada" (get-in exported [:items 0 :cells 0])))
    (is (= "button" (get-in exported [:items 0 :cells 1 :type])))
    (is (string? click))
    (is (= {:ok true :id click} (runtime/invoke-callback! click)))
    (is (= :ping @got))))

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
