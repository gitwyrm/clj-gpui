(ns gpui.platform
  "Native platform actions served by the GPUI host.

  Folder picking, revealing a path in the file manager, and opening a
  path with the system handler all go through the host so they use the
  real OS dialogs (and work even when the JVM is headless)."
  (:require [gpui.runtime :as runtime]))

(defonce ^:private pending* (atom {}))
(defonce ^:private counter* (atom 0))

(defn pick-directory
  "Open a native folder picker without blocking the caller.

  `on-result` is invoked later with a map:

    {:path \"/Users/me/Documents\"}   ; selected
    {:cancelled true}                 ; user dismissed the dialog
    {:error \"...\"}                  ; dialog could not be shown

  Optional `opts`: `:title` (string shown on some platforms).

  Returns the request id."
  ([on-result]
   (pick-directory nil on-result))
  ([opts on-result]
   (when-not (fn? on-result)
     (throw (ex-info "pick-directory requires a callback" {:on-result on-result})))
   (let [id (str "pick-" (swap! counter* inc))
         title (some-> (:title opts) str)]
     (swap! pending* assoc id {:on-result on-result})
     (runtime/send! (cond-> {:op "pick-directory" :request-id id}
                      (seq title) (assoc :title title)))
     id)))

(defn deliver-pick!
  "Runtime hook: the host finished a `pick-directory` request."
  [{:keys [request-id path error cancelled]}]
  (when-let [{:keys [on-result]} (get @pending* request-id)]
    (swap! pending* dissoc request-id)
    (try
      (on-result (cond
                   (some? error) {:error (str error)}
                   (or cancelled (nil? path) (= path "")) {:cancelled true}
                   :else {:path (str path)}))
      (catch Exception e
        (binding [*out* *err*]
          (println "[clj-gpui] pick-directory callback failed:" (.getMessage e)))))
    true))

(defn pending-picks
  "Request ids waiting for a folder-picker result. Intended for tests."
  []
  (vec (keys @pending*)))

(defn clear-pending!
  "Drop outstanding picker callbacks. Intended for tests."
  []
  (reset! pending* {})
  (reset! counter* 0)
  nil)

(defn reveal-path!
  "Show `path` in Finder (macOS) or the system file manager (Linux)."
  [path]
  (runtime/send! {:op "reveal-path" :path (str path)})
  true)

(defn open-path!
  "Open `path` with the system default handler (folder window, app, …)."
  [path]
  (runtime/send! {:op "open-path" :path (str path)})
  true)
