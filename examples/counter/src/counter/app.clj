(ns counter.app
  "Example application: ordinary JVM Clojure rendered by native GPUI."
  (:require [counter.helpers :as helpers]
            [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state
  (r/atom {:count 0
           :show-completed true
           :items [{:id 1 :title "Ship a native window" :done true}
                   {:id 2 :title "Call real Clojure from a click" :done true}
                   {:id 3 :title "Reload without cargo" :done false}]}))

(defn- toggle-item [id]
  (swap! !state update :items
         (fn [items]
           (mapv (fn [item]
                   (if (= id (:id item))
                     (update item :done not)
                     item))
                 items))))

(defn- add-item []
  (swap! !state update :items
         (fn [items]
           (conj items {:id (inc (apply max 0 (map :id items)))
                        :title (str "REPL item " (inc (clojure.core/count items)))
                        :done false}))))

(defn- visible-items [{:keys [items show-completed]}]
  (if show-completed items (helpers/incomplete items)))

(defn app []
  (let [{:keys [count show-completed items] :as state} @!state
        shown (visible-items state)]
    (ui/scroll
     {}
     (ui/vstack
      {:gap 14 :padding 18}
      (ui/label "Clojure + native GPUI" {:font-size 22 :font-weight :semibold})
      (ui/label "Real JVM Clojure. Native window. No webview."
                {:font-size 13 :color "#9aa4b2"})
      (ui/hstack
       {:gap 12}
       (ui/label (str "Count: " count) {:font-size 18 :font-weight :semibold})
       (ui/button "−" #(swap! !state update :count dec))
       (ui/button "+" #(swap! !state update :count inc))
       (ui/button "Reset" #(swap! !state assoc :count 0)))
      (ui/hstack
       {:gap 12}
       (ui/button "Add item" add-item)
       (ui/checkbox show-completed
                    #(swap! !state update :show-completed not)
                    "Show completed")
       (ui/label (str (clojure.core/count shown) " / " (clojure.core/count items))
                 {:font-size 13 :color "#9aa4b2"}))
      (when (empty? shown)
        (ui/label "Nothing to show. Toggle completed or add an item."
                  {:font-size 13 :color "#9aa4b2"}))
      (apply ui/vstack
             {:gap 8}
             (map-indexed
              (fn [i {:keys [id title done]}]
                (ui/checkbox
                 done
                 #(toggle-item id)
                 (helpers/bullet title)
                 {:font-weight (if done :regular :semibold)
                  :color (if done "#7a8194" "#c0caf5")}))
              shown))))))
