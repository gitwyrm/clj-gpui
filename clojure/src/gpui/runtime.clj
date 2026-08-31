(ns gpui.runtime
  "Host-side Clojure runtime for ClojureGPUI.

  This namespace is ordinary JVM Clojure. It:
    * loads the application namespace
    * serves the UI tree to the Rust GPUI host over a local socket
    * invokes real Clojure functions when GPUI reports a click
    * starts nREPL
    * reloads changed namespaces without touching the Rust binary"
  (:require [clojure.data.json :as json]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [gpui.core :as ui]
            [nrepl.server :as nrepl])
  (:import [java.io BufferedReader InputStreamReader OutputStreamWriter]
           [java.net InetSocketAddress Socket]
           [java.nio.charset StandardCharsets]))

(set! *warn-on-reflection* true)

(defonce ^:private conn (atom nil))
(defonce ^:private callbacks (atom {}))
(defonce ^:private callback-counter (atom 0))
(defonce ^:private render-scheduled? (atom false))
(defonce ^:private app-var* (atom nil))
(defonce ^:private nrepl-port* (atom nil))
(defonce ^:private file-mtimes (atom {}))

(defn- env
  ([k]
   (env k nil))
  ([k default]
   (or (System/getenv k) default)))

(defn- app-symbol
  []
  (symbol (env "CLOJUREGPUI_APP" "demo.app/app")))

(defn- src-root
  []
  (io/file (env "CLOJUREGPUI_SRC" "src")))

(defn- send!
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
            (println "[clojure] request-render failed:" (.getMessage e))))))))

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
            [(if (keyword? k) (name k) (str k))
             (json-tree v)]))

    (sequential? node)
    (mapv json-tree node)

    (keyword? node)
    (name node)

    (fn? node)
    (register-callback! node)

    :else node))

(defn load-app!
  "Require the application namespace and resolve its root `app` function."
  []
  (let [sym (app-symbol)
        nspace (symbol (namespace sym))
        nam (symbol (name sym))]
    (require nspace)
    (let [v (ns-resolve nspace nam)]
      (when-not v
        (throw (ex-info (str "Could not resolve " sym) {:var sym})))
      (reset! app-var* v)
      v)))

(defn reload-app!
  "Reload application namespaces in place. `defonce` state is preserved."
  []
  (let [sym (app-symbol)
        nspace (symbol (namespace sym))]
    (require 'gpui.core :reload)
    (require nspace :reload)
    (load-app!)
    {:ok true :ns (str nspace)}))

(defn export-tree
  "Build the current UI tree, registering callbacks as string ids."
  []
  (reset! callbacks {})
  (reset! callback-counter 0)
  (when-not @app-var*
    (load-app!))
  (let [app-fn (var-get @app-var*)
        tree (app-fn)]
    (json-tree (sanitize tree))))

(defn invoke-callback!
  "Invoke a previously registered Clojure function from a GPUI event."
  [callback-id]
  (if-let [f (get @callbacks callback-id)]
    (do
      (f)
      {:ok true :id callback-id})
    {:ok false :error (str "unknown callback " callback-id)}))

(defn- handle
  [msg]
  (try
    (let [id (:id msg)
          op (:op msg)
          result (case op
                   "render" {:ok true :tree (export-tree)}
                   "callback" (invoke-callback! (:callback-id msg))
                   "reload" (assoc (reload-app!) :tree (export-tree))
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
  (->> (file-seq root)
       (filter #(.isFile ^java.io.File %))
       (filter #(str/ends-with? (.getName ^java.io.File %) ".clj"))
       (remove #(str/includes? (.getPath ^java.io.File %) "runtime.clj"))))

(defn- snapshot-mtimes
  []
  (into {}
        (for [^java.io.File f (clj-files (src-root))]
          [(.getCanonicalPath f) (.lastModified f)])))

(defn- start-watcher!
  []
  (reset! file-mtimes (snapshot-mtimes))
  (future
    (loop []
      (try
        (Thread/sleep 400)
        (let [now (snapshot-mtimes)
              prev @file-mtimes]
          (when (and (seq prev) (not= prev now))
            (println "[clojure] source change detected, reloading")
            (try
              (reload-app!)
              (schedule-render!)
              (catch Exception e
                (binding [*out* *err*]
                  (println "[clojure] reload failed:" (.getMessage e))))))
          (reset! file-mtimes now))
        (catch Exception e
          (binding [*out* *err*]
            (println "[clojure] watcher error:" (.getMessage e)))))
      (recur))))

(defn- start-nrepl!
  []
  (let [preferred (Long/parseLong (env "CLOJUREGPUI_NREPL_PORT" "7888"))]
    (try
      (let [server (nrepl/start-server :port preferred :bind "127.0.0.1")]
        (reset! nrepl-port* preferred)
        (try (spit ".nrepl-port" (str preferred)) (catch Exception _))
        server)
      (catch Exception _
        (let [server (nrepl/start-server :port 0 :bind "127.0.0.1")
              port (:port server)]
          (reset! nrepl-port* port)
          (try (spit ".nrepl-port" (str port)) (catch Exception _))
          server)))))

(defn- connect-host!
  [host port]
  (println "[clojure] connecting to GPUI host" (str host ":" port))
  (let [sock (doto (Socket.)
               (.connect (InetSocketAddress. ^String host (int port)) 30000))
        in (BufferedReader.
            (InputStreamReader. (.getInputStream sock) StandardCharsets/UTF_8))
        out (OutputStreamWriter. (.getOutputStream sock) StandardCharsets/UTF_8)]
    (reset! conn {:socket sock :in in :out out})
    {:in in :out out :socket sock}))

(defn- read-loop
  [^BufferedReader in]
  (loop []
    (when-let [line (.readLine in)]
      (when-not (str/blank? line)
        (handle (json/read-str line :key-fn keyword)))
      (recur))))

(defn -main
  [& args]
  (let [host (env "CLOJUREGPUI_HOST" "127.0.0.1")
        port (Long/parseLong
              (or (second (drop-while #(not= % "--port") args))
                  (env "CLOJUREGPUI_PORT")
                  (throw (ex-info "CLOJUREGPUI_PORT is required" {}))))]
    (ui/set-request-render! schedule-render!)
    (load-app!)
    (start-nrepl!)
    (start-watcher!)
    (let [{:keys [in]} (connect-host! host port)]
      (send! {:op "ready"
              :nrepl @nrepl-port*
              :app (str (app-symbol))})
      (println (str "[clojure] nREPL listening on 127.0.0.1:" @nrepl-port*))
      (println (str "[clojure] hot reload watching " (.getPath ^java.io.File (src-root))))
      (println "[clojure] root UI var" (app-symbol))
      (read-loop in)
      (println "[clojure] host disconnected, exiting")
      (System/exit 0))))
