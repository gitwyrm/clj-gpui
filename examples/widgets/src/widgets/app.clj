(ns widgets.app
  "Gallery of GPUI Kit widgets exposed through gpui.ui.

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
           :dialog-open nil
           :sheet? false
           :toasts []
           :sticky-toast? false
           :toast-hit nil
           :qty 4
           :otp ""
           :color "#3366ff"
           :date "2026-09-02"
           :src "(defn hi [] \n  :ok)"
           :notes "Multi-line notes."
           :alert-dialog? false
           :field-kind :number
           :field-val 4
           :combo :clj
           :combo-multi [:clj]
           :stars 3
           :step :pay
           :vlist-sel :r0
           :nav :home
           :sidebar-collapsed false
           :setting-notify true
           :setting-theme :dark
           :setting-accent :blue
           :split-id "split-a"
           :split-sizes nil}))

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
              (ui/separator {:orientation :vertical :height 28})
              (ui/label "h"))
   (ui/separator)
   (ui/skeleton {:width 220 :height 12})))

(defn- overlay-panel [{:keys [dialog? alert-dialog? popover? menu overlay-lock? tick batch-shift? close-hit dialog-open sheet? toasts sticky-toast? toast-hit]}]
  (ui/vstack
   {:gap 12}
   (ui/label (str "Menu " (pr-str menu)
                  " · tick " tick
                  " · close-hit " (pr-str close-hit)
                  " · dialog-open " (pr-str dialog-open)
                  " · shift " (pr-str batch-shift?)
                  " · toast-hit " (pr-str toast-hit)))
   (when batch-shift?
     [(ui/button "shifted-a" (fn []))
      (ui/button "shifted-b" (fn []))])
   (ui/hstack
    {:gap 8 :align :center}
    (ui/button "Open dialog" #(swap! !state assoc :dialog? true :close-hit false :dialog-open true) {:primary true})
    (ui/button "Open alert" #(swap! !state assoc :alert-dialog? true))
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
              (ui/button "Disabled" {:disabled true}))
   (ui/alert-dialog alert-dialog?
                    {:title "Alert"
                     :variant :confirm
                     :on-ok #(swap! !state assoc :alert-dialog? false :menu :alert-ok)
                     :on-cancel #(swap! !state assoc :alert-dialog? false :menu :alert-cancel)
                     :on-close #(swap! !state assoc :alert-dialog? false)}
                    (ui/label "Backdrop clicks do not dismiss this alert.")
                    (ui/button "Retry" #(swap! !state assoc :menu :alert-retry)))
   (ui/hstack
    {:gap 8 :align :center}
    (ui/button "Open sheet" #(swap! !state assoc :sheet? true))
    (ui/button "Toast ok" #(swap! !state update :toasts conj
                                  {:id (str "t-" tick) :variant :success :title "Saved" :message "ok"})
               {:variant :primary})
    (ui/button "Toast err" #(swap! !state update :toasts conj
                                   {:id (str "e-" tick) :variant :error :title "Failed" :message "nope"}))
    (ui/button (if sticky-toast? "Hide sticky" "Sticky toast")
               #(swap! !state update :sticky-toast? not)))
   (ui/sheet sheet?
             {:title "Inspector"
              :placement :right
              :on-close #(swap! !state assoc :sheet? false)
              :footer (ui/button "Close" #(swap! !state assoc :sheet? false) {:primary true})}
             (ui/label (str "Sheet body · tick " tick)))
   (map (fn [{:keys [id variant title message]}]
          (ui/notification {:id id
                            :variant variant
                            :title title
                            :message message
                            :autohide true
                            :on-close #(swap! !state update :toasts
                                              (fn [xs] (vec (remove (fn [t] (= id (:id t))) xs))))}))
        toasts)
   (when sticky-toast?
     (ui/notification {:id "sticky"
                       :variant :info
                       :title "Sticky"
                       :message (str "click me · tick " tick)
                       :autohide false
                       :on-click #(swap! !state assoc :toast-hit tick)
                       :on-close #(swap! !state assoc :sticky-toast? false)}))))

(defn- data-panel [{:keys [list-sel list-confirm table-sel table-confirm table-shift tree-sel list-rev batch-shift? vlist-sel]}]
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
     (ui/context-menu
      [{:id :inspect :label "Inspect"}
       {:id :delete :label "Delete"}]
      {:on-change (set-key :menu)}
      (ui/data-table {:columns [{:id :name :label "Name" :width (if (pos? list-rev) 180 140)}
                                {:id :lang :label "Lang" :width 100}]
                      :rows table-rows
                      :selected (when (not= table-sel :gone) table-sel)
                      :height 160
                      :on-change (fn [id]
                                   (swap! !state #(-> %
                                                      (assoc :table-sel id)
                                                      (update :table-shift (fnil inc 0)))))
                      :on-confirm (set-key :table-confirm)}))
     (ui/hstack
      {:gap 8 :align :center}
      (ui/button "Tree lib" #(swap! !state assoc :tree-sel :lib))
      (ui/button "Tree nil" #(swap! !state assoc :tree-sel nil))
      (ui/button "Drop lib" #(swap! !state assoc :tree-sel :gone)))
     (ui/tree tree-items
              {:selected (when (not= tree-sel :gone) tree-sel)
               :height 160
               :on-change (set-key :tree-sel)})
     (ui/table {:columns [{:label "Invoice" :width 90}
                          {:label "Status"}
                          {:label "Amount" :align :end}]
                :rows [["INV001" "Paid" "$250.00"]
                       ["INV002" "Pending" "$150.00"]
                       ["INV003" "Unpaid" "$350.00"]]
                :footer ["Total" "" "$750.00"]
                :caption "Declarative Kit Table shorthand (not virtualized)."})
     (ui/table
      (ui/table-header
       (ui/table-row
        (ui/table-head "Person")
        (ui/table-head {:align :end} "Role")))
      (ui/table-body
       (ui/table-row
        (ui/table-cell
         (ui/hstack {:gap 8 :align :center}
                    (ui/avatar "Ada Lovelace")
                    (ui/label "Ada")))
        (ui/table-cell {:align :end} (ui/tag "Math"))))
      (ui/table-footer
       (ui/table-row
        (ui/table-cell {:span 2 :align :end} "Footer cell spanning both columns")))
      (ui/table-caption "Kit Table primitives (per-cell span, widget children)."))
     (ui/label (str "Virtual " (pr-str vlist-sel)))
     (ui/virtual-list (mapv (fn [i]
                              {:id (keyword (str "r" i))
                               :label (str "Row " i)
                               :height (if (even? i) 36 48)})
                            (range 40))
                      {:selected vlist-sel
                       :height 160
                       :on-change (set-key :vlist-sel)}))))

(defn- forms-panel [{:keys [qty otp color date src notes field-kind field-val combo combo-multi stars step]}]
  (ui/vstack
   {:gap 12}
   (ui/label (str "qty " qty " · otp " (pr-str otp) " · " (pr-str color) " · " date
                  " · field " (pr-str field-kind) " " (pr-str field-val)
                  " · combo " (pr-str combo)
                  " · multi " (pr-str combo-multi)
                  " · stars " stars
                  " · step " (pr-str step)))
   (ui/number-input qty {:id "qty" :min 0 :max 20 :step 1 :on-change (set-key :qty)})
   (ui/otp-input otp {:id "otp" :count 6 :on-change (set-key :otp)})
   (ui/hstack
    {:gap 8 :align :center}
    (ui/combobox combo
                 {:id "combo"
                  :options [{:id :clj :label "Clojure"}
                            {:id :rs :label "Rust"}
                            {:id :go :label "Go"}]
                  :placeholder "Language"
                  :flex 1
                  :on-change (set-key :combo)})
    (ui/combobox combo-multi
                 {:id "combo-multi"
                  :options [{:id :clj :label "Clojure"}
                            {:id :rs :label "Rust"}
                            {:id :go :label "Go"}]
                  :multiple true
                  :placeholder "Languages"
                  :flex 1
                  :on-change (set-key :combo-multi)}))
   (ui/rating stars {:id "stars" :max 5 :on-change (set-key :stars)})
   (ui/stepper step {:items [{:id :cart :label "Cart"}
                             {:id :pay :label "Pay"}
                             {:id :done :label "Done"}]
                     :on-change (set-key :step)})
   (ui/hstack
    {:gap 8 :align :center}
    (ui/color-picker color {:on-change (set-key :color)})
    (ui/button "Clear color" #(swap! !state assoc :color nil))
    (ui/button "Pink" #(swap! !state assoc :color "#ff00aa")))
   (ui/date-picker date {:on-change (set-key :date)})
   (ui/hstack
    {:gap 8 :align :center}
    (ui/button (if (= field-kind :number) "Field as text" "Field as number")
               #(swap! !state assoc :field-kind (if (= field-kind :number) :text :number)
                       :field-val (if (= field-kind :number) (str field-val) qty)))
    (if (= field-kind :number)
      (ui/number-input field-val {:id "field" :min 0 :max 20 :step 1 :on-change (set-key :field-val)})
      (ui/input (str field-val) {:id "field" :on-change (set-key :field-val)})))
   (ui/textarea notes {:id "notes" :rows 4 :on-change (set-key :notes)})
   (ui/editor src {:id "src" :language "clojure" :height 160 :on-change (set-key :src)})))

(defn- docs-panel [_]
  (ui/vstack
   {:gap 12}
   (ui/chart :line [{:id :a :label "Mon" :value 4}
                    {:id :b :label "Tue" :value 8}
                    {:id :c :label "Wed" :value 6}
                    {:id :d :label "Thu" :value 10}]
             {:height 160 :flex 1})
   (ui/bar-chart [{:id :a :label "A" :value 3}
                  {:id :b :label "B" :value 7}]
                 {:width 220 :height 140})
   (ui/pie-chart [{:id :a :label "A" :value 2}
                  {:id :b :label "B" :value 5}]
                 {:width 180 :height 160})
   (ui/markdown "# Markdown\n\nSelectable **GPUI Kit** `TextView`.\n\n- sheet\n- notification\n- charts"
                {:height 140})))

(defn- shell-panel [{:keys [nav sidebar-collapsed setting-notify setting-theme setting-accent split-id]}]
  (ui/vstack
   {:gap 12 :flex 1}
   (ui/label (str "nav " (pr-str nav)
                  " · notify " (pr-str setting-notify)
                  " · theme " (pr-str setting-theme)
                  " · accent " (pr-str setting-accent)
                  " · split " split-id))
   (ui/hstack
    {:gap 8 :align :center}
    (ui/button (if sidebar-collapsed "Expand" "Collapse")
               #(swap! !state update :sidebar-collapsed not))
    (ui/button "Remount split"
               #(swap! !state assoc :split-id (if (= split-id "split-a") "split-b" "split-a")))
    (ui/label "Sidebar + settings + dock + resizable"))
   (ui/sidebar [{:id :home :label "Home" :icon :check}
                {:id :files :label "Files" :icon :folder}
                {:id :gear :label "Settings" :icon :settings}]
               {:selected nav
                :collapsed sidebar-collapsed
                :title "Demo"
                :height 180
                :on-change (set-key :nav)})
   (ui/settings [{:id :general :label "General"
                  :items [{:id :notify :label "Notifications"
                           :variant :switch :checked setting-notify}
                          {:id :theme :label "Theme"
                           :variant :dropdown :value setting-theme
                           :items [{:id :dark :label "Dark"}
                                   {:id :light :label "Light"}]}
                          {:label "Advanced"
                           :items [{:id :accent :label "Accent"
                                    :variant :dropdown :value setting-accent
                                    :items [{:id :blue :label "Blue"}
                                            {:id :pink :label "Pink"}]}]}]}]
                {:height 220
                 :on-change (fn [{:keys [id value]}]
                              (case id
                                :notify (swap! !state assoc :setting-notify value)
                                :theme (swap! !state assoc :setting-theme value)
                                :accent (swap! !state assoc :setting-accent value)
                                nil))})
   (ui/resizable {:id split-id :orientation :horizontal :height 140}
                 (ui/markdown "Left pane" {:width 160})
                 (ui/markdown "Right pane"))
   (ui/dock {:height 320
             :items [{:id :files :side :left :label "Files"
                      :content (ui/markdown "**Files**\n\n- a.clj\n- b.rs")}
                     {:id :main :side :center :label "Main"
                      :content (ui/chart :area [{:id :a :label "A" :value 2}
                                                {:id :b :label "B" :value 5}]
                                         {:height 120})}
                     {:id :log :side :bottom :label "Log"
                      :content (ui/label "ready")}]})))

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
      (ui/label "Controlled Clojure state. Native GPUI Kit widgets."
                {:font-size 13})
      (ui/tabs tab
               {:items [{:id :general :label "General"}
                        {:id :chrome :label "Chrome"}
                        {:id :overlay :label "Overlay"}
                        {:id :data :label "Data"}
                        {:id :forms :label "Forms"}
                        {:id :docs :label "Docs"}
                        {:id :shell :label "Shell"}]
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
          :forms (forms-panel state)
          :docs (docs-panel state)
          :shell (shell-panel state)
          (general-panel state))))))))
