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

(deftest prefers-native-triple-over-foreign
  (let [root (doto (io/file (System/getProperty "java.io.tmpdir")
                            (str "clj-gpui-triples-" (random-uuid)))
               (.mkdirs))
        foreign (touch-exe (io/file root "aarch64-apple-darwin" "release" "clj-gpui"))
        native (touch-exe (io/file root "x86_64-unknown-linux-gnu" "release" "clj-gpui"))]
    (try
      (is (= (.getCanonicalFile native)
             (.getCanonicalFile
              (dev/locate-host-binary
               root
               {:host-triple "x86_64-unknown-linux-gnu"}))))
      (is (= (.getCanonicalFile foreign)
             (.getCanonicalFile
              (dev/locate-host-binary
               root
               {:host-triple "aarch64-apple-darwin"}))))
      (finally
        (.delete foreign)
        (.delete native)
        (.delete (.getParentFile foreign))
        (.delete (.getParentFile native))
        (.delete (.getParentFile (.getParentFile foreign)))
        (.delete (.getParentFile (.getParentFile native)))
        (.delete root)))))

(deftest cargo-build-target-reads-config-toml
  (let [host (doto (io/file (System/getProperty "java.io.tmpdir")
                            (str "clj-gpui-cargo-cfg-" (random-uuid)))
               (.mkdirs))
        cfg (io/file host ".cargo" "config.toml")]
    (try
      (.mkdirs (.getParentFile cfg))
      (spit cfg "[build]\ntarget = \"wasm32-unknown-unknown\"\n")
      (is (= "wasm32-unknown-unknown" (dev/cargo-build-target host)))
      (finally
        (.delete cfg)
        (.delete (.getParentFile cfg))
        (.delete host)))))

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

(deftest host-stale-when-rust-is-newer
  (let [root (doto (io/file (System/getProperty "java.io.tmpdir")
                            (str "clj-gpui-stale-" (random-uuid)))
               (.mkdirs))
        src (doto (io/file root "src") (.mkdirs))
        cargo (io/file root "Cargo.toml")
        rust (io/file src "main.rs")
        bin (io/file root "clj-gpui")]
    (try
      (spit cargo "[package]\nname = \"x\"\n")
      (spit rust "fn main() {}")
      (spit bin "old")
      (let [now (System/currentTimeMillis)]
        (is (.setLastModified bin (- now 60000)))
        (is (.setLastModified cargo (- now 120000)))
        (is (.setLastModified rust now)))
      (is (dev/host-stale? root bin))
      (let [now (System/currentTimeMillis)]
        (is (.setLastModified rust (- now 60000)))
        (is (.setLastModified cargo (- now 60000)))
        (is (.setLastModified bin now)))
      (is (not (dev/host-stale? root bin)))
      (is (some #(.endsWith (.getName ^java.io.File %) "main.rs")
                (dev/host-input-files root)))
      (finally
        (.delete rust)
        (.delete src)
        (.delete cargo)
        (.delete bin)
        (.delete root)))))
