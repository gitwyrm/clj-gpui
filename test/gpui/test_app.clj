(ns gpui.test-app
  "Minimal app used by `:protocol-test`. Keep labels stable: clj-gpui, Count, +."
  (:require [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state (r/atom {:count 0}))

(defn app []
  (let [{:keys [count]} @!state]
    (ui/vstack
     {:gap 12 :padding 16}
     (ui/label "clj-gpui" {:font-size 18 :font-weight :semibold})
     (ui/hstack
      {:gap 12}
      (ui/label (str "Count: " count) {:font-size 16})
      (ui/button "+" #(swap! !state update :count inc))))))
