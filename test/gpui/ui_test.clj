(ns gpui.ui-test
  (:require [clojure.test :refer [deftest is testing]]
            [gpui.runtime :as runtime]
            [gpui.ui :as ui]))

(deftest window-title
  (is (= "clj-gpui" ui/window-title)))

(deftest protocol-version
  (is (= 1 ui/protocol-version)))

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
    (is (true? (:focus (ui/text-field "" {:focus true}))))))

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
    (is (= "system" (:theme exported)))))

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
