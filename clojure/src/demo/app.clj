(ns demo.app
  "Demo application written in ordinary Clojure, rendered by native GPUI."
  (:require [demo.helpers :as helpers]
            [gpui.core :as ui]
            [gpui.ratom :as r]))

(defonce state
  (r/atom {:count 0
           :show-done? true
           :items [{:title "Write UI in real Clojure" :done true}
                   {:title "Render it with native GPUI" :done true}
                   {:title "Click a Clojure function from Rust" :done false}
                   {:title "Reload without recompiling the host" :done false}]}))

(defn increment!
  []
  (swap! state update :count inc))

(defn decrement!
  []
  (swap! state update :count dec))

(defn add-item!
  []
  (swap! state update :items conj
         {:title (str "REPL item " (inc (count (:items @state))))
          :done false}))

(defn toggle-item!
  [idx]
  (swap! state update-in [:items idx :done] not))

(defn item-view
  "Nested component: a real Clojure function returning UI data."
  [idx {:keys [title done]}]
  (when (or (:show-done? @state) (not done))
    (ui/checkbox
     done
     #(toggle-item! idx)
     (helpers/bullet title)
     {:font-weight (if done :normal :semibold)
      :color (if done "#7a8194" "#c0caf5")})))

(defn counter-controls
  []
  (ui/hstack
   {:gap 8}
   (ui/button "−" decrement!)
   (ui/button "+" increment!)
   (ui/button "Add item" add-item!)))

(defn app
  []
  (let [{:keys [count items show-done?]} @state
        remaining (clojure.core/count (remove :done items))]
    (ui/vstack
     {:gap 12 :padding 8}
     (ui/label "ClojureGPUI" {:font-size 22 :font-weight :bold})
     (ui/label "Ordinary Clojure driving a native GPUI window. Edit this file to hot reload."
               {:color "#9aa3b5"})

     (ui/label (str "Count: " count)
               {:font-size 18 :font-weight :semibold})
     (counter-controls)

     (when (helpers/lots? count)
       (ui/label "That's a lot!" {:color "#e0af68" :font-weight :semibold}))

     (ui/hstack
      {:gap 8}
      (ui/label (str remaining " remaining") {:color "#9aa3b5"})
      (ui/spacer)
      (ui/checkbox show-done?
                   #(swap! state update :show-done? not)
                   "Show completed"))

     (ui/scroll
      {:height 220}
      (ui/vstack
       {:gap 8}
       (map-indexed item-view items))))))
