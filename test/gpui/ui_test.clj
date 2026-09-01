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
    (is (= :ghost (:variant (ui/button "All" (fn []) {:variant :ghost}))))))

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
