(ns gpui.dev
  "Clojure-first entry point for clj-gpui.

  Starts nREPL, watches app sources, listens for the native host, then
  launches the GPUI binary. Application authors run:

    clj -M:dev my.app/app

  The host is built with Cargo on first use when `host/` is present
  (this repo, a git dep checkout, or CLJ_GPUI_ROOT)."
  (:require [clojure.java.io :as io]
            [gpui.runtime :as runtime])
  (:import [java.io BufferedReader InputStreamReader OutputStreamWriter]
           [java.net ServerSocket]
           [java.nio.charset StandardCharsets]
           [java.util ArrayList]))

(set! *warn-on-reflection* true)

(defn- env
  ([k] (env k nil))
  ([k default]
   (or (System/getenv k) default)))

(defn- library-root
  "Directory that contains this library's deps.edn and host/ crate."
  ^java.io.File []
  (if-let [explicit (env "CLJ_GPUI_ROOT")]
    (io/file explicit)
    (when-let [res (io/resource "gpui/dev.clj")]
      (try
        (let [uri (.toURI res)]
          (when (= "file" (.getScheme uri))
            (let [file (io/file uri)]
              (when (.exists file)
                (-> file .getParentFile .getParentFile .getParentFile)))))
        (catch Exception _
          nil)))))

(defn- host-candidates
  [^java.io.File root]
  [(io/file root "host" "target" "release" "clj-gpui")
   (io/file root "host" "target" "debug" "clj-gpui")])

(defn- build-host!
  [^java.io.File root]
  (let [host-dir (io/file root "host")]
    (when-not (.exists (io/file host-dir "Cargo.toml"))
      (throw (ex-info (str "No host/ crate under " (.getPath root)
                           ". Set CLJ_GPUI_BIN to a clj-gpui binary, or CLJ_GPUI_ROOT to the clj-gpui checkout.")
                      {:root (.getPath root)})))
    (println "[clj-gpui] building native host with cargo (first run can take a while)")
    (let [args (doto (ArrayList.)
                 (.add "cargo")
                 (.add "build")
                 (.add "--release"))
          pb (doto (ProcessBuilder. args)
               (.directory host-dir)
               (.inheritIO))
          ^Process proc (.start pb)
          code (.waitFor proc)]
      (when-not (zero? code)
        (throw (ex-info (str "cargo build --release failed with exit " code)
                        {:dir (.getPath host-dir) :code code}))))))

(defn- ensure-host
  ^java.io.File []
  (if-let [explicit (env "CLJ_GPUI_BIN")]
    (let [f (io/file explicit)]
      (when-not (.canExecute f)
        (throw (ex-info (str "CLJ_GPUI_BIN is not executable: " explicit) {})))
      f)
    (let [^java.io.File root (or (library-root)
                                 (throw (ex-info "Could not locate clj-gpui. Set CLJ_GPUI_ROOT or CLJ_GPUI_BIN." {})))]
      (or (first (filter #(.canExecute ^java.io.File %) (host-candidates root)))
          (do (build-host! root)
              (or (first (filter #(.canExecute ^java.io.File %) (host-candidates root)))
                  (throw (ex-info "Host build succeeded but clj-gpui binary was not found."
                                  {:root (.getPath root)}))))))))

(defn- spawn-host!
  [^java.io.File exe port protocol-test?]
  (let [cmd (doto (ArrayList.)
              (.add (.getCanonicalPath exe))
              (cond-> protocol-test? (.add "--protocol-test")))
        pb (doto (ProcessBuilder. cmd)
             (.inheritIO))
        env-map (.environment pb)]
    (.put env-map "CLJ_GPUI_PORT" (str port))
    (.put env-map "CLJ_GPUI_HOST" "127.0.0.1")
    (when-let [icd (env "VK_ICD_FILENAMES")]
      (.put env-map "VK_ICD_FILENAMES" icd))
    (println (str "[clj-gpui] starting host " (.getName exe)))
    (.start pb)))

(defn- parse-args
  [args]
  (let [protocol-test? (boolean (some #{"--protocol-test"} args))
        rest (vec (remove #{"--protocol-test"} args))
        app (or (first rest) (env "CLJ_GPUI_APP"))]
    {:protocol-test? protocol-test?
     :app app}))

(defn -main
  [& args]
  (let [{:keys [protocol-test? app]} (parse-args args)]
    (when-not app
      (binding [*out* *err*]
        (println "Usage: clojure -M -m gpui.dev [ --protocol-test ] my.app/app")
        (println "Example: cd examples/counter && clojure -M:dev"))
      (System/exit 2))
    (runtime/set-app-symbol! app)
    (runtime/install-render-hook!)
    (try
      (runtime/load-app!)
      (catch Exception e
        (binding [*out* *err*]
          (println "[clj-gpui] failed to load" app ":" (.getMessage e)))
        (when protocol-test?
          (.printStackTrace e)
          (System/exit 1))))
    (when-not protocol-test?
      (runtime/start-nrepl!)
      (runtime/start-watcher!)
      (println (str "[clj-gpui] nREPL 127.0.0.1:" (runtime/nrepl-port)))
      (println (str "[clj-gpui] hot reload watching " (env "CLJ_GPUI_SRC" "src")))
      (println "[clj-gpui] root UI var" (runtime/app-symbol)))
    (let [exe (ensure-host)
          server (doto (ServerSocket. 0)
                   (.setReuseAddress true)
                   (.setSoTimeout 60000))
          port (.getLocalPort server)]
      (println (str "[clj-gpui] waiting for host on 127.0.0.1:" port))
      (let [^Process proc (spawn-host! exe port protocol-test?)
            sock (.accept server)
            in (BufferedReader.
                (InputStreamReader. (.getInputStream sock) StandardCharsets/UTF_8))
            out (OutputStreamWriter. (.getOutputStream sock) StandardCharsets/UTF_8)]
        (.setSoTimeout server 0)
        (runtime/bind-connection! {:socket sock :in in :out out})
        (runtime/send-ready!)
        (if protocol-test?
          (let [reader (future (runtime/read-loop in))
                code (.waitFor proc)]
            (future-cancel reader)
            (println (str "[clj-gpui] protocol test host exit " code))
            (System/exit (int code)))
          (try
            (runtime/read-loop in)
            (println "[clj-gpui] host disconnected")
            (finally
              (.destroy proc)
              (System/exit 0))))))))
