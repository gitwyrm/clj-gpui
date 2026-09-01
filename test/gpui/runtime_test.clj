(ns gpui.runtime-test
  (:require [clojure.java.io :as io]
            [clojure.string :as str]
            [clojure.test :refer [deftest is testing]]
            [gpui.ratom :as r]
            [gpui.runtime :as runtime]
            [gpui.ui :as ui]))

(def ^:private widgets-file
  (io/file "test/gpui/reload_probe/widgets.clj"))

(def ^:private widgets-original
  (slurp widgets-file))

(defn- probe-tree-text [tree]
  (->> (tree-seq :children :children tree)
       (keep :text)
       (str/join "\n")))

(defn- restore-probe! []
  (spit widgets-file widgets-original)
  (reset! @#'runtime/load-error* nil)
  (try
    (require 'gpui.reload-probe.widgets :reload)
    (require 'gpui.reload-probe.app :reload)
    (catch Exception _)))

(deftest require-reload-on-root-does-not-reload-deps
  (restore-probe!)
  (require 'gpui.reload-probe.app :reload)
  (is (= "probe-old" ((requiring-resolve 'gpui.reload-probe.widgets/banner))))
  (try
    (spit widgets-file "(ns gpui.reload-probe.widgets)\n(defn banner [] \"probe-new\")\n")
    (require 'gpui.reload-probe.app :reload)
    (is (= "probe-old" ((requiring-resolve 'gpui.reload-probe.widgets/banner)))
        "(require app :reload) must not reload already-loaded helper namespaces")
    (finally
      (restore-probe!))))

(deftest reload-app-picks-up-helper-namespace
  (restore-probe!)
  (runtime/set-app-symbol! 'gpui.reload-probe.app/app)
  (try
    (runtime/load-app!)
    (is (str/includes? (probe-tree-text (runtime/export-tree)) "probe-old-0"))
    (spit widgets-file "(ns gpui.reload-probe.widgets)\n(defn banner [] \"probe-new\")\n")
    (runtime/reload-app! [widgets-file])
    (is (str/includes? (probe-tree-text (runtime/export-tree)) "probe-new-0"))
    (swap! @(requiring-resolve 'gpui.reload-probe.app/!state) assoc :n 7)
    (runtime/reload-app! [widgets-file])
    (is (str/includes? (probe-tree-text (runtime/export-tree)) "probe-new-7")
        "defonce state survives helper + app reload")
    (finally
      (restore-probe!))))

(deftest reload-compile-error-shows-in-tree-until-fixed
  (restore-probe!)
  (runtime/set-app-symbol! 'gpui.reload-probe.app/app)
  (try
    (runtime/load-app!)
    (spit widgets-file "(ns gpui.reload-probe.widgets)\n(defn banner []\n")
    (is (thrown? Exception (runtime/reload-app! [widgets-file])))
    (let [text (probe-tree-text (runtime/export-tree))]
      (is (str/includes? text "Clojure error"))
      (is (re-find #"reload_probe/widgets" text)))
    (spit widgets-file "(ns gpui.reload-probe.widgets)\n(defn banner [] \"probe-fixed\")\n")
    (runtime/reload-app! [widgets-file])
    (let [text (probe-tree-text (runtime/export-tree))]
      (is (not (str/includes? text "Clojure error")))
      (is (str/includes? text "probe-fixed")))
    (finally
      (restore-probe!))))

(deftest error-tree-includes-compiler-location
  (let [tree (runtime/export-tree
              (fn []
                (throw (ex-info "Syntax error compiling at (my/widgets.clj:4:1)."
                                {:clojure.error/source "my/widgets.clj"
                                 :clojure.error/line 4
                                 :clojure.error/column 1}))))
        text (probe-tree-text tree)]
    (is (str/includes? text "Clojure error"))
    (is (str/includes? text "my/widgets.clj:4:1"))))

(deftest skip-reload-ns-covers-package-fragments
  (is (#'runtime/skip-reload-ns? 'gpui.package))
  (is (#'runtime/skip-reload-ns? 'gpui.package_build))
  (is (#'runtime/skip-reload-ns? 'gpui.package-native))
  (is (#'runtime/skip-reload-ns? 'gpui.package-launch))
  (is (not (#'runtime/skip-reload-ns? 'gpui.ui)))
  (is (not (#'runtime/skip-reload-ns? 'gpui.platform))))

(deftest ns-from-file-uses-path-and-ns-form
  (let [src (doto (io/file (System/getProperty "java.io.tmpdir")
                           (str "clj-gpui-ns-" (random-uuid)))
              (.mkdirs))
        f (io/file src "my" "widgets.clj")]
    (try
      (.mkdirs (.getParentFile f))
      (spit f "(ns my.widgets)\n(defn x [] 1)\n")
      (is (= 'my.widgets (runtime/ns-from-file src f)))
      (spit f "(defn broken")
      (is (= 'my.widgets (runtime/ns-from-file src f))
          "cached ns is used when the file no longer reads")
      (finally
        (.delete f)
        (.delete (.getParentFile f))
        (.delete src)))))

(deftest changed-clj-files-detects-new-and-updated
  (let [a "/tmp/a.clj"
        b "/tmp/b.clj"]
    (is (= #{a b} (set (map #(.getPath %)
                            (runtime/changed-clj-files {a 1} {a 2 b 1})))))
    (is (= #{a} (set (map #(.getPath %)
                          (runtime/changed-clj-files {a 1 b 1} {a 2 b 1})))))
    (is (empty? (runtime/changed-clj-files {a 1} {a 1})))))

(deftest atom-callback-does-not-enqueue-second-render
  (let [buf (java.io.StringWriter.)
        !state (r/atom {:n 0})]
    (runtime/bind-connection! {:out buf})
    (runtime/install-render-hook!)
    (try
      (testing "swap! outside a host callback still requests a render"
        (swap! !state update :n inc)
        (Thread/sleep 40)
        (is (str/includes? (str buf) "request-render")))
      (let [before (str buf)
            exported (runtime/export-tree
                      (fn []
                        (ui/button "+" #(swap! !state update :n inc))))
            id (:on-click exported)]
        (runtime/invoke-callback! id)
        (is (= 2 (:n @!state)))
        (Thread/sleep 40)
        (is (= before (str buf))
            "r/atom watch must not send request-render during callback; the host already renders"))
      (let [before (str buf)
            exported (runtime/export-tree
                      (fn []
                        (ui/button "noop" (fn [] :ok))))
            id (:on-click exported)]
        (runtime/invoke-callback! id)
        (Thread/sleep 40)
        (is (= before (str buf))))
      (finally
        (runtime/bind-connection! nil)
        (ui/set-request-render! nil)))))
