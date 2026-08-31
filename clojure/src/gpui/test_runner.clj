(ns gpui.test-runner
  (:require [clojure.test :as t]))

(defn -main
  [& _]
  (let [_ (require 'gpui.core-test)
        summary (t/run-tests 'gpui.core-test)]
    (System/exit (if (t/successful? summary) 0 1))))
