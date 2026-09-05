(ns widgets.app
  "Gallery of GPUI Kit widgets exposed through gpui.ui.

  Browse labeled examples backed by real r/atom state.
  Option ids are keywords on purpose: callbacks must round-trip them."
  (:require [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state
  (r/atom {:notify? true
           :bold? false
           :formats [:bold]
           :theme-mode :dark
           :volume 36
           :span [20 70]
           :zoom 1.0
           :remaining 40
           :released nil
           :lang :clj
           :dialect :clj
           :gallery-page :controls
           :section :audio
           :crumb :home
           :alert? true
           :dialog? false
           :overlay-lock? false
           :tick 0
           :popover? false
           :hover-card? false
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
           :secret "hunter2"
           :color "#3366ff"
           :date "2026-09-02"
           :src "(defn hi [] \n  :ok)"
           :notes "Multi-line notes."
           :alert-dialog? false
           :field-kind :number
           :field-val 4
           :combo :clj
           :combo-multi [:clj]
           :combo-query nil
           :stars 3
           :step :pay
           :page 4
           :vlist-sel :r0
           :nav :home
           :sidebar-collapsed false
           :setting-notify true
           :setting-theme :dark
           :setting-accent :blue
           :split-id "split-a"
           :split-sizes nil
           :chat-count 8
           :chat-scroll nil
           :chat-scroll-gen 0
           :trail [:home]
           :forward []
           :reuse-forward? true
           :replace-generation 0
           :native-menu? false
           :wrap? false
           :command-pick nil
           :command-query ""}))

(defn- set-key [k]
  (fn [v]
    (swap! !state assoc k v)))

(defn- example
  "A function label, a short explanation, and its live native example."
  [function-name description & children]
  (ui/vstack
   {:gap 10}
   (ui/label function-name {:font-family "Menlo" :font-size 14 :color "#7aa2f7"})
   (ui/label description {:font-size 13 :color "#a9b1d6"})
   (apply ui/vstack {:gap 12 :padding 16 :border "#343b58"} children)))

(defn- testing-controls [id & children]
  (let [expanded? (get-in @!state [:expanded-examples id])]
    (ui/vstack
     {:gap 8}
     (ui/button (if expanded? "Hide state & testing controls" "Show state & testing controls")
                #(swap! !state update-in [:expanded-examples id] not)
                {:variant :ghost})
     (when expanded?
       (apply ui/vstack {:gap 8} children)))))

(defn- chat-row [n]
  (let [base [{:id "m1" :who :ada :text "Can you review this draft?" :time "10:24 AM"}
              {:id "m2" :who :you :text "On it — sending comments." :time "10:25 AM" :status "Delivered"}
              {:id "m3" :who :ada :text "Thanks. Also attached the PDF." :time "10:26 AM"}]
        extras (map (fn [i]
                      {:id (str "m" (+ 4 i))
                       :who :you
                       :text (str "Follow-up " (inc i))
                       :time "10:27 AM"
                       :status "Sent"})
                    (range (max 0 (- n 3))))]
    (into base extras)))

(defn- chat-message [{:keys [id who text time status]}]
  (let [outgoing? (= who :you)]
    (ui/message {:id id
                 :alignment (if outgoing? :end :start)
                 :avatar (ui/avatar (if outgoing? "You" "Ada"))
                 :header (ui/message-header (if outgoing? "You" "Ada") time)
                 :footer (when status (ui/message-footer status))}
                (ui/bubble text (cond-> {:variant (if outgoing? :filled :secondary)}
                                  (= id "m3") (assoc :reactions (ui/bubble-reactions "👍")))))))

(defn- request-chat-scroll! [target]
  #(swap! !state (fn [s]
                   (-> s
                       (assoc :chat-scroll target)
                       (update :chat-scroll-gen (fnil inc 0))))))

(defn- controls-panel [{:keys [notify? bold? formats]}]
  (ui/vstack
   {:gap 24}
   (example "ui/button" "Actions can use a primary, secondary, or disabled appearance."
            (ui/hstack {:gap 12 :align :center}
                       (ui/button "Save changes" #(swap! !state update :tick inc) {:primary true})
                       (ui/button "Cancel" #(swap! !state update :tick inc))
                       (ui/button "Unavailable" {:disabled true})))
   (example "ui/button · custom-variant" "Nested Kit ButtonCustomVariant colors, not host text :color."
            (ui/button "Delete" #(swap! !state update :tick inc)
                       {:variant :custom
                        :custom-variant {:color "#b91c1c"
                                         :foreground "#f8fafc"
                                         :hover "#991b1b"
                                         :active "#7f1d1d"
                                         :shadow true}}))
   (example "ui/button · loading icon caret" "Kit loading, icon-only, rounded, and dropdown caret."
            (ui/hstack {:gap 12 :align :center}
                       (ui/button "Saving" {:loading true :primary true})
                       (ui/button "" {:icon :inbox :tooltip "Inbox" :accessibility-label "Inbox"})
                       (ui/button "More" {:icon :chevron-down :dropdown-caret true :rounded :none})))
   (example "ui/switch" "A boolean value and an on-change callback."
            (ui/switch notify? (set-key :notify?) "Notifications"))
   (example "ui/toggle" "A button that stays pressed while its value is true."
            (ui/toggle bold? {:on-change (set-key :bold?) :text "Bold" :width 120
                              :tooltip "Toggle bold"}))
   (example "ui/toggle-group" "Independent multi-toggle with real grouped selection state."
            (ui/toggle-group (or formats [])
                             {:items [{:id :bold :label "Bold"}
                                      {:id :italic :label "Italic"}
                                      {:id :underline :label "Underline"}]
                              :segmented true
                              :variant :outline
                              :on-change (set-key :formats)}))
   (example "ui/checkbox" "Use a zero-argument callback to toggle your atom."
            (ui/checkbox notify? #(swap! !state update :notify? not) "Receive updates"
                         {:tooltip "Native Kit tooltip"}))))

(defn- selection-panel [{:keys [theme-mode lang dialect]}]
  (ui/vstack
   {:gap 24}
   (example "ui/radio-group" "Choose one option; callbacks receive the original keyword id."
            (ui/radio-group theme-mode
                            {:options [{:id :light :label "Light"}
                                       {:id :dark :label "Dark"}]
                             :orientation :horizontal
                             :on-change (set-key :theme-mode)}))
   (example "ui/select" "Search the options or clear the selection."
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
             (ui/button "Clear" #(swap! !state assoc :lang nil))))
   (example "ui/select · grouped options" "Group related options and disable individual choices."
            (ui/select dialect
                       {:id "dialect"
                        :options [{:label "Lisp"
                                   :items [{:id :clj :label "Clojure"}
                                           {:id :cljs :label "ClojureScript"
                                            :display "ClojureScript (cljs)"}]}
                                  {:label "Systems"
                                   :items [{:id :rs :label "Rust"}
                                           {:id :go :label "Go" :disabled true}]}]
                        :placeholder "Grouped language"
                        :searchable true
                        :cleanable true
                        :title-prefix "Lang: "
                        :search-placeholder "Filter languages"
                        :empty "No languages"
                        :on-change (set-key :dialect)}))))

(defn- combobox-panel [{:keys [combo combo-multi combo-query]}]
  (ui/vstack
   {:gap 24}
   (example "ui/combobox" "Search grouped options and clear the current choice."
            (ui/combobox combo
                         {:id "combo"
                          :options [{:label "Lisp"
                                     :items [{:id :clj :label "Clojure"}
                                             {:id :cljs :label "ClojureScript"}]}
                                    {:label "Systems"
                                     :items [{:id :rs :label "Rust"}
                                             {:id :go :label "Go"}]}]
                          :placeholder "Language"
                          :search-placeholder "Filter languages"
                          :empty "No languages"
                          :cleanable true
                          :check-icon :check
                          :menu-width 280
                          :query combo-query
                          :flex 1
                          :on-change (set-key :combo)})
            (testing-controls :combobox
                              (ui/button "Filter clj" #(swap! !state assoc :combo-query "clj"))
                              (ui/button "Clear query" #(swap! !state assoc :combo-query ""))))
   (example "ui/combobox · :multiple true" "Select more than one language."
            (ui/combobox combo-multi
                         {:id "combo-multi"
                          :options [{:id :clj :label "Clojure"}
                                    {:id :rs :label "Rust"}
                                    {:id :go :label "Go"}]
                          :multiple true
                          :placeholder "Languages"
                          :flex 1
                          :on-change (set-key :combo-multi)}))))

(defn- sliders-panel [{:keys [volume span zoom remaining released]}]
  (ui/vstack
   {:gap 24}
   (example "ui/slider" "A numeric value between :min and :max."
            (ui/hstack
             {:gap 12 :align :center}
             (ui/label (str "Volume " volume))
             (ui/slider volume {:id "volume"
                                :min 0 :max 100 :flex 1
                                :tooltip "0–100"
                                :on-change (set-key :volume)
                                :on-release (set-key :released)})))
   (example "ui/slider · range" "Pass a pair of values for two draggable thumbs."
            (ui/hstack
             {:gap 12 :align :center}
             (ui/label (str "Span " (pr-str span)))
             (ui/slider span {:id "span"
                              :min 0 :max 100 :flex 1
                              :tooltip "Range thumbs"
                              :on-change (set-key :span)
                              :on-release (set-key :released)})))
   (example "ui/slider · :scale :logarithmic" "Useful for values that span different orders of magnitude."
            (ui/hstack
             {:gap 12 :align :center}
             (ui/label (str "Zoom " zoom))
             (ui/slider zoom {:id "zoom"
                              :min 0.25 :max 4 :step 0.05 :flex 1
                              :scale :logarithmic
                              :tooltip "Log zoom"
                              :on-change (set-key :zoom)})))
   (example "ui/slider · :reverse true" "Fill from the opposite end of the track."
            (ui/hstack
             {:gap 12 :align :center}
             (ui/label (str "Left " remaining))
             (ui/slider remaining {:id "remaining"
                                   :min 0 :max 100 :flex 1
                                   :reverse true
                                   :tooltip "Remaining fill"
                                   :on-change (set-key :remaining)}))
            (ui/label (str "Released " (pr-str released))))))

(defn- inputs-panel [{:keys [qty otp notes secret field-kind field-val]}]
  (ui/vstack
   {:gap 24}
   (example "ui/input" "A controlled text field with a stable :id."
            (ui/input notes {:id "single-line" :placeholder "Write something…" :on-change (set-key :notes)}))
   (example "ui/input · Kit chrome" "Prefix/suffix strings, a search icon, cleanable, password mask-toggle."
            (ui/vstack
             {:gap 8}
             (ui/input notes {:id "search" :icon :search :cleanable true
                              :placeholder "Search…" :on-change (set-key :notes)})
             (ui/input (str qty) {:id "amount" :prefix "$" :suffix "USD"
                                  :placeholder "Amount"})
             (ui/input secret {:id "password" :masked true :mask-toggle true
                               :content-type :password :placeholder "Password"
                               :on-change (set-key :secret)})))
   (example "ui/number-input" "Constrain numeric values with :min, :max, and :step."
            (ui/number-input qty {:id "qty" :min 0 :max 20 :step 1 :prefix "$"
                                  :on-change (set-key :qty)}))
   (example "ui/otp-input" "The callback fires when every cell has been filled. :groups clusters the cells."
            (ui/otp-input otp {:id "otp" :count 6 :groups 3 :on-change (set-key :otp)}))
   (example "ui/textarea" "A multi-line field; edits update the Clojure atom."
            (ui/textarea notes {:id "notes" :rows 4 :on-change (set-key :notes)})
            (testing-controls :dynamic-field
                              (ui/hstack
                               {:gap 8 :align :center}
                               (ui/button (if (= field-kind :number) "Field as text" "Field as number")
                                          #(swap! !state assoc :field-kind (if (= field-kind :number) :text :number)
                                                  :field-val (if (= field-kind :number) (str field-val) qty)))
                               (if (= field-kind :number)
                                 (ui/number-input field-val {:id "field" :min 0 :max 20 :step 1 :on-change (set-key :field-val)})
                                 (ui/input (str field-val) {:id "field" :on-change (set-key :field-val)})))))))

(defn- pickers-panel [{:keys [color date stars]}]
  (ui/vstack
   {:gap 24}
   (example "ui/color-picker" "Choose a color, set one programmatically, or clear it."
            (ui/hstack
             {:gap 8 :align :center}
             (ui/color-picker color {:on-change (set-key :color)
                                     :featured-colors ["#3366ff" "#22c55e" "#f59e0b"]})
             (ui/button "Clear color" #(swap! !state assoc :color nil))
             (ui/button "Pink" #(swap! !state assoc :color "#ff00aa"))))
   (example "ui/date-picker" "The selected date is an ISO date string."
            (ui/date-picker date {:on-change (set-key :date)
                                  :number-of-months 2
                                  :first-day-of-week :mon}))
   (example "ui/rating" "An interactive five-star rating."
            (ui/rating stars {:id "stars" :max 5 :on-change (set-key :stars)}))))

(defn- progress-panel [{:keys [volume]}]
  (ui/vstack
   {:gap 24}
   (example "ui/progress" "A linear indicator. Adjust Volume on the Sliders page to change it."
            (ui/progress volume {:tooltip "Mirrors the slider"}))
   (example "ui/progress · :loading :color" "Indeterminate bar, hex fill, and a named size."
            (ui/progress nil {:loading true :size :small :color "#3366ff"
                              :accessibility-label "Syncing" :width 240}))
   (example "ui/progress-circle" "A determinate indicator with optional content in the center."
            (ui/progress-circle volume {:size :large :color "#3366ff"
                                        :accessibility-label "Volume"}
                                (ui/label (str volume))))
   (example "ui/progress-circle · :loading true" "An indeterminate indicator for work without a known duration."
            (ui/progress-circle nil {:loading true :size :large
                                     :accessibility-label "Syncing"}))
   (example "ui/shimmer" "Animated text for an operation in progress."
            (ui/shimmer "Indexing project…" {:id "shimmer-index"}))
   (example "ui/shimmer · :truncate true" "Clip overflowing sweep text with a layout ellipsis, not a guessed character count."
            (ui/hstack {:width 220 :overflow-hidden true :align :center}
                       (ui/shimmer "Indexing src/gpui/ui.clj · host/src/renderer.rs · examples/widgets/app.clj"
                                   {:id "shimmer-truncate" :flex 1 :truncate true})))))

(defn- feedback-panel [{:keys [alert?]}]
  (ui/vstack
   {:gap 24}
   (example "ui/alert" "A dismissible message with a title and semantic variant."
            (ui/button "Show success message" #(swap! !state assoc :alert? true))
            (when alert?
              (ui/alert "Copied to the clipboard."
                        {:variant :success
                         :title "Done"
                         :on-close #(swap! !state assoc :alert? false)})))
   (example "ui/alert · :banner" "Banner style is full-width and does not show a title."
            (ui/alert "Scheduled maintenance tonight."
                      {:variant :warning :banner true :icon :triangle-alert}))
   (example "ui/badge · ui/icon" "Add a count, an unread dot, or an icon to a child."
            (ui/hstack {:gap 20 :align :center}
                       (ui/badge 3 (ui/icon :bell))
                       (ui/badge {:dot true} (ui/icon :inbox))
                       (ui/badge {:count 120 :max 99 :color "#3366ff"} (ui/icon :bell))
                       (ui/badge {:icon :check :color "#22c55e"} (ui/icon :user))))
   (example "ui/spinner" "A compact loading indicator."
            (ui/hstack {:gap 16 :align :center}
                       (ui/spinner {:size :small})
                       (ui/spinner {:size :large :color "#3366ff"})))
   (example "ui/skeleton" "A placeholder while content is loading."
            (ui/vstack {:gap 8}
                       (ui/skeleton {:width 220 :height 12})
                       (ui/skeleton {:width 180 :height 12 :secondary true})))))

(defn- avatars-panel [_]
  (ui/vstack
   {:gap 24}
   (example "ui/avatar" "Display initials or an icon for a person."
            (ui/avatar {:name "Ada Lovelace" :icon :user}))
   (example "ui/avatar-group" "Limit the visible avatars and show an overflow count."
            (ui/avatar-group {:limit 4 :ellipsis true}
                             (ui/avatar "Ada Lovelace")
                             (ui/avatar "Grace Hopper")
                             (ui/avatar "Alan Kay")
                             (ui/avatar "Barbara Liskov")
                             (ui/avatar "Rich Hickey")))
   (example "ui/tag · ui/kbd · ui/link" "Small labels, keyboard hints, and an external link."
            (ui/hstack {:gap 16 :align :center}
                       (ui/tag "Clojure" {:variant :info})
                       (ui/kbd "ctrl-s")
                       (ui/kbd "ctrl-k" {:outline true})
                       (ui/link "https://clojure.org" "clojure.org")))))

(defn- navigation-panel [{:keys [page]}]
  (ui/vstack
   {:gap 24}
   (example "ui/tabs" "Tabs are useful for a small set of sibling views."
            (ui/tabs (:demo-tab @!state :overview)
                     {:items [{:id :overview :label "Overview"}
                              {:id :activity :label "Activity with a long label"}]
                      :menu true
                      :max-width 140
                      :on-change (set-key :demo-tab)}))
   (example "ui/pagination" "Navigate a fixed number of pages."
            (ui/label (str "Page " page))
            (ui/pagination page {:id "pages" :total 12 :on-change (set-key :page)}))
   (example "ui/pagination · :compact true" "A smaller pagination control using the same state."
            (ui/pagination page {:id "pages-compact" :total 12 :compact true
                                 :on-change (set-key :page)}))))

(defn- breadcrumbs-panel [{:keys [crumb]}]
  (ui/vstack
   {:gap 24}
   (example "ui/breadcrumb" "Show a location in a hierarchy."
            (ui/breadcrumb
             [{:id :home :label "Home"}
              {:id :project :label "Project"}
              {:label "Widgets"}]
             {:on-change (set-key :crumb)})
            (ui/label (str "Crumb " (pr-str crumb))))))

(defn- stepper-panel [{:keys [step]}]
  (ui/vstack
   {:gap 24}
   (example "ui/stepper" "Show progress through a sequence of steps."
            (ui/stepper step {:items [{:id :cart :label "Cart"}
                                      {:id :pay :label "Pay"}
                                      {:id :done :label "Done"}]
                              :on-change (set-key :step)}))))

(defn- menus-panel [{:keys [menu batch-shift? native-menu? wrap? command-pick command-query]}]
  (ui/vstack
   {:gap 24}
   (example "ui/dropdown-menu · ui/dropdown-button" "Open a menu or combine it with a primary action."
            (ui/hstack
             {:gap 8 :align :center}
             (ui/dropdown-menu
              [{:id :copy :label "Copy"
                :on-click #(swap! !state assoc :batch-shift? true)}
               :-
               {:id :share :label "Share"
                :items [{:id :email :label "Email"}
                        {:id :link :label "Copy link"}]}]
              {:on-change (set-key :menu)}
              (ui/button "Edit"))
             (ui/dropdown-button
              [{:id :copy :label "Copy"
                :on-click #(swap! !state assoc :batch-shift? true)}
               :-
               {:id :share :label "Share"
                :items [{:id :email :label "Email"}
                        {:id :link :label "Copy link"}]}]
              {:on-change (set-key :menu) :variant :primary :selected true}
              (ui/button "Export" #(swap! !state assoc :menu :export)))
             (ui/dropdown-button
              [{:id :copy :label "Copy"}
               {:id :share :label "Share"}]
              {:on-change (set-key :menu) :variant :warning}
              (ui/button "Warn" {:size :small}))))
   (example "ui/context-menu" "Right-click the text to open contextual actions."
            (ui/context-menu
             [{:id :inspect :label "Inspect"}
              {:id :delete :label "Delete"}]
             {:on-change (set-key :menu)}
             (ui/label "Right-click this label.")))
   (example "ui/native-menu" "Show a platform menu with icons and checked items."
            (ui/hstack
             {:gap 8 :align :center}
             (ui/button "Native menu" #(swap! !state assoc :native-menu? true))
             (ui/label (if native-menu? "show requested" "closed")))
            (ui/native-menu
             [{:id :copy :label "Copy" :icon :copy}
              :-
              {:id :wrap :label "Word wrap" :checked wrap?}
              {:id :share :label "Share" :disabled true
               :items [{:id :email :label "Email"}
                       {:id :link :label "Copy link"}]}]
             {:id "gallery-native"
              :open? native-menu?
              :position [24 160]
              :on-change (fn [id]
                           (swap! !state (fn [s]
                                           (cond-> (assoc s :menu id)
                                             (= id :wrap) (update :wrap? not)))))
              :on-open-change (set-key :native-menu?)}))
   (example "ui/command" "Search commands by label or keyword."
            (ui/command
             [{:id :copy :label "Copy" :icon :copy :keywords [:duplicate]}
              {:id :wrap :label "Word wrap" :checked wrap?}
              :-
              {:label "Edit"
               :items [{:id :find :label "Find" :keywords [:search]}
                       {:id :replace :label "Replace"}]}]
             {:id "gallery-command"
              :placeholder "Type a command…"
              :menu-max-h 220
              :query command-query
              :on-change (fn [id]
                           (swap! !state (fn [s]
                                           (cond-> (assoc s :menu id :command-pick id)
                                             (= id :wrap) (update :wrap? not)))))
              :on-select (set-key :command-pick)
              :on-query (set-key :command-query)}))))

(defn- dialogs-panel [{:keys [dialog? alert-dialog? popover? hover-card? menu overlay-lock? tick batch-shift? close-hit dialog-open]}]
  (ui/vstack
   {:gap 24}
   (example "ui/dialog · ui/alert-dialog" "Open a confirmation dialog or an alert that ignores backdrop clicks."
            (ui/hstack {:gap 12}
                       (ui/button "Open dialog" #(swap! !state assoc :dialog? true :close-hit false :dialog-open true) {:primary true})
                       (ui/button "Open alert" #(swap! !state assoc :alert-dialog? true)))
            (ui/dialog dialog?
                       {:title (str "Confirm · tick " tick)
                        :variant :confirm
                        :ok-text "Delete"
                        :ok-variant :danger
                        :cancel-text "Keep"
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
            (testing-controls :dialogs
                              (ui/button "Rerender" #(swap! !state update :tick inc)
                                         {:tooltip "Unrelated atom update while a dialog stays open"})
                              (ui/switch overlay-lock? (set-key :overlay-lock?) "Lock overlay")))
   (example "ui/popover" "Anchor interactive content to a trigger."
            (ui/popover popover?
                        {:trigger (ui/button "Popover")
                         :on-open-change (set-key :popover?)}
                        (ui/label "Anchored content.")
                        (ui/button "Close" #(swap! !state assoc :popover? false) {:variant :ghost})))
   (example "ui/hover-card" "Hover over the profile button to reveal more information."
            (ui/hstack
             {:gap 8 :align :center}
             (ui/hover-card {:id "profile"
                             :trigger (ui/button "@huacnlee")
                             :open-delay 0.15
                             :placement :bottom-center
                             :on-open-change (set-key :hover-card?)}
                            (ui/hstack
                             {:gap 8 :align :center :padding 8}
                             (ui/avatar {:name "Jason Lee"
                                         :src "https://avatars.githubusercontent.com/u/5518?s=64"
                                         :size :large})
                             (ui/vstack
                              {:gap 2}
                              (ui/label "Jason Lee" {:font-weight :semibold})
                              (ui/label "GPUI Kit author"))))
             (ui/label (if hover-card? "card open" "hover the button"))))))

(defn- notifications-panel [{:keys [tick sheet? toasts sticky-toast? toast-hit]}]
  (ui/vstack
   {:gap 24}
   (example "ui/sheet · ui/notification" "Open an inspector sheet, or show temporary and persistent messages."
            (ui/hstack
             {:gap 8 :align :center}
             (ui/button "Open sheet" #(swap! !state assoc :sheet? true))
             (ui/button "Toast ok" #(swap! !state update :toasts conj
                                           {:id (str "t-" (random-uuid)) :variant :success :title "Saved" :message "Your changes have been saved."})
                        {:variant :primary})
             (ui/button "Toast err" #(swap! !state update :toasts conj
                                            {:id (str "e-" (random-uuid)) :variant :error :title "Failed" :message "Could not save changes. Try again."}))
             (ui/button (if sticky-toast? "Hide sticky" "Sticky toast")
                        #(swap! !state update :sticky-toast? not)))
            (ui/sheet sheet?
                      {:title "Inspector"
                       :placement :right
                       :overlay true
                       :resizable true
                       :on-close #(swap! !state assoc :sheet? false)
                       :footer (ui/button "Close" #(swap! !state assoc :sheet? false) {:primary true})}
                      (ui/label (str "Sheet body · tick " tick)))
            (map (fn [{:keys [id variant title message]}]
                   (ui/notification {:id id
                                     :variant variant
                                     :title title
                                     :message message
                                     :autohide true
                                     :placement :bottom-right
                                     :icon :check
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
                                :on-close #(swap! !state assoc :sticky-toast? false)})))))

(defn- nav-stack-panel [{:keys [trail forward reuse-forward? replace-generation]}]
  (ui/vstack
   {:gap 24}
   (example "ui/nav-stack · ui/nav-page" "Push and pop pages with animated transitions."
            (ui/nav-stack {:id "gallery-nav"
                           :stack trail
                           :transition 0.22
                           :item [{:phase :entering :operation [:push :replace]
                                   :left {:from 1 :to 0}
                                   :opacity {:from 0.35 :to 1}}
                                  {:phase :exiting :operation :pop
                                   :left {:from 0 :to 1}
                                   :opacity {:from 1 :to 0.35}}
                                  {:phase :exiting :operation :push
                                   :left {:from 0 :to -0.3}}
                                  {:phase :entering :operation :pop
                                   :left {:from -0.3 :to 0}}]
                           :overflow :hidden
                           :reuse-forward reuse-forward?
                           :replace-generation replace-generation
                           :on-forward-change #(swap! !state assoc :forward (vec %))
                           :height 180
                           :border "#3b4261"}
                          (ui/nav-page {:id :home :padding 12 :gap 8}
                                       (ui/label "Home" {:font-weight :semibold})
                                       (ui/button "Open detail" #(swap! !state assoc :trail [:home :detail])))
                          (ui/nav-page {:id :detail :padding 12 :gap 8}
                                       (ui/label "Detail" {:font-weight :semibold})
                                       (ui/hstack {:gap 8}
                                                  (ui/button "Back" #(swap! !state assoc :trail [:home]))
                                                  (ui/button "Open settings"
                                                             #(swap! !state assoc :trail [:home :detail :settings]))
                                                  (ui/button "Replace with settings"
                                                             #(swap! !state assoc :trail [:home :settings]))))
                          (ui/nav-page {:id :settings :padding 12 :gap 8}
                                       (ui/label "Settings" {:font-weight :semibold})
                                       (ui/button "Back to home" #(swap! !state assoc :trail [:home]))))
            (testing-controls :nav-stack
                              (ui/label (str "Trail " (pr-str trail) " · Forward " (pr-str forward)
                                             " · Gen " replace-generation))
                              (ui/hstack
                               {:gap 8 :align :center}
                               (when (seq forward)
                                 (ui/button "Forward" #(swap! !state update :trail conj (first forward))))
                               (when (> (count trail) 1)
                                 (ui/button "Pop to root" #(swap! !state assoc :trail [(first trail)])))
                               (ui/button "Replace page" #(swap! !state update :replace-generation inc))
                               (ui/switch reuse-forward? (set-key :reuse-forward?) "Reuse forward"))))))

(defn- lists-panel [{:keys [list-sel tree-sel list-rev vlist-sel]}]
  (let [suffix (when (pos? list-rev) (str " · " list-rev))
        list-items [{:id :alpha :label (str "Alpha" suffix)}
                    {:id :beta :label (str "Beta" suffix)}
                    {:id :gamma :label (str "Gamma" suffix)}
                    {:id :delta :label (str "Delta" suffix)}]
        list-items (if (= list-sel :gone)
                     (vec (remove #(= :alpha (:id %)) list-items))
                     list-items)
        tree-items (if (= tree-sel :gone)
                     [{:id :src :label "src" :expanded true
                       :items [{:id :main :label "main.rs"}]}
                      {:id :readme :label "README.md"}]
                     [{:id :src :label "src" :expanded true
                       :items [{:id :lib :label "lib.rs"}
                               {:id :main :label "main.rs"}]}
                      {:id :readme :label "README.md"}])]
    (ui/vstack
     {:gap 24}
     (example "ui/list" "A searchable list with selection and confirmation callbacks."
              (ui/list list-items
                       {:selected (when (not= list-sel :gone) list-sel)
                        :searchable true
                        :search-placeholder "Filter…"
                        :height 160
                        :on-change (fn [id]
                                     (swap! !state assoc :list-sel id :batch-shift? true))
                        :on-confirm (set-key :list-confirm)})
              (testing-controls :list
                                (ui/hstack
                                 {:gap 8 :align :center}
                                 (ui/button "List A→B" #(swap! !state assoc :list-sel :beta))
                                 (ui/button "List nil" #(swap! !state assoc :list-sel nil))
                                 (ui/button "Drop row" #(swap! !state assoc :list-sel :gone))
                                 (ui/button "Mutate labels" #(swap! !state update :list-rev inc)))))
     (example "ui/tree" "Browse nested items with expandable branches."
              (ui/tree tree-items
                       {:selected (when (not= tree-sel :gone) tree-sel)
                        :height 160
                        :on-change (set-key :tree-sel)})
              (testing-controls :tree
                                (ui/hstack
                                 {:gap 8 :align :center}
                                 (ui/button "Tree lib" #(swap! !state assoc :tree-sel :lib))
                                 (ui/button "Tree nil" #(swap! !state assoc :tree-sel nil))
                                 (ui/button "Drop lib" #(swap! !state assoc :tree-sel :gone)))))
     (example "ui/virtual-list" "Render a larger collection with different row heights."
              (ui/virtual-list (mapv (fn [i]
                                       {:id (keyword (str "r" i))
                                        :label (str "Row " i)
                                        :height (if (even? i) 36 48)})
                                     (range 40))
                               {:selected vlist-sel
                                :height 160
                                :on-change (set-key :vlist-sel)})))))

(defn- tables-panel [{:keys [table-sel table-shift list-rev table-cell? table-export-gen]}]
  (let [table-rows [{:id :ada :cells ["Ada" "Clojure"]}
                    {:id :grace :cells ["Grace" "Rust"]}
                    {:id :alan :cells ["Alan" "Go"]}]
        table-rows (if (= table-sel :gone)
                     (vec (remove #(= :ada (:id %)) table-rows))
                     table-rows)]
    (ui/vstack
     {:gap 24}
     (example "ui/data-table" "Interactive rows and cells. Right-click for contextual actions."
              (ui/context-menu
               [{:id :inspect :label "Inspect"}
                {:id :delete :label "Delete"}]
               {:on-change (set-key :menu)}
               (ui/data-table (cond-> {:columns [{:id :name :label "Name" :width (if (pos? list-rev) 180 140)
                                                  :sortable true :fixed :left}
                                                 {:id :lang :label "Lang" :width 100 :align :end}]
                                       :rows table-rows
                                       :header-groups [[{:label "Identity" :span 2}]]
                                       :stripe true
                                       :row-height 40
                                       :selected (when (not= table-sel :gone) table-sel)
                                       :height 160
                                       :on-change (fn [id]
                                                    (swap! !state #(-> %
                                                                       (assoc :table-sel id)
                                                                       (update :table-shift (fnil inc 0)))))
                                       :on-confirm (set-key :table-confirm)
                                       :on-export (fn [dump]
                                                    (swap! !state assoc :table-export dump))}
                                table-cell? (assoc :cell-selectable true :row-header false)
                                table-export-gen (assoc :export-generation table-export-gen))))
              (testing-controls :data-table
                                (ui/hstack
                                 {:gap 8 :align :center}
                                 (ui/button "Table A→B" #(swap! !state assoc :table-sel :grace))
                                 (ui/button "Table nil" #(swap! !state assoc :table-sel nil))
                                 (ui/button "Drop Ada" #(swap! !state assoc :table-sel :gone))
                                 (ui/button "Cell Ada/Lang" #(swap! !state assoc :table-sel {:row :ada :col :lang}
                                                                    :table-cell? true))
                                 (ui/switch (boolean table-cell?) (set-key :table-cell?) "Cells")
                                 (ui/button "Dump"
                                            #(swap! !state update :table-export-gen (fnil inc 0))))
                                (ui/scroll
                                 {:height 52}
                                 (if (pos? (or table-shift 0))
                                   (mapcat (fn [i]
                                             [(ui/button (str "tbl-canary-" i "-a") (fn []))
                                              (ui/button (str "tbl-canary-" i "-b") (fn []))])
                                           (range table-shift))
                                   (ui/label "tbl-canary slot")))))
     (example "ui/data-table · cell widgets" "Kit render_td paints supported RenderOnce nodes — progress, tag, stacks — not stringified cells."
              (ui/data-table {:columns [{:id :name :label "Name" :width 100}
                                        {:id :status :label "Status" :width 90}
                                        {:id :done :label "Done"}]
                              :rows [{:id :ada
                                      :cells ["Ada"
                                              (ui/tag "stable")
                                              (ui/progress 80 {:width 140})]}
                                     {:id :grace
                                      :cells ["Grace"
                                              (ui/tag "beta" {:variant :warning})
                                              (ui/progress 45 {:width 140})]}
                                     {:id :alan
                                      :cells [(ui/hstack {:gap 8 :align :center}
                                                         (ui/avatar "Alan")
                                                         (ui/label "Alan"))
                                              (ui/tag "wip" {:variant :info})
                                              (ui/progress 15 {:width 140})]}]
                              :row-height 40
                              :height 160}))
     (example "ui/table" "A declarative table with columns, rows, a footer, and a caption."
              (ui/table {:columns [{:label "Invoice" :width 90}
                                   {:label "Status"}
                                   {:label "Amount" :align :end}]
                         :rows [["INV001" "Paid" "$250.00"]
                                ["INV002" "Pending" "$150.00"]
                                ["INV003" "Unpaid" "$350.00"]]
                         :footer ["Total" "" "$750.00"]
                         :caption "Declarative Kit Table shorthand (not virtualized)."
                         :accessibility-label "Recent invoices"}))
     (example "ui/table · table-header / table-body / table-cell" "Compose cells from widgets and span multiple columns."
              (ui/table
               {:accessibility-label "Staff"}
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
               (ui/table-caption "Kit Table primitives (per-cell span, widget children)."))))))

(defn- text-panel [{:keys [src]}]
  (ui/vstack
   {:gap 24}
   (example "ui/editor" "Editable Clojure source with syntax highlighting."
            (ui/editor src {:id "src" :language "clojure" :height 160 :on-change (set-key :src)}))))

(defn- markdown-panel [_]
  (ui/vstack
   {:gap 24}
   (example "ui/markdown" "Render formatted, selectable text."
            (ui/markdown "# Markdown\n\nSelectable **GPUI Kit** `TextView`.\n\n- sheet\n- notification\n- charts"
                         {:height 140 :selectable true}))))

(defn- structure-panel [{:keys [section alert?]}]
  (ui/vstack
   {:gap 24}
   (example "ui/accordion" "Expand one section at a time."
            (ui/accordion section
                          {:on-change (set-key :section)
                           :bordered true
                           :items [{:id :audio
                                    :title "Audio"
                                    :content (ui/label "Speakers, mic, and volume.")}
                                   {:id :display
                                    :title "Display"
                                    :content (ui/label "Theme, density, and motion.")}]}))
   (example "ui/description-list" "Present a small set of labeled values."
            (ui/description-list [{:label "Host" :value "GPUI"}
                                  {:label "UI" :value "clj-gpui"}]
                                 {:bordered true}))
   (example "ui/separator" "Divide content horizontally or vertically."
            (ui/hstack {:gap 12 :align :center :height 36}
                       (ui/label "v")
                       (ui/separator {:orientation :vertical :height 28})
                       (ui/label "h"))
            (ui/separator))
   (example "ui/clipboard" "Copy a value with a compact control."
            (ui/clipboard "clj-gpui" {:on-copied (fn [_]
                                                   (swap! !state assoc :alert? true))}))))

(defn- charts-panel [_]
  (ui/vstack
   {:gap 24}
   (example "ui/horizontal-bar-chart" "Compare categories with labels and a value axis."
            (ui/horizontal-bar-chart
             [{:id :src :label "src" :value 412}
              {:id :target :label "target" :value 128}
              {:id :docs :label "docs" :value 48}
              {:id :test :label "test" :value 36}
              {:id :host :label "host" :value 29}
              {:id :examples :label "examples" :value 22}
              {:id :other :label "Other" :value 19}
              {:id :tmp :label "tmp" :value 11}]
             {:labels true :value-axis true}))
   (example "ui/chart · :line" "Show a series over time with interactive points."
            (ui/chart :line [{:id :a :label "Mon" :value 4}
                             {:id :b :label "Tue" :value 8}
                             {:id :c :label "Wed" :value 6}
                             {:id :d :label "Thu" :value 10}]
                      {:name "Desktop" :height 160 :interactive true}))
   (example "ui/bar-chart" "Compare discrete values."
            (ui/bar-chart [{:id :a :label "A" :value 3}
                           {:id :b :label "B" :value 7}]
                          {:name "Count" :width 220 :height 140 :corner-radii 4 :fill-gradient true}))
   (example "ui/bar-chart · :fill :display" "Kit BarChart fill maps and custom bar labels."
            (ui/bar-chart [{:id :a :label "A" :value 3 :display "3u"
                            :fill {:stops [{:color "#3366ff" :at 0}
                                           {:color "#88aaff" :at 1}]
                                   :space :bar}}
                           {:id :b :label "B" :value 7 :display "7u"
                            :fill "#22c55e"}]
                          {:name "Count" :width 220 :height 140 :corner-radii 4}))
   (example "ui/area-chart" "Display multiple series with a filled area."
            (ui/area-chart [{:id :a :label "Mon" :values [4 2]}
                            {:id :b :label "Tue" :values [8 5]}
                            {:id :c :label "Wed" :values [6 4]}]
                           {:series [{:id :desk :label "Desktop" :stroke "#ff0000"}
                                     {:id :mob :label "Mobile"}]
                            :height 140}))))

(defn- special-charts-panel [_]
  (ui/vstack
   {:gap 24}
   (example "ui/radar-chart" "Compare several dimensions across series."
            (ui/radar-chart [{:id :speed :label "Speed" :values [80 55]
                              :content (ui/badge 1 (ui/label "Sp"))}
                             {:id :range :label "Range" :values [40 90]}
                             {:id :rel :label "Reliability" :values [70 60]}]
                            {:series [{:id :a :label "A"} {:id :b :label "B"}]
                             ;; Leave room for the custom label and its overlaid badge.
                             :height 240
                             :outer-radius 80
                             :dot true
                             :grid-levels 5}))
   (example "ui/candlestick-chart" "Display open, high, low, and close values."
            (ui/candlestick-chart [{:id :mon :label "Mon" :open 100 :high 110 :low 95 :close 105}
                                   {:id :tue :label "Tue" :open 105 :high 118 :low 101 :close 112}
                                   {:id :wed :label "Wed" :open 112 :high 115 :low 98 :close 101}]
                                  {:height 160 :body-width-ratio 0.7}))
   (example "ui/sankey-chart" "Visualize weighted flows between nodes."
            (ui/sankey-chart [{:id :rev :label "Revenue"}
                              {:id :profit :label "Profit"}
                              {:id :cost :label "Cost"}]
                             {:links [{:source :rev :target :profit :value 45}
                                      {:source :rev :target :cost :value 55}]
                              :height 180
                              :node-corner-radius 3}))
   (example "ui/pie-chart" "Show proportions; :inner-radius turns a pie into a donut."
            (ui/pie-chart [{:id :a :label "A" :value 2 :color "#3366ff"}
                           {:id :b :label "B" :value 5}]
                          {:width 180 :height 160 :inner-radius 42 :labels true}))))

(defn- messages-panel [{:keys [chat-count chat-scroll chat-scroll-gen]}]
  (ui/vstack
   {:gap 24}
   (example "ui/message-group · ui/message · ui/bubble" "Compose incoming and outgoing messages with headers and footers."
            (ui/message-group
             (ui/message {:alignment :start
                          :stack-style {:gap 6}
                          :avatar (ui/avatar "Ada")
                          :header (ui/message-header "Ada" "10:24 AM")}
                         (ui/bubble "Incoming" {:variant :secondary}))
             (ui/message {:alignment :end
                          :avatar (ui/avatar "You")
                          :header (ui/message-header "You" "10:25 AM")
                          :footer (ui/message-footer "Delivered")}
                         (ui/bubble {}
                                    (ui/bubble-content {:bg "#1a1b26"} "Outgoing")
                                    "edited"))))
   (example "ui/marker" "Separate a conversation by day or event."
            (ui/marker "Today" {:variant :separator :id "day"
                                :separator-style {:color "#7aa2f7"}}))
   (example "ui/message · :variant :ghost on the bubble" "A system message with a quieter appearance."
            (ui/message {:alignment :start
                         :header (ui/message-header {:content-inset false} "System")}
                        (ui/bubble "Ghost system notice" {:variant :ghost})))
   (example "ui/message-scroller" "Append messages and jump to a specific item or the latest message."
            (ui/hstack
             {:gap 8 :align :center}
             (ui/button "Append" #(swap! !state update :chat-count inc))
             (ui/button "Top" (request-chat-scroll! "m1"))
             (ui/button "Latest" (request-chat-scroll! :end))
             (ui/label (str chat-count " rows · scroll " (pr-str chat-scroll)
                            " · gen " chat-scroll-gen)
                       {:font-size 13}))
            (apply ui/message-scroller
                   (cond-> {:id "chat" :height 280 :padding 8
                            :jump-button-label "Jump to latest"
                            :content-style {:padding 4}
                            :jump-button-renderer {:label "Latest" :size :small :icon :arrow-down}
                            :scroll-generation chat-scroll-gen}
                     (= :end chat-scroll) (assoc :scroll-to-end true)
                     (and (some? chat-scroll) (not= :end chat-scroll))
                     (assoc :scroll-to-item chat-scroll))
                   (map chat-message (chat-row chat-count))))))

(defn- attachments-panel [_]
  (ui/vstack
   {:gap 24}
   (example "ui/attachment · ui/attachment-content" "Compose a file preview with a title, description, and actions."
            (ui/attachment {:id "file-1" :status :uploading :size :small}
                           (ui/attachment-media (ui/icon :file))
                           (ui/attachment-content
                            (ui/attachment-title {:shimmer-style {:duration 1.2}}
                                                 "report.pdf")
                            (ui/attachment-description "Uploading"))
                           (ui/attachment-actions (ui/button "Cancel" {:compact true}))))
   (example "ui/attachment-media" "Use an image URL and an overlay icon for a preview."
            (ui/attachment {:id "img-1" :size :small}
                           (ui/attachment-media {:src "https://avatars.githubusercontent.com/u/5518?s=64"
                                                 :size :lg
                                                 :overlay (ui/icon :search)})
                           (ui/attachment-content (ui/attachment-title "preview.png"))))))

(defn- layout-panel [{:keys [nav sidebar-collapsed split-id]}]
  (ui/vstack
   {:gap 24}
   (example "ui/sidebar" "The gallery navigation is itself a ui/sidebar."
            (ui/sidebar [{:id :home :label "Home" :icon :check}
                         {:id :files :label "Files" :icon :folder}
                         {:id :gear :label "Settings" :icon :settings}]
                        {:selected nav
                         :collapsed sidebar-collapsed
                         :collapsible :icon
                         :title "Demo"
                         :height 180
                         :on-change (set-key :nav)})
            (ui/button (if sidebar-collapsed "Expand sidebar" "Collapse sidebar") #(swap! !state update :sidebar-collapsed not)))
   (example "ui/resizable" "Drag the divider to resize adjacent panes."
            (ui/resizable {:id split-id :orientation :horizontal :height 140}
                          (ui/markdown "Left pane" {:width 160})
                          (ui/markdown "Right pane")))
   (example "ui/dock" "Combine files, a main view, and a log in a dock layout."
            (ui/dock {:height 320
                      :items [{:id :files :side :left :label "Files"
                               :content (ui/markdown "**Files**\n\n- a.clj\n- b.rs")}
                              {:id :main :side :center :label "Main"
                               :content (ui/chart :area [{:id :a :label "A" :value 2}
                                                         {:id :b :label "B" :value 5}]
                                                  {:height 120})}
                              {:id :log :side :bottom :label "Log"
                               :content (ui/label "ready")}]}))))

(defn- settings-panel [{:keys [setting-notify setting-theme setting-accent]}]
  (ui/vstack
   {:gap 24}
   (example "ui/settings" "Declare pages and fields; callbacks receive {:id … :value …}."
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
                         {:height 360
                          :sidebar-width 180
                          :on-change (fn [{:keys [id value]}]
                                       (case id
                                         :notify (swap! !state assoc :setting-notify value)
                                         :theme (swap! !state assoc :setting-theme value)
                                         :accent (swap! !state assoc :setting-accent value)
                                         nil))}))))

(defn- status-panel [{:keys [wrap? command-pick]}]
  (ui/vstack
   {:gap 24}
   (example "ui/status-bar · truncated scan path" "Kit regions already clip; :truncate plus :flex 1 is wrap-off plus a layout ellipsis."
            (ui/status-bar {:left (ui/label "Ln 1")
                            :right [(ui/label "UTF-8")]}
                           (ui/shimmer "Indexing /Users/ada/src/clj-gpui/host/src/renderer.rs"
                                       {:id "scan-path" :flex 1 :truncate true})))
   (example "ui/label · :text-overflow :ellipsis-middle" "Keep the start and end of a path when the middle has to go."
            (ui/label "/Users/ada/projects/clj-gpui/host/src/renderer.rs"
                      {:width 220 :text-overflow :ellipsis-middle}))
   (example "ui/label · Kit secondary / highlights" "Secondary is muted trailing text; highlights is Kit search markup."
            (ui/hstack {:gap 16}
                       (ui/label "Ada" {:secondary "Lovelace"})
                       (ui/label "Hello World" {:highlights "World"})))
   (example "ui/status-bar" "Compose left, center, and right status content."
            (ui/status-bar {:left (ui/label (str "Ln 1 · wrap " (pr-str wrap?)))
                            :right [(ui/kbd "ctrl-k") (ui/label "UTF-8")]}
                           (ui/label (or (ui/format-option-id command-pick) "Ready"))))))

(def ^:private pages
  [{:id :controls
    :label "Buttons & toggles"
    :description "Start with actions and boolean controls."
    :panels [controls-panel]}
   {:id :selection
    :label "Select & combobox"
    :description "Choose from options using ordinary Clojure values."
    :panels [selection-panel combobox-panel]}
   {:id :sliders
    :label "Sliders"
    :description "Explore numeric ranges and different scales."
    :panels [sliders-panel]}
   {:id :inputs
    :label "Text & number inputs"
    :description "Edit values with controlled native fields."
    :panels [inputs-panel]}
   {:id :pickers
    :label "Pickers & rating"
    :description "Choose a color, a date, or a rating."
    :panels [pickers-panel]}
   {:id :feedback
    :label "Progress & feedback"
    :description "Communicate activity, status, and results."
    :panels [progress-panel feedback-panel]}
   {:id :avatars
    :label "Avatars & labels"
    :description "Represent people and add small pieces of context."
    :panels [avatars-panel]}
   {:id :navigation
    :label "Navigation"
    :description "Move between views, pages, and steps."
    :panels [navigation-panel breadcrumbs-panel stepper-panel nav-stack-panel]}
   {:id :menus
    :label "Menus & commands"
    :description "Offer actions in dropdown, contextual, and native menus."
    :panels [menus-panel]}
   {:id :dialogs
    :label "Dialogs & overlays"
    :description "Show details and ask for confirmation."
    :panels [dialogs-panel notifications-panel]}
   {:id :lists
    :label "Lists & trees"
    :description "Browse searchable, nested, and virtualized collections."
    :panels [lists-panel]}
   {:id :tables
    :label "Tables"
    :description "Explore interactive data and composable table cells."
    :panels [tables-panel]}
   {:id :text
    :label "Text & structure"
    :description "Compose documents, editable code, and expandable content."
    :panels [text-panel markdown-panel structure-panel]}
   {:id :charts
    :label "Basic charts"
    :description "Compare categories and follow series over time."
    :panels [charts-panel]}
   {:id :special-charts
    :label "More charts"
    :description "Explore dimensions, financial data, flows, and proportions."
    :panels [special-charts-panel]}
   {:id :messages
    :label "Messages"
    :description "Build a conversation from message and bubble primitives."
    :panels [messages-panel]}
   {:id :attachments
    :label "Attachments"
    :description "Compose file cards and image previews."
    :panels [attachments-panel]}
   {:id :layout
    :label "App layout"
    :description "Combine navigation, resizable panes, and dock panels."
    :panels [layout-panel]}
   {:id :settings
    :label "Settings & status"
    :description "Build a settings page and persistent status content."
    :panels [settings-panel status-panel]}])

(defn app []
  (let [state @!state
        page (or (some #(when (= (:id %) (:gallery-page state)) %) pages)
                 (first pages))]
    (ui/window
     {:title "Widgets" :chrome :dev :width 1040 :height 880 :theme "Tokyo Night"}
     (ui/hstack
      {:gap 0 :flex 1 :align :stretch}
      (ui/vstack
       {:width 230 :bg "#1c1e2a"}
       (ui/vstack
        {:gap 16 :padding 16}
        (ui/label "clj-gpui" {:font-size 22 :font-weight :semibold})
        (ui/label "WIDGET GALLERY" {:font-size 11 :color "#a9b1d6"}))
       (ui/sidebar (mapv #(select-keys % [:id :label]) pages)
                   {:id "gallery-sidebar" :selected (:id page) :flex 1
                    :on-change (set-key :gallery-page)})
       (ui/vstack
        {:padding 16}
        (ui/label "[gpui.ui :as ui]" {:font-family "Menlo" :font-size 12 :color "#a9b1d6"})))
      (ui/vstack
       {:flex 1}
       (ui/vstack
        {:gap 12 :padding 24}
        (ui/label (:label page) {:font-size 26 :font-weight :semibold})
        (ui/label (:description page) {:font-size 14 :color "#a9b1d6"}))
       (ui/separator)
       (ui/scroll
        {:id (str "gallery-page-" (name (:id page))) :flex 1 :padding 24 :gap 24}
        (map #(% state) (:panels page)))
       (ui/vstack
        {:padding 24}
        (ui/label "Live examples · Function labels match gpui.ui · Source: examples/widgets/src/widgets/app.clj"
                  {:font-size 11 :color "#a9b1d6"})))))))
