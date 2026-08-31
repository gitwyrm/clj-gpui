(ns gpui.ratom-test
  (:require [clojure.test :refer [deftest is]]
            [gpui.core :as ui]
            [gpui.ratom :as r]))

(deftest ratom-is-a-real-clojure-atom
  (let [a (r/atom 0)]
    (is (instance? clojure.lang.Atom a))
    (is (r/atom? a))
    (is (not (r/atom? (atom 0))))
    (is (zero? @a))
    (is (= 1 (swap! a inc)))
    (is (= 7 (reset! a 7)))))

(deftest ratom-accepts-atom-options
  (let [a (r/atom 1 :meta {:from :test} :validator pos?)]
    (is (= {:from :test} (meta a)))
    (is (= 2 (swap! a inc)))))

(deftest watch-existing-atom
  (let [a (atom :ready)]
    (is (not (r/atom? a)))
    (ui/watch! a)
    (is (r/atom? a))))
