(ns my.app
  (:require [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state (r/atom {:n 0}))

(defn app []
  (let [{:keys [n]} @!state]
    (ui/window
     {:title "My App" :chrome :dev :theme :system}
     (ui/vstack
      {:gap 12 :padding 16}
      (ui/label "Hello from Clojure" {:font-size 20 :font-weight :semibold})
      (ui/hstack
       {:gap 10}
       (ui/label (str "Clicks: " n) {:font-size 16})
       (ui/button "Click" #(swap! !state update :n inc)))))))
