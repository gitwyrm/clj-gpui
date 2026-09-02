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
           :overlay-lock? false
           :tick 0
           :popover? false
           :menu nil
           :list-sel :alpha
           :list-confirm nil
           :table-sel :ada
           :table-confirm nil
           :table-shift 0
           :tree-sel :src
           :list-rev 0
           :batch-shift? false
           :close-hit false
           :dialog-open nil}))

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

(defn- overlay-panel [{:keys [dialog? popover? menu overlay-lock? tick batch-shift? close-hit dialog-open]}]
  (ui/vstack
   {:gap 12}
   (ui/label (str "Menu " (pr-str menu)
                  " · tick " tick
                  " · close-hit " (pr-str close-hit)
                  " · dialog-open " (pr-str dialog-open)
                  " · shift " (pr-str batch-shift?)))
   (when batch-shift?
     [(ui/button "shifted-a" (fn []))
      (ui/button "shifted-b" (fn []))])
   (ui/hstack
    {:gap 8 :align :center}
    (ui/button "Open dialog" #(swap! !state assoc :dialog? true :close-hit false :dialog-open true) {:primary true})
    (ui/button "Rerender" #(swap! !state update :tick inc)
               {:tooltip "Unrelated atom update while a dialog stays open"})
    (ui/switch overlay-lock? (set-key :overlay-lock?) "Lock overlay")
    (ui/popover popover?
                {:trigger (ui/button "Popover")
                 :on-open-change (set-key :popover?)}
                (ui/label "Anchored content.")
                (ui/button "Close" #(swap! !state assoc :popover? false) {:variant :ghost}))
    (ui/dropdown-menu
     [{:id :copy :label "Copy"
       :on-click #(swap! !state assoc :batch-shift? true)}
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
              {:title (str "Confirm · tick " tick)
               :variant :confirm
               :overlay-closable (not overlay-lock?)
               :on-ok #(swap! !state assoc :dialog? false :menu :ok :batch-shift? true)
               :on-cancel #(swap! !state assoc :dialog? false :menu :cancel)
               :on-close #(swap! !state assoc :dialog? false :close-hit true)
               :on-open-change (set-key :dialog-open)}
              (ui/label (str "Close from OK, Cancel, Escape, or the overlay. Tick " tick "."))
              (ui/button "Disabled" {:disabled true}))))

(defn- data-panel [{:keys [list-sel list-confirm table-sel table-confirm table-shift tree-sel list-rev batch-shift?]}]
  (let [suffix (when (pos? list-rev) (str " · " list-rev))
        list-items [{:id :alpha :label (str "Alpha" suffix)}
                    {:id :beta :label (str "Beta" suffix)}
                    {:id :gamma :label (str "Gamma" suffix)}
                    {:id :delta :label (str "Delta" suffix)}]
        list-items (if (= list-sel :gone)
                     (vec (remove #(= :alpha (:id %)) list-items))
                     list-items)
        table-rows [{:id :ada :cells ["Ada" "Clojure"]}
                    {:id :grace :cells ["Grace" "Rust"]}
                    {:id :alan :cells ["Alan" "Go"]}]
        table-rows (if (= table-sel :gone)
                     (vec (remove #(= :ada (:id %)) table-rows))
                     table-rows)
        tree-items (if (= tree-sel :gone)
                     [{:id :src :label "src" :expanded true
                       :items [{:id :main :label "main.rs"}]}
                      {:id :readme :label "README.md"}]
                     [{:id :src :label "src" :expanded true
                       :items [{:id :lib :label "lib.rs"}
                               {:id :main :label "main.rs"}]}
                      {:id :readme :label "README.md"}])]
    (ui/vstack
     {:gap 12}
     (ui/label (str "List " (pr-str list-sel)
                    " confirm " (pr-str list-confirm)
                    " · table " (pr-str table-sel)
                    " confirm " (pr-str table-confirm)
                    " · tree " (pr-str tree-sel)
                    " · shift " (pr-str batch-shift?)
                    " · tbl-shift " table-shift))
     (when batch-shift?
       [(ui/button "shifted-a" (fn []))
        (ui/button "shifted-b" (fn []))])
     ;; Fixed-height slot so inserting 0-arg canaries shifts callback ids
     ;; without moving the table (needed for a real click_count=2).
     (ui/scroll
      {:height 52}
      (if (pos? (or table-shift 0))
        (mapcat (fn [i]
                  [(ui/button (str "tbl-canary-" i "-a") (fn []))
                   (ui/button (str "tbl-canary-" i "-b") (fn []))])
                (range table-shift))
        (ui/label "tbl-canary slot")))
     (ui/hstack
      {:gap 8 :align :center}
      (ui/button "List A→B" #(swap! !state assoc :list-sel :beta))
      (ui/button "List nil" #(swap! !state assoc :list-sel nil))
      (ui/button "Drop row" #(swap! !state assoc :list-sel :gone))
      (ui/button "Mutate labels" #(swap! !state update :list-rev inc)))
     (ui/list list-items
              {:selected (when (not= list-sel :gone) list-sel)
               :searchable true
               :height 160
               :on-change (fn [id]
                            (swap! !state assoc :list-sel id :batch-shift? true))
               :on-confirm (set-key :list-confirm)})
     (ui/hstack
      {:gap 8 :align :center}
      (ui/button "Table A→B" #(swap! !state assoc :table-sel :grace))
      (ui/button "Table nil" #(swap! !state assoc :table-sel nil))
      (ui/button "Drop Ada" #(swap! !state assoc :table-sel :gone)))
     (ui/table {:columns [{:id :name :label "Name" :width (if (pos? list-rev) 180 140)}
                          {:id :lang :label "Lang" :width 100}]
                :rows table-rows
                :selected (when (not= table-sel :gone) table-sel)
                :height 160
                :on-change (fn [id]
                             (swap! !state #(-> %
                                                (assoc :table-sel id)
                                                (update :table-shift (fnil inc 0)))))
                :on-confirm (set-key :table-confirm)})
     (ui/hstack
      {:gap 8 :align :center}
      (ui/button "Tree lib" #(swap! !state assoc :tree-sel :lib))
      (ui/button "Tree nil" #(swap! !state assoc :tree-sel nil))
      (ui/button "Drop lib" #(swap! !state assoc :tree-sel :gone)))
     (ui/tree tree-items
              {:selected (when (not= tree-sel :gone) tree-sel)
               :height 160
               :on-change (set-key :tree-sel)}))))

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
