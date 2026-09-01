(ns catppuccin-violet.app
  "Custom gpui-component theme defined in Clojure, selected by :theme name."
  (:require [catppuccin-violet.theme]
            [gpui.ratom :as r]
            [gpui.ui :as ui]))

(def family "Catppuccin Violet")
(def light-name "Catppuccin Violet Light")
(def dark-name "Catppuccin Violet Dark")

(defonce !state
  (r/atom {:choice family
           :draft ""
           :done? false}))

(defn- select-theme
  [name]
  #(swap! !state assoc :choice name))

(defn app []
  (let [{:keys [choice draft done?]} @!state]
    (ui/window
     {:title "Catppuccin Violet"
      :chrome :dev
      :width 520
      :height 520
      :theme choice}
     (ui/vstack
      {:gap 14 :padding 20 :flex 1}
      (ui/label "Catppuccin Violet" {:font-size 22 :font-weight :semibold})
      (ui/label "A gpui-component ThemeSet as JVM Clojure maps. :theme is only a name.")
      (ui/label (str "Active: " choice) {:font-size 13})
      (ui/hstack
       {:gap 8}
       (ui/button "System pair" (select-theme family)
                  {:primary (= choice family)})
       (ui/button "Light" (select-theme light-name)
                  {:primary (= choice light-name)})
       (ui/button "Dark" (select-theme dark-name)
                  {:primary (= choice dark-name)}))
      (ui/hstack
       {:gap 10 :align :center}
       (ui/button "Primary" (fn []) {:primary true})
       (ui/checkbox done? #(swap! !state update :done? not) "Checkbox"))
      (ui/text-field
       draft
       {:id "note"
        :placeholder "Themed text field"
        :on-change #(swap! !state assoc :draft %)})
      (ui/label "Family name follows OS light/dark. Light and Dark pin one member."
                {:font-size 12})))))
