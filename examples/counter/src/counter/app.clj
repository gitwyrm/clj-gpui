(ns counter.app
  "Example application: a plain counter in real JVM Clojure."
  (:require [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state (r/atom {:count 0}))

(defn app []
  (let [{:keys [count]} @!state]
    (ui/vstack
     {:title "Counter"
      :chrome :dev
      :window-width 440
      :window-height 400
      :theme :dark
      :gap 16
      :padding 16
      :flex 1}
     (ui/label "Counter" {:font-size 22 :font-weight :semibold})
     (ui/label "Real JVM Clojure. Native GPUI window. No webview."
               {:font-size 13 :color "#9aa4b2"})
     (ui/hstack
      {:gap 12}
      (ui/button "−" #(swap! !state update :count dec))
      (ui/label (str "Count: " count) {:font-size 28 :font-weight :bold})
      (ui/button "+" #(swap! !state update :count inc) {:primary true}))
     (ui/button "Reset" #(swap! !state assoc :count 0)))))
