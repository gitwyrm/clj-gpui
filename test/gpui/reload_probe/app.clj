(ns gpui.reload-probe.app
  (:require [gpui.reload-probe.widgets :as w]
            [gpui.ui :as ui]))

(defonce !state (atom {:n 0}))

(defn app
  []
  (ui/label (str (w/banner) "-" (:n @!state))))
