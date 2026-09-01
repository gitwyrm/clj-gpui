(ns gpui.prod-test
  (:require [clojure.java.io :as io]
            [clojure.test :refer [deftest is testing]]
            [gpui.host :as host]
            [gpui.prod :as prod]))

(defn- touch-exe
  ^java.io.File [^java.io.File f]
  (.mkdirs (.getParentFile f))
  (spit f "x")
  (.setExecutable f true)
  f)

(defn- without-prod-env
  "Hide CLJ_GPUI_BIN / CLJ_GPUI_APP_HOME so lookup cannot see this machine."
  [f]
  (let [env host/env]
    (with-redefs [host/env (fn
                             ([k] (host/env k nil))
                             ([k default]
                              (if (#{"CLJ_GPUI_BIN" "CLJ_GPUI_APP_HOME"} k)
                                default
                                (env k default))))]
      (f))))

(deftest prod-does-not-auto-build
  (let [cargo (atom 0)]
    (with-redefs [host/build-host! (fn [& _]
                                     (swap! cargo inc)
                                     (throw (ex-info "cargo must not run in production lookup" {})))]
      (testing "lookup never invokes Cargo even if a host binary already exists"
        (host/locate-prod-host)
        (is (zero? @cargo)
            "gpui.prod / locate-prod-host must never invoke Cargo"))
      (testing "isolated filesystem: missing binary does not build"
        (let [tmp (doto (io/file (System/getProperty "java.io.tmpdir")
                                 (str "clj-gpui-prod-" (random-uuid)))
                    (.mkdirs))
              fake-home (doto (io/file tmp "jre") (.mkdirs))
              orig-user-dir (System/getProperty "user.dir")
              orig-java-home (System/getProperty "java.home")]
          (try
            (System/setProperty "user.dir" (.getPath tmp))
            (System/setProperty "java.home" (.getPath fake-home))
            (without-prod-env
             (fn []
               (is (nil? (host/locate-prod-host)))
               (is (thrown-with-msg? clojure.lang.ExceptionInfo
                                     #"host binary not found"
                                     (host/require-prod-host)))
               (is (zero? @cargo))))
            (finally
              (System/setProperty "user.dir" orig-user-dir)
              (when orig-java-home
                (System/setProperty "java.home" orig-java-home))
              (.delete fake-home)
              (.delete tmp)))))
      (testing "explicit CLJ_GPUI_BIN is found without cargo"
        (let [tmp (doto (io/file (System/getProperty "java.io.tmpdir")
                                 (str "clj-gpui-prod-bin-" (random-uuid)))
                    (.mkdirs))
              bin (touch-exe (io/file tmp "clj-gpui"))]
          (try
            (let [env host/env]
              (with-redefs [host/env (fn
                                       ([k] (host/env k nil))
                                       ([k default]
                                        (case k
                                          "CLJ_GPUI_BIN" (.getPath bin)
                                          "CLJ_GPUI_APP_HOME" default
                                          (env k default))))]
                (is (= (.getCanonicalFile bin)
                       (.getCanonicalFile (host/locate-prod-host))))
                (is (zero? @cargo))))
            (finally
              (.delete bin)
              (.delete tmp))))))))

(deftest prod-has-main
  (is (ifn? (ns-resolve 'gpui.prod '-main))))
