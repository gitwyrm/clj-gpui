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
           :alert? true
           :dialog? false
           :popover? false
           :menu nil
           :list-sel :alpha
           :table-sel :ada
           :tree-sel :src}))

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

(defn- overlay-panel [{:keys [dialog? popover? menu]}]
  (ui/vstack
   {:gap 12}
   (ui/label (str "Menu " (pr-str menu)))
   (ui/hstack
    {:gap 8 :align :center}
    (ui/button "Open dialog" #(swap! !state assoc :dialog? true) {:primary true})
    (ui/popover popover?
                {:trigger (ui/button "Popover")
                 :on-open-change (set-key :popover?)}
                (ui/label "Anchored content.")
                (ui/button "Close" #(swap! !state assoc :popover? false)))
    (ui/dropdown-menu
     [{:id :copy :label "Copy"}
      :-
      {:id :share :label "Share"
       :items [{:id :email :label "Email"}
               {:id :link :label "Copy link"}]}]
     {:on-change (set-key :menu)}
     (ui/button "Edit")))
   (ui/context-menu
    [{:id :inspect :label "Inspect"}
     {:id :delete :label "Delete"}]
    {:on-change (set-key :menu)}
    (ui/label "Right-click this label."))
   (ui/dialog dialog?
              {:title "Confirm"
               :variant :confirm
               :on-ok #(swap! !state assoc :dialog? false :menu :ok)
               :on-cancel #(swap! !state assoc :dialog? false)
               :on-close #(swap! !state assoc :dialog? false)}
              (ui/label "Close from OK, Cancel, or the overlay."))))

(defn- data-panel [{:keys [list-sel table-sel tree-sel]}]
  (ui/vstack
   {:gap 12}
   (ui/label (str "List " (pr-str list-sel)
                  " · table " (pr-str table-sel)
                  " · tree " (pr-str tree-sel)))
   (ui/list [{:id :alpha :label "Alpha"}
             {:id :beta :label "Beta"}
             {:id :gamma :label "Gamma"}
             {:id :delta :label "Delta"}]
            {:selected list-sel
             :searchable true
             :height 160
             :on-change (set-key :list-sel)})
   (ui/table {:columns [{:id :name :label "Name" :width 140}
                        {:id :lang :label "Lang" :width 100}]
              :rows [{:id :ada :cells ["Ada" "Clojure"]}
                     {:id :grace :cells ["Grace" "Rust"]}
                     {:id :alan :cells ["Alan" "Go"]}]
              :selected table-sel
              :height 160
              :on-change (set-key :table-sel)})
   (ui/tree [{:id :src :label "src" :expanded true
              :items [{:id :lib :label "lib.rs"}
                      {:id :main :label "main.rs"}]}
             {:id :readme :label "README.md"}]
            {:selected tree-sel
             :height 160
             :on-change (set-key :tree-sel)})))

(defn app []
  (let [{:keys [tab] :as state} @!state]
    (ui/window
     {:title "Widgets"
      :chrome :dev
      :width 620
      :height 820
      :theme "Tokyo Night"}
     (ui/vstack
      {:gap 14 :padding 16 :flex 1}
      (ui/label "gpui.ui widgets" {:font-size 22 :font-weight :semibold})
      (ui/label "Controlled Clojure state. Native gpui-component widgets."
                {:font-size 13})
      (ui/tabs tab
               {:items [{:id :general :label "General"}
                        {:id :chrome :label "Chrome"}
                        {:id :overlay :label "Overlay"}
                        {:id :data :label "Data"}]
                :variant :underline
                :on-change (set-key :tab)})
      (ui/scroll
       {:flex 1}
       (ui/vstack
        {:gap 14 :padding 4}
        (case tab
          :chrome (chrome-panel state)
          :overlay (overlay-panel state)
          :data (data-panel state)
          (general-panel state))))))))
