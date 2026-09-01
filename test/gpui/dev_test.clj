(ns gpui.dev-test
  (:require [clojure.java.io :as io]
            [clojure.test :refer [deftest is]]
            [gpui.dev :as dev]))

(defn- touch-exe
  ^java.io.File [^java.io.File f]
  (.mkdirs (.getParentFile f))
  (spit f "x")
  (.setExecutable f true)
  f)

(deftest locates-host-under-target-triple
  (let [root (doto (io/file (System/getProperty "java.io.tmpdir")
                            (str "clj-gpui-target-" (random-uuid)))
               (.mkdirs))
        bin (touch-exe (io/file root "aarch64-apple-darwin" "release" "clj-gpui"))]
    (try
      (is (= (.getCanonicalFile bin)
             (.getCanonicalFile (dev/locate-host-binary root))))
      (finally
        (.delete bin)
        (.delete (.getParentFile bin))
        (.delete (.getParentFile (.getParentFile bin)))
        (.delete root)))))

(deftest prefers-default-release-over-debug
  (let [root (doto (io/file (System/getProperty "java.io.tmpdir")
                            (str "clj-gpui-target-" (random-uuid)))
               (.mkdirs))
        release (touch-exe (io/file root "release" "clj-gpui"))
        debug (touch-exe (io/file root "debug" "clj-gpui"))]
    (try
      (is (= (.getCanonicalFile release)
             (.getCanonicalFile (dev/locate-host-binary root))))
      (finally
        (.delete release)
        (.delete debug)
        (.delete (.getParentFile release))
        (.delete (.getParentFile debug))
        (.delete root)))))
