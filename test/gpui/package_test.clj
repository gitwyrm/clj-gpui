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
    (is (str/includes? plist "<key>CFBundleExecutable</key><string>cljdu</string>"))
    (is (str/includes? plist "<key>CFBundleIconFile</key><string>cljdu</string>"))))

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

(deftest launcher-script-is-valid-posix-sh
  (let [script (pkg/launcher-script {:name "cljdu" :main "cljdu.app/app"})
        f (io/file (System/getProperty "java.io.tmpdir")
                   (str "clj-gpui-launch-" (random-uuid) ".sh"))]
    (try
      (spit f script)
      (let [proc (.start (ProcessBuilder. ["sh" "-n" (.getPath f)]))]
        (is (zero? (.waitFor proc)) script))
      (finally
        (.delete f)))))

(defn- sh-n-ok?
  [script]
  (let [f (io/file (System/getProperty "java.io.tmpdir")
                   (str "clj-gpui-shn-" (random-uuid) ".sh"))]
    (try
      (spit f script)
      (zero? (.waitFor (.start (ProcessBuilder. ["sh" "-n" (.getPath f)]))))
      (finally
        (.delete f)))))

(deftest posix-launchers-are-valid-sh
  (is (sh-n-ok? (pkg/launcher-script {:name "My App" :main "my.app/app"})))
  (is (sh-n-ok? (pkg/appimage-apprun {:name "My App"})))
  (is (sh-n-ok? (pkg/deb-wrapper {:name "My App"}))))

(deftest launcher-script-quotes-macos-and-linux-layouts
  (let [script (pkg/launcher-script {:name "My App" :main "my.app/app"})]
    (is (str/includes? script "\"$here/../Resources/My App.jar\""))
    (is (str/includes? script "\"$here/../Resources/runtime/bin/java\""))
    (is (str/includes? script "\"$here/../runtime/bin/java\""))
    (is (str/includes? script "\"$here/../lib/My App.jar\""))
    (is (str/includes? script "export CLJ_GPUI_BIN=\"$host\""))
    (is (str/includes? script "exec \"$java_home/bin/java\""))))

(deftest appimage-apprun-and-deb-wrapper-quote-paths
  (is (str/includes? (pkg/appimage-apprun {:name "My App"})
                     "exec \"$here/usr/bin/My App\""))
  (is (str/includes? (pkg/deb-wrapper {:name "My App"})
                     "export CLJ_GPUI_APP_HOME=\"/usr/lib/My App/bin\""))
  (is (str/includes? (pkg/deb-wrapper {:name "My App"})
                     "exec \"/usr/lib/My App/bin/My App\"")))

(deftest jlink-executable-is-absolute
  (let [path (pkg/jlink-executable)
        f (io/file path)]
    (is (.isAbsolute f))
    (is (.canExecute f))
    (is (str/ends-with? path "jlink"))
    (is (not= "jlink" path))))

(deftest appimagetool-pin-is-versioned
  (is (= "1.9.1" pkg/appimagetool-version))
  (is (string? (get pkg/appimagetool-sha256 "x86_64")))
  (is (string? (get pkg/appimagetool-sha256 "aarch64")))
  (is (str/includes? (pkg/appimagetool-url "x86_64") "/1.9.1/"))
  (is (not (str/includes? (pkg/appimagetool-url "x86_64") "continuous"))))

(deftest collect-license-files-picks-license-and-notice
  (let [dir (io/file (System/getProperty "java.io.tmpdir")
                     (str "clj-gpui-lic-" (random-uuid)))]
    (try
      (.mkdirs dir)
      (spit (io/file dir "LICENSE") "MIT")
      (spit (io/file dir "NOTICE") "notice")
      (spit (io/file dir "THIRD") "extra")
      (let [files (pkg/collect-license-files {:project-dir dir
                                              :license-files ["THIRD"]})]
        (is (= ["LICENSE" "NOTICE" "THIRD"]
               (mapv #(.getName ^java.io.File %) files))))
      (finally
        (doseq [n ["LICENSE" "NOTICE" "THIRD"]]
          (.delete (io/file dir n)))
        (.delete dir)))))

(deftest collect-license-files-skips-missing-defaults
  (let [dir (io/file (System/getProperty "java.io.tmpdir")
                     (str "clj-gpui-lic-empty-" (random-uuid)))]
    (try
      (.mkdirs dir)
      (is (empty? (pkg/collect-license-files {:project-dir dir})))
      (finally
        (.delete dir)))))

(deftest collect-license-files-requires-explicit-extras
  (let [dir (io/file (System/getProperty "java.io.tmpdir")
                     (str "clj-gpui-lic-miss-" (random-uuid)))]
    (try
      (.mkdirs dir)
      (is (thrown? Exception
                   (pkg/collect-license-files {:project-dir dir
                                               :license-files ["NOPE"]})))
      (finally
        (.delete dir)))))

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
