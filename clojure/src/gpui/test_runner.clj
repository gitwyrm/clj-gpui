(ns gpui.test-runner
  (:require [clojure.test :as t]))

(defn -main
  [& _]
  (let [_ (require 'gpui.core-test 'gpui.ratom-test)
        summary (t/run-tests 'gpui.core-test 'gpui.ratom-test)]
    (System/exit (if (t/successful? summary) 0 1))))
