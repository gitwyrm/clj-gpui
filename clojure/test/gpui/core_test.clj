(ns gpui.core-test
  (:require [clojure.test :refer [deftest is testing]]
            [gpui.core :as ui]))

(deftest label-produces-data
  (is (= {:type :label :text "Hello from Clojure"}
         (ui/label "Hello from Clojure")))
  (is (= :bold (:font-weight (ui/label "Hi" {:font-weight :bold})))))

(deftest button-keeps-real-functions
  (let [f #(inc 1)
        node (ui/button "+" f)]
    (is (= :button (:type node)))
    (is (fn? (:on-click node)))
    (is (= 2 ((:on-click node))))))

(deftest sequences-flatten-inside-stacks
  (let [tree (ui/vstack
              (ui/label "Todos")
              (map ui/label ["Clojure" "Rust" "GPUI"])
              (when false (ui/label "hidden"))
              (when true (ui/label "visible")))]
    (is (= :vstack (:type tree)))
    (is (= ["Todos" "Clojure" "Rust" "GPUI" "visible"]
           (mapv :text (:children tree))))))

(deftest nested-function-components
  (letfn [(item-view [s] (ui/label (str "• " s)))]
    (is (= ["• a" "• b"]
           (->> (ui/vstack (map item-view ["a" "b"]))
                :children
                (mapv :text))))))
