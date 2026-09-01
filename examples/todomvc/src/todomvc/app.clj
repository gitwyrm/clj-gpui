(ns todomvc.app
  "Classic TodoMVC layout in real JVM Clojure."
  (:require [clojure.string :as str]
            [gpui.ratom :as r]
            [gpui.ui :as ui]))

(def ^:private page-bg "#f5f5f5")
(def ^:private card-bg "#ffffff")
(def ^:private title-color "#b83f45")
(def ^:private text-color "#4d4d4d")
(def ^:private muted "#777777")
(def ^:private completed-color "#949494")
(def ^:private line "#ededed")
(def ^:private destroy "#cc9a9a")
(def ^:private hint "#bfbfbf")

(defonce !state
  (r/atom {:draft ""
           :filter :all
           :next-id 4
           :items [{:id 1 :title "Taste Clojure" :done true}
                   {:id 2 :title "Buy a unicorn" :done false}
                   {:id 3 :title "Render this with GPUI" :done false}]}))

(defn- visible-items [items filt]
  (case filt
    :active (filterv (complement :done) items)
    :completed (filterv :done items)
    items))

(defn- add-todo
  [title]
  (let [title (str/trim (str title))]
    (swap! !state
           (fn [state]
             (let [cleared (assoc state :draft "")]
               (if (seq title)
                 (-> cleared
                     (update :items conj {:id (:next-id state)
                                          :title title
                                          :done false})
                     (update :next-id inc))
                 cleared))))))

(defn- toggle-item [id]
  (swap! !state update :items
         (fn [items]
           (mapv (fn [item]
                   (if (= id (:id item))
                     (update item :done not)
                     item))
                 items))))

(defn- delete-item [id]
  (swap! !state update :items (fn [items] (filterv #(not= id (:id %)) items))))

(defn- toggle-all []
  (swap! !state update :items
         (fn [items]
           (let [all-done? (and (seq items) (every? :done items))]
             (mapv #(assoc % :done (not all-done?)) items)))))

(defn- clear-completed []
  (swap! !state update :items (fn [items] (filterv (complement :done) items))))

(defn- filter-link [current filt label]
  (ui/button
   label
   #(swap! !state assoc :filter filt)
   {:variant (if (= current filt) :outline :ghost)
    :compact true}))

(defn- item-row [{:keys [id title done]}]
  (ui/hstack
   {:gap 12 :padding 8 :border-bottom line :align :center}
   (ui/checkbox done #(toggle-item id) {:shape :circle :size 30})
   (ui/label title {:flex 1
                    :font-size 22
                    :color (if done completed-color text-color)
                    :strikethrough done})
   (ui/button "×" #(delete-item id) {:variant :text :color destroy :compact true})))

(defn- remaining-label [n]
  (str n " item" (when (not= 1 n) "s") " left"))

(defn app []
  (let [{:keys [draft items] item-filter :filter} @!state
        shown (visible-items items item-filter)
        remaining (count (remove :done items))
        completed (count (filterv :done items))
        all-done? (and (seq items) (every? :done items))]
    (ui/vstack
     {:theme :light :flex 1 :bg page-bg :padding 28 :gap 8 :align :center}
     (ui/label "todos" {:font-size 80
                        :font-weight :light
                        :font-family ".SystemUIFont"
                        :color title-color})
     (ui/vstack
      {:width 550 :bg card-bg :shadow true}
      (ui/hstack
       {:gap 8 :padding 12 :border-bottom line :align :center}
       (when (seq items)
         (ui/button "⌄" toggle-all
                    {:variant :text
                     :compact true
                     :font-size 22
                     :color (if all-done? "#737373" "#e6e6e6")}))
       (ui/text-field
        draft
        {:id "new-todo"
         :flex 1
         :font-size 22
         :placeholder "What needs to be done?"
         :on-change #(swap! !state assoc :draft %)
         :on-submit add-todo}))
      (when (seq items)
        (ui/vstack
         {}
         (if (empty? shown)
           (ui/label (if (= item-filter :completed)
                       "No completed todos"
                       "No active todos")
                     {:padding 16 :color muted :font-size 16})
           (ui/scroll
            {:height 280}
            (map item-row shown)))
         (ui/hstack
          {:padding 10}
          (ui/label (remaining-label remaining)
                    {:flex 1 :font-size 13 :color muted})
          (ui/hstack
           {:gap 4}
           (filter-link item-filter :all "All")
           (filter-link item-filter :active "Active")
           (filter-link item-filter :completed "Completed"))
          (ui/hstack
           {:flex 1 :justify :end}
           (when (pos? completed)
             (ui/button "Clear completed" clear-completed
                        {:variant :text :compact true :color muted})))))))
     (ui/vstack
      {:padding 20 :gap 4 :align :center}
      (ui/label "Press Enter to add a todo" {:font-size 11 :color hint})
      (ui/label "Click a checkbox to toggle · × to delete" {:font-size 11 :color hint})
      (ui/label "Written in real Clojure · rendered by GPUI" {:font-size 11 :color hint})))))
