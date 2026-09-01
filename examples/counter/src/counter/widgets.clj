(ns counter.widgets
  (:require [gpui.ui :as ui]))

(defn subtitle
  []
  (ui/label "Real JVM Clojure. Native GPUI window. No webview."
            {:font-size 13 :color "#9aa4b2"}))
