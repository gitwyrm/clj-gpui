(ns counter.app
  "Example application: a plain counter in real JVM Clojure."
  (:require [counter.widgets :as widgets]
            [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state (r/atom {:count 0}))

(defn app []
  (let [{:keys [count]} @!state]
    (ui/window
     {:title "Counter"
      :chrome :dev
      :width 440
      :height 400
      :theme :dark}
     (ui/vstack
      {:gap 16 :padding 16 :flex 1}
      (ui/label "Counter" {:font-size 22 :font-weight :semibold})
      (widgets/subtitle)
      (ui/hstack
       {:gap 12}
       (ui/button "−" #(swap! !state update :count dec))
       (ui/label (str "Count: " count) {:font-size 28 :font-weight :bold})
       (ui/button "+" #(swap! !state update :count inc) {:primary true}))
      (ui/button "Reset" #(swap! !state assoc :count 0))))))
