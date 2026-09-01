(ns gpui.test-runner
  (:require [clojure.test :as t]
            [gpui.core-test]
            [gpui.ratom-test]
            [gpui.ui-test]))

(defn -main [& _]
  (let [{:keys [fail error]} (t/run-all-tests #"gpui\..*-test")]
    (System/exit (if (pos? (+ fail error)) 1 0))))
