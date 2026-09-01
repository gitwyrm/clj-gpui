(ns gpui.dev
  "Clojure-first development entry point for clj-gpui.

  Starts nREPL, watches app sources, listens for the native host, then
  launches the GPUI binary. Application authors run:

    clj -M:dev my.app/app

  The host is built with Cargo when `host/` is present and either the
  binary is missing or a host source file is newer than the binary.

  Production packages use `gpui.prod` instead: no nREPL, no watcher,
  no Cargo."
  (:require [gpui.host :as host]
            [gpui.runtime :as runtime]))

(set! *warn-on-reflection* true)

(def locate-host-binary host/locate-host-binary)
(def host-binary-candidates host/host-binary-candidates)
(def cargo-build-target host/cargo-build-target)
(def host-stale? host/host-stale?)
(def host-input-files host/host-input-files)

(defn- parse-args
  [args]
  (let [protocol-test? (boolean (some #{"--protocol-test"} args))
        rest (vec (remove #{"--protocol-test"} args))
        app (or (first rest) (host/env "CLJ_GPUI_APP"))]
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
      (println (str "[clj-gpui] hot reload watching " (host/env "CLJ_GPUI_SRC" "src")))
      (println "[clj-gpui] root UI var" (runtime/app-symbol)))
    (host/run-bridge! {:exe (host/ensure-dev-host)
                       :protocol-test? protocol-test?})))
