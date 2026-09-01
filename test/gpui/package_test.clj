(ns gpui.package-test
  (:require [clojure.java.io :as io]
            [clojure.string :as str]
            [clojure.test :refer [deftest is]]
            [gpui.package :as pkg]))

(deftest xml-escape-encodes-markup
  (is (= "a &amp; b &lt;c&gt;" (pkg/xml-escape "a & b <c>"))))

(deftest load-config-normalizes-app-keys
  (let [f (io/file (System/getProperty "java.io.tmpdir")
                   (str "gpui-edn-" (random-uuid) ".edn"))]
    (try
      (spit f "{:app/name \"demo\" :app/version \"1.2.3\" :app/main demo.app/app :app/id \"com.demo.app\"}")
      (let [cfg (pkg/load-config {:file (.getPath f)})]
        (is (= "demo" (:name cfg)))
        (is (= "1.2.3" (:version cfg)))
        (is (= "demo.app/app" (:main cfg)))
        (is (= "com.demo.app" (:id cfg))))
      (finally
        (.delete f)))))

(deftest load-config-keeps-pipeline-keys
  (let [jar (io/file "/tmp/app.jar")
        cfg (pkg/load-config {:name "demo"
                              :version "1.0.0"
                              :main "demo.app/app"
                              :jar jar
                              :host (io/file "/tmp/host")})]
    (is (= jar (:jar cfg)))
    (is (= "demo" (:name cfg)))))

(deftest info-plist-contains-identity
  (let [plist (pkg/info-plist {:name "cljdu"
                               :title "cljdu"
                               :version "0.1.0"
                               :id "com.gitwyrm.cljdu"})]
    (is (str/includes? plist "CFBundleIdentifier"))
    (is (str/includes? plist "com.gitwyrm.cljdu"))
    (is (str/includes? plist "0.1.0"))
    (is (str/includes? plist "cljdu"))))

(deftest desktop-file-is-valid
  (let [desktop (pkg/desktop-file {:name "cljdu"
                                   :title "cljdu"
                                   :description "Disk usage"})]
    (is (str/starts-with? desktop "[Desktop Entry]"))
    (is (str/includes? desktop "Type=Application"))
    (is (str/includes? desktop "Exec=cljdu"))
    (is (str/includes? desktop "Icon=cljdu"))))

(deftest launcher-script-uses-bundled-runtime
  (let [script (pkg/launcher-script {:name "cljdu" :main "cljdu.app/app"})]
    (is (str/starts-with? script "#!/bin/sh"))
    (is (str/includes? script "CLJ_GPUI_BIN"))
    (is (str/includes? script "gpui.prod"))
    (is (str/includes? script "cljdu.app/app"))
    (is (not (str/includes? script "/usr/bin/java")))
    (is (not (str/includes? script "nrepl")))
    (is (not (str/includes? script "cargo")))))

(deftest debian-control-names-package
  (let [control (pkg/debian-control {:name "cljdu"
                                     :version "0.1.0"
                                     :description "Disk usage"
                                     :maintainer "a <b@c>"})]
    (is (str/includes? control "Package: cljdu"))
    (is (str/includes? control "Architecture: amd64"))))

(deftest linux-arch-is-known
  (let [arch (pkg/linux-arch)]
    (is (string? (:deb arch)))
    (is (string? (:appimage arch)))
    (is (seq (:deb arch)))))
