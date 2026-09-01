(ns gpui.prod
  "Production entry point for a packaged clj-gpui app.

  Unlike `gpui.dev` this does not start nREPL, does not watch source
  files, and never invokes Cargo. The native host must already exist
  (`CLJ_GPUI_BIN` or the package layout).

  Packaged launchers exec:

    java -cp app.jar gpui.prod my.app/app

  Keep the two-process architecture: this JVM serves the UI protocol
  and spawns the bundled GPUI host."
  (:gen-class)
  (:require [clojure.java.io :as io]
            [gpui.host :as host]
            [gpui.runtime :as runtime]))

(set! *warn-on-reflection* true)

(defn- read-packaged-main
  []
  (when-let [res (io/resource "gpui-app.edn")]
    (try
      (let [cfg (read-string (slurp res))]
        (some-> (or (:main cfg) (:app/main cfg)) str))
      (catch Exception _
        nil))))

(defn- parse-args
  [args]
  (or (first args)
      (host/env "CLJ_GPUI_APP")
      (read-packaged-main)))

(defn -main
  [& args]
  (let [app (parse-args args)]
    (when-not app
      (binding [*out* *err*]
        (println "Usage: java -cp app.jar gpui.prod my.app/app")
        (println "A packaged app also reads :main from gpui-app.edn on the classpath."))
      (System/exit 2))
    (runtime/set-app-symbol! app)
    (runtime/install-render-hook!)
    (try
      (runtime/load-app!)
      (catch Exception e
        (binding [*out* *err*]
          (println "[clj-gpui] failed to load" app ":" (.getMessage e))
          (.printStackTrace e))
        (System/exit 1)))
    (println "[clj-gpui] production" (runtime/app-symbol))
    (host/run-bridge! {:exe (host/require-prod-host)
                       :protocol-test? false})))
