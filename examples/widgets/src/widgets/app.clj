(ns widgets.app
  "Gallery of gpui-component widgets newly exposed through gpui.ui.

  Real r/atom state so controls can be dogfooded as a smoke test.
  Option ids are keywords on purpose: callbacks must round-trip them."
  (:require [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state
  (r/atom {:notify? true
           :bold? false
           :theme-mode :dark
           :volume 36
           :lang :clj
           :tab :general
           :section :audio
           :crumb :home
           :alert? true}))

(defn- set-key [k]
  (fn [v]
    (swap! !state assoc k v)))

(defn- general-panel [{:keys [notify? bold? theme-mode volume lang]}]
  (ui/vstack
   {:gap 12}
   (ui/hstack
    {:gap 16 :align :center}
    (ui/switch notify? (set-key :notify?) "Notifications")
    (ui/toggle bold? {:on-change (set-key :bold?) :text "Bold"}))
   (ui/radio-group theme-mode
                   {:options [{:id :light :label "Light"}
                              {:id :dark :label "Dark"}]
                    :orientation :horizontal
                    :on-change (set-key :theme-mode)})
   (ui/hstack
    {:gap 12 :align :center}
    (ui/label (str "Volume " volume))
    (ui/slider volume {:id "volume"
                       :min 0 :max 100 :flex 1
                       :tooltip "0–100"
                       :on-change (set-key :volume)}))
   (ui/progress volume {:tooltip "Mirrors the slider"})
   (ui/hstack
    {:gap 8 :align :center}
    (ui/select lang
               {:options [{:id :clj :label "Clojure"}
                          {:id :rs :label "Rust"}
                          {:id :go :label "Go"}]
                :placeholder "Language"
                :searchable true
                :flex 1
                :on-change (set-key :lang)})
    (ui/button "Clear" #(swap! !state assoc :lang nil)))
   (ui/hstack
    {:gap 8 :align :center}
    (ui/tag (if lang (name lang) "none") {:variant :info})
    (ui/kbd "ctrl-s")
    (ui/button "Save" #(swap! !state assoc :alert? true)
               {:primary true :tooltip "Write the current settings"})
    (ui/link "https://clojure.org" "clojure.org"))))

(defn- chrome-panel [{:keys [section crumb alert?]}]
  (ui/vstack
   {:gap 12}
   (ui/breadcrumb
    [{:id :home :label "Home"}
     {:id :project :label "Project"}
     {:label "Widgets"}]
    {:on-change (set-key :crumb)})
   (ui/label (str "Crumb " (pr-str crumb)))
   (ui/hstack
    {:gap 12 :align :center}
    (ui/badge 3 (ui/icon :bell))
    (ui/badge {:dot true} (ui/icon :inbox))
    (ui/avatar "Ada Lovelace")
    (ui/clipboard "clj-gpui" {:on-copied (fn [_]
                                           (swap! !state assoc :alert? true))})
    (ui/spinner {:size :small}))
   (when alert?
     (ui/alert "Copied to the clipboard."
               {:variant :success
                :title "Done"
                :on-close #(swap! !state assoc :alert? false)}))
   (ui/accordion section
                 {:on-change (set-key :section)
                  :items [{:id :audio
                           :title "Audio"
                           :content (ui/label "Speakers, mic, and volume.")}
                          {:id :display
                           :title "Display"
                           :content (ui/label "Theme, density, and motion.")}]})
   (ui/description-list [{:label "Host" :value "GPUI"}
                         {:label "UI" :value "clj-gpui"}])
   (ui/hstack {:gap 12 :align :center :height 36}
              (ui/label "v")
              (ui/divider {:orientation :vertical :height 28})
              (ui/label "h"))
   (ui/divider)
   (ui/skeleton {:width 220 :height 12})))

(defn app []
  (let [{:keys [tab] :as state} @!state]
    (ui/window
     {:title "Widgets"
      :chrome :dev
      :width 560
      :height 760
      :theme "Tokyo Night"}
     (ui/vstack
      {:gap 14 :padding 16 :flex 1}
      (ui/label "gpui.ui widgets" {:font-size 22 :font-weight :semibold})
      (ui/label "Controlled Clojure state. Native gpui-component widgets."
                {:font-size 13})
      (ui/tabs tab
               {:items [{:id :general :label "General"}
                        {:id :chrome :label "Chrome"}]
                :variant :underline
                :on-change (set-key :tab)})
      (ui/scroll
       {:flex 1}
       (ui/vstack
        {:gap 14 :padding 4}
        (case tab
          :chrome (chrome-panel state)
          (general-panel state))))))))
