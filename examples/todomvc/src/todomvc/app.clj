(ns todomvc.app
  "TodoMVC in real JVM Clojure, rendered with gpui-component widgets."
  (:require [clojure.string :as str]
            [gpui.ratom :as r]
            [gpui.ui :as ui]))

(defonce !state
  (r/atom {:draft ""
           :filter :all
           :next-id 4
           :items [{:id 1 :title "Write the UI in real Clojure" :done true}
                   {:id 2 :title "Let GPUI own the native window" :done true}
                   {:id 3 :title "Type a todo into a real text field" :done false}]}))

(defn- visible-items [items filt]
  (case filt
    :active (filterv (complement :done) items)
    :completed (filterv :done items)
    items))

(defn- add-todo
  [title]
  (let [title (str/trim (str title))]
    (when (seq title)
      (swap! !state
             (fn [state]
               (-> state
                   (update :items conj {:id (:next-id state)
                                        :title title
                                        :done false})
                   (update :next-id inc)
                   (assoc :draft "")))))))

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

(defn- filter-button [current filt label]
  (ui/button
   label
   #(swap! !state assoc :filter filt)
   {:primary (= current filt)}))

(defn- item-row [{:keys [id title done]}]
  (ui/hstack
   {:gap 10}
   (ui/checkbox done #(toggle-item id) title
                {:color (if done "#7a8194" "#c0caf5")
                 :flex 1})
   (ui/button "Delete" #(delete-item id))))

(defn app []
  (let [{:keys [draft items] item-filter :filter} @!state
        shown (visible-items items item-filter)
        remaining (count (remove :done items))
        completed (count (filterv :done items))
        all-done? (and (seq items) (every? :done items))]
    (ui/vstack
     {:gap 14 :padding 8}
     (ui/label "todos" {:font-size 32 :font-weight :bold})
     (ui/label "Native GPUI, real Clojure. Type and press Enter."
               {:font-size 13 :color "#9aa4b2"})
     (ui/hstack
      {:gap 8}
      (ui/text-field
       draft
       {:id "new-todo"
        :flex 1
        :placeholder "What needs to be done?"
        :on-change #(swap! !state assoc :draft %)
        :on-submit add-todo})
      (ui/button "Add" #(add-todo draft) {:primary true}))
     (when (seq items)
       (ui/hstack
        {:gap 12}
        (ui/checkbox all-done? toggle-all
                     (if all-done? "Uncheck all" "Check all"))
        (ui/label (str remaining " item" (when (not= 1 remaining) "s") " left")
                  {:font-size 13 :color "#9aa4b2"})))
     (ui/hstack
      {:gap 8}
      (filter-button item-filter :all "All")
      (filter-button item-filter :active "Active")
      (filter-button item-filter :completed "Completed")
      (ui/spacer)
      (when (pos? completed)
        (ui/button "Clear completed" clear-completed)))
     (cond
       (empty? items)
       (ui/label "No todos yet. Add one above." {:font-size 13 :color "#9aa4b2"})

       (empty? shown)
       (ui/label (if (= item-filter :completed)
                   "Nothing completed yet."
                   "Nothing left to do.")
                 {:font-size 13 :color "#9aa4b2"})

       :else
       (ui/scroll
        {:height 360}
        (apply ui/vstack {:gap 8} (map item-row shown)))))))
