(ns gpui.runtime
  "Internal Clojure↔host protocol. Application code should not require this.

  The runtime:
    * loads the application var
    * sanitizes UI trees (functions become callback ids)
    * invokes Clojure functions when the host reports a click
    * reloads app namespaces on request"
  (:require [clojure.data.json :as json]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [gpui.ui :as ui])
  (:import [java.io BufferedReader OutputStreamWriter]
           [java.nio.charset StandardCharsets]))

(set! *warn-on-reflection* true)

(def protocol-version ui/protocol-version)

(defonce ^:private conn (atom nil))
(defonce ^:private callbacks (atom {}))
(defonce ^:private callback-counter (atom 0))
(defonce ^:private render-scheduled? (atom false))
(defonce ^:private app-var* (atom nil))
(defonce ^:private app-sym* (atom nil))
(defonce ^:private nrepl-port* (atom nil))
(defonce ^:private file-mtimes (atom {}))
(defonce ^:private load-error* (atom nil))

(defn- env
  ([k]
   (env k nil))
  ([k default]
   (or (System/getenv k) default)))

(defn app-symbol
  []
  (or @app-sym*
      (when-let [s (env "CLOJUREGPUI_APP")]
        (symbol s))))

(defn set-app-symbol!
  [sym]
  (reset! app-sym* (symbol (str sym)))
  @app-sym*)

(defn nrepl-port
  []
  @nrepl-port*)

(defn- src-root
  []
  (io/file (env "CLOJUREGPUI_SRC" "src")))

(defn send!
  [message]
  (when-let [{:keys [^OutputStreamWriter out]} @conn]
    (locking out
      (.write out ^String (str (json/write-str message) "\n"))
      (.flush out))))

(defn- schedule-render!
  []
  (when (compare-and-set! render-scheduled? false true)
    (future
      (try
        (Thread/sleep 16)
        (reset! render-scheduled? false)
        (send! {:op "request-render"})
        (catch Exception e
          (reset! render-scheduled? false)
          (binding [*out* *err*]
            (println "[clj-gpui] request-render failed:" (.getMessage e))))))))

(defn install-render-hook!
  []
  (ui/set-request-render! schedule-render!))

(defn bind-connection!
  [m]
  (reset! conn m)
  m)

(defn- register-callback!
  [f]
  (let [id (str "cb-" (swap! callback-counter inc))]
    (swap! callbacks assoc id f)
    id))

(defn- sanitize
  "Replace Clojure functions in the UI tree with callback ids before JSON."
  [node]
  (cond
    (ui/ui-node? node)
    (let [node (cond-> node
                 (fn? (:on-click node))
                 (assoc :on-click (register-callback! (:on-click node)))
                 (fn? (:on-change node))
                 (assoc :on-change (register-callback! (:on-change node))))]
      (update node :children #(mapv sanitize (or % []))))

    (sequential? node)
    (mapv sanitize node)

    :else node))

(defn- json-tree
  [node]
  (cond
    (map? node)
    (into {}
          (for [[k v] node]
            [k (json-tree v)]))

    (sequential? node)
    (mapv json-tree node)

    (keyword? node)
    (name node)

    (fn? node)
    (register-callback! node)

    :else node))

(defn- error-tree
  [^Throwable e]
  (let [sw (java.io.StringWriter.)
        pw (java.io.PrintWriter. sw)]
    (.printStackTrace e pw)
    (.flush pw)
    (ui/vstack
     {:gap 8 :padding 12}
     (ui/label "Clojure error" {:font-size 18 :font-weight :bold :color "#f7768e"})
     (ui/label (str (.getClass e) ": " (.getMessage e)) {:color "#c0caf5"})
     (ui/scroll
      {:height 280}
      (ui/label (str sw) {:color "#9aa3b5"})))))

(defn load-app!
  "Require the application namespace and resolve its root UI var."
  []
  (let [sym (or (app-symbol)
                (throw (ex-info "No application var. Pass my.app/app to gpui.dev." {})))
        nspace (symbol (namespace sym))
        nam (symbol (name sym))]
    (when-not (namespace sym)
      (throw (ex-info (str "Application var must be namespaced, got " sym) {:var sym})))
    (require nspace)
    (let [v (ns-resolve nspace nam)]
      (when-not v
        (throw (ex-info (str "Could not resolve " sym) {:var sym})))
      (reset! load-error* nil)
      (reset! app-var* v)
      v)))

(defn reload-app!
  "Reload application namespaces in place. `defonce` / ratom state is preserved."
  []
  (let [sym (app-symbol)
        nspace (symbol (namespace sym))]
    (require 'gpui.ui :reload)
    (require 'gpui.core :reload)
    (require 'gpui.ratom :reload)
    (require nspace :reload)
    (load-app!)
    {:ok true :ns (str nspace)}))

(defn reset-callbacks!
  []
  (reset! callbacks {})
  (reset! callback-counter 0))

(defn lookup-callback
  [id]
  (get @callbacks id))

(defn- export-node
  [tree]
  (json-tree (sanitize tree)))

(defn export-tree
  "Build a UI tree, registering callbacks as string ids.

  Zero-arity calls the application var. One-arity sanitizes an already
  built tree (or invokes a 0-arg function that returns one)."
  ([]
   (reset-callbacks!)
   (try
     (when-not @app-var*
       (load-app!))
     (let [app-fn (var-get @app-var*)
           tree (app-fn)]
       (export-node tree))
     (catch Exception e
       (reset! load-error* e)
       (.printStackTrace e)
       (export-node (error-tree e)))))
  ([tree]
   (reset-callbacks!)
   (try
     (export-node (if (fn? tree) (tree) tree))
     (catch Exception e
       (reset! load-error* e)
       (.printStackTrace e)
       (export-node (error-tree e))))))

(defn invoke-callback!
  "Invoke a previously registered Clojure function from a GPUI event."
  [callback-id]
  (if-let [f (get @callbacks callback-id)]
    (do
      (f)
      {:ok true :id callback-id})
    {:ok false :error (str "unknown callback " callback-id)}))

(defn handle
  [msg]
  (try
    (let [id (:id msg)
          op (:op msg)
          result (case op
                   "render" {:ok true :tree (export-tree)}
                   "callback" (invoke-callback! (:callback-id msg))
                   "reload" (try
                              (assoc (reload-app!) :ok true :tree (export-tree))
                              (catch Exception e
                                {:ok true :tree (json-tree (sanitize (error-tree e)))}))
                   {:ok false :error (str "unknown op: " op)})]
      (send! (cond-> result
               id (assoc :id id)
               true (assoc :op "response"))))
    (catch Exception e
      (.printStackTrace e)
      (send! {:op "response"
              :id (:id msg)
              :ok false
              :error (str (.getClass e) ": " (.getMessage e))}))))

(defn- clj-files
  [^java.io.File root]
  (if-not (.exists root)
    []
    (->> (file-seq root)
         (filter #(.isFile ^java.io.File %))
         (filter #(str/ends-with? (.getName ^java.io.File %) ".clj"))
         (remove (fn [^java.io.File f]
                   (re-find #"(^|/)(runtime|dev)\.clj$" (.getPath f)))))))

(defn- snapshot-mtimes
  []
  (into {}
        (for [^java.io.File f (clj-files (src-root))]
          [(.getCanonicalPath f) (.lastModified f)])))

(defn start-watcher!
  []
  (reset! file-mtimes (snapshot-mtimes))
  (future
    (loop []
      (try
        (Thread/sleep 400)
        (let [now (snapshot-mtimes)
              prev @file-mtimes]
          (when (and (seq prev) (not= prev now))
            (println "[clj-gpui] source change detected, reloading")
            (try
              (reload-app!)
              (schedule-render!)
              (catch Exception e
                (binding [*out* *err*]
                  (println "[clj-gpui] reload failed:" (.getMessage e)))
                (schedule-render!))))
          (reset! file-mtimes now))
        (catch Exception e
          (binding [*out* *err*]
            (println "[clj-gpui] watcher error:" (.getMessage e)))))
      (recur))))

(defn start-nrepl!
  []
  (require 'nrepl.server)
  (let [start-server (ns-resolve 'nrepl.server 'start-server)
        preferred (Long/parseLong (env "CLOJUREGPUI_NREPL_PORT" "7888"))]
    (try
      (let [server (start-server :port preferred :bind "127.0.0.1")]
        (reset! nrepl-port* preferred)
        (try (spit ".nrepl-port" (str preferred)) (catch Exception _))
        server)
      (catch Exception _
        (let [server (start-server :port 0 :bind "127.0.0.1")
              port (:port server)]
          (reset! nrepl-port* port)
          (try (spit ".nrepl-port" (str port)) (catch Exception _))
          server)))))

(defn send-ready!
  []
  (send! {:op "ready"
          :protocol-version protocol-version
          :nrepl (or @nrepl-port* 0)
          :app (str (app-symbol))}))

(defn read-loop
  [^BufferedReader in]
  (loop []
    (when-let [line (.readLine in)]
      (when-not (str/blank? line)
        (handle (json/read-str line :key-fn keyword)))
      (recur))))
