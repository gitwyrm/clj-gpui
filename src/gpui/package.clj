(ns gpui.package
  "Reusable packaging for clj-gpui applications.

  Application config lives in `gpui.edn` at the project root:

    {:name \"cljdu\"
     :version \"0.1.0\"
     :main cljdu.app/app
     :id \"com.example.cljdu\"
     :icon \"resources/icon.png\"
     :title \"cljdu\"
     :description \"A native disk usage browser\"}

  Then, with a `:build` alias whose `:ns-default` is `gpui.package`
  and tools.build as `:extra-deps` (so the project still sees clj-gpui):

    clj -X:build package

  That command is native-only: macOS produces a `.app`, Linux produces
  an AppImage and a `.deb`. It never cross-compiles."
  (:require [clojure.edn :as edn]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [gpui.host :as host])
  (:import [java.lang ProcessBuilder$Redirect]
           [java.util ArrayList]))

(set! *warn-on-reflection* true)

(def default-jlink-modules
  "JDK modules sufficient for Clojure + a headless JVM talking to the host."
  ["java.base"
   "java.datatransfer"
   "java.desktop"
   "java.logging"
   "java.management"
   "java.naming"
   "java.prefs"
   "java.security.jgss"
   "java.sql"
   "java.xml"
   "java.instrument"
   "jdk.unsupported"
   "jdk.unsupported.desktop"
   "jdk.zipfs"
   "jdk.crypto.ec"
   "jdk.crypto.cryptoki"
   "jdk.localedata"
   "jdk.management"])

(defn xml-escape
  [s]
  (-> (str s)
      (str/replace "&" "&amp;")
      (str/replace "<" "&lt;")
      (str/replace ">" "&gt;")
      (str/replace "\"" "&quot;")
      (str/replace "'" "&apos;")))

(defn os-key
  []
  (let [n (str/lower-case (System/getProperty "os.name" ""))]
    (cond
      (str/includes? n "mac") :macos
      (str/includes? n "linux") :linux
      (str/includes? n "win") :windows
      :else :unknown)))

(defn- as-str
  [x]
  (cond
    (keyword? x) (name x)
    (symbol? x) (str x)
    (nil? x) nil
    :else (str x)))

(defn load-config
  "Read `gpui.edn` (or `:config` / `:file` in opts) and normalize keys."
  ([]
   (load-config {}))
  ([opts]
   (let [file (io/file (or (:file opts)
                           (:config opts)
                           "gpui.edn"))
         raw (if (:name opts)
               opts
               (do
                 (when-not (.isFile file)
                   (throw (ex-info (str "Missing packaging config " (.getPath file)
                                        ". Write a gpui.edn or pass :file.")
                                   {:file (.getPath file)})))
                 (edn/read-string (slurp file))))
         name (or (as-str (or (:name raw) (:app/name raw)))
                  (throw (ex-info "gpui.edn needs :name" {:config raw})))
         main (or (as-str (or (:main raw) (:app/main raw)))
                  (throw (ex-info "gpui.edn needs :main (e.g. my.app/app)" {:config raw})))
         version (or (as-str (or (:version raw) (:app/version raw))) "0.1.0")
         id (or (as-str (or (:id raw) (:app/id raw)))
                (str "com.cljgpui." name))
         icon (some-> (or (:icon raw) (:app/icon raw)) as-str)
         title (or (as-str (or (:title raw) (:app/title raw))) name)
         description (or (as-str (or (:description raw) (:app/description raw)))
                         title)
         maintainer (or (as-str (or (:maintainer raw) (:app/maintainer raw)))
                        "clj-gpui packager <nobody@example.com>")
         target (io/file (or (:target-dir raw) (:target-dir opts) "target"))]
     (merge
      (dissoc raw :app/name :app/version :app/main :app/id :app/icon
              :app/title :app/description :app/maintainer)
      {:name name
       :version version
       :main main
       :id id
       :icon icon
       :title title
       :description description
       :maintainer maintainer
       :target target}))))

(defn- sh!
  [args {:keys [dir env]}]
  (let [list (doto (ArrayList.)
               (.addAll args))
        pb (doto (ProcessBuilder. list)
             (.inheritIO))
        env-map (.environment pb)]
    (when dir
      (.directory pb (io/file dir)))
    (doseq [[k v] env]
      (.put env-map (str k) (str v)))
    (let [code (.waitFor (.start pb))]
      (when-not (zero? code)
        (throw (ex-info (str "command failed with exit " code ": " (str/join " " args))
                        {:args args :code code})))
      code)))

(defn- capture!
  [args {:keys [dir]}]
  (let [list (doto (ArrayList.)
               (.addAll args))
        pb (doto (ProcessBuilder. list)
             (.redirectError ProcessBuilder$Redirect/INHERIT))
        _ (when dir (.directory pb (io/file dir)))
        proc (.start pb)
        out (slurp (.getInputStream proc))
        code (.waitFor proc)]
    (when-not (zero? code)
      (throw (ex-info (str "command failed with exit " code ": " (str/join " " args))
                      {:args args :code code :out out})))
    out))

(defn- mkdirp
  [^java.io.File dir]
  (.mkdirs dir)
  dir)

(defn- copy-file
  [^java.io.File src ^java.io.File dest]
  (mkdirp (.getParentFile dest))
  (io/copy src dest)
  dest)

(defn- chmod-exec
  [^java.io.File f]
  (.setExecutable f true false)
  f)

(defn- require-tools-build []
  (try
    (requiring-resolve 'clojure.tools.build.api/create-basis)
    (catch Exception e
      (throw (ex-info
              (str "Packaging needs tools.build on the classpath.\n"
                   "Add a :build alias:\n"
                   "  :build {:extra-deps {io.github.clojure/tools.build {:mvn/version \"0.10.10\"}}\n"
                   "          :ns-default gpui.package\n"
                   "          :exec-fn gpui.package/package}")
              {} e)))))

(defn uberjar
  "Compile `gpui.prod` and emit an uberjar under `target/`."
  [opts]
  (let [cfg (load-config opts)
        _ (require-tools-build)
        b-create (requiring-resolve 'clojure.tools.build.api/create-basis)
        b-delete (requiring-resolve 'clojure.tools.build.api/delete)
        b-copy-dir (requiring-resolve 'clojure.tools.build.api/copy-dir)
        b-copy-file (requiring-resolve 'clojure.tools.build.api/copy-file)
        b-compile (requiring-resolve 'clojure.tools.build.api/compile-clj)
        b-uber (requiring-resolve 'clojure.tools.build.api/uber)
        class-dir (str (io/file (:target cfg) "classes"))
        jar-file (str (io/file (:target cfg) (str (:name cfg) ".jar")))
        src-dirs (filterv #(.isDirectory (io/file %)) ["src" "resources"])
        basis (b-create {:project "deps.edn"})]
    (b-delete {:path class-dir})
    (when (seq src-dirs)
      (b-copy-dir {:src-dirs src-dirs :target-dir class-dir}))
    (when (.isFile (io/file "gpui.edn"))
      (b-copy-file {:src "gpui.edn" :target (str (io/file class-dir "gpui-app.edn"))}))
    (println "[clj-gpui] compiling gpui.prod")
    (b-compile {:basis basis
                :ns-compile '[gpui.prod]
                :class-dir class-dir})
    (println "[clj-gpui] writing" jar-file)
    (b-uber {:class-dir class-dir
             :uber-file jar-file
             :basis basis
             :main 'gpui.prod})
    (assoc cfg :jar (io/file jar-file))))

(defn- loaded-config?
  [opts]
  (and (map? opts) (string? (:name opts)) (string? (:main opts))))

(defn host
  "Build the native GPUI host (`cargo build --release`) and return its path.

  Packaging always rebuilds with `--release` unless `CLJ_GPUI_BIN` points at
  an existing binary. Development `ensure-dev-host` may otherwise reuse a
  debug build that happens to be newer than sources."
  [opts]
  (let [cfg (if (loaded-config? opts) opts (load-config (if (map? opts) opts {})))
        exe (if (seq (host/env "CLJ_GPUI_BIN"))
              (host/ensure-dev-host)
              (do
                (host/build-host!)
                (host/ensure-dev-host)))]
    (println "[clj-gpui] host" (.getPath exe))
    (assoc cfg :host exe)))

(defn- java-home
  []
  (or (System/getenv "JAVA_HOME")
      (System/getProperty "java.home")))

(defn jre
  "Build a reduced JRE with jlink into `target/runtime`."
  [opts]
  (let [cfg (if (loaded-config? opts) opts (load-config opts))
        dest (io/file (:target cfg) "runtime")
        jlink (io/file (java-home) "bin" "jlink")
        modules (or (:modules opts) default-jlink-modules)]
    (when-not (.canExecute jlink)
      (throw (ex-info (str "jlink not found at " (.getPath jlink)
                           ". Packaging needs a JDK, not a JRE.")
                      {:jlink (.getPath jlink)})))
    (when (.exists dest)
      (sh! ["rm" "-rf" (.getPath dest)] {}))
    (println "[clj-gpui] jlink ->" (.getPath dest))
    (sh! ["jlink"
          "--add-modules" (str/join "," modules)
          "--strip-debug"
          "--no-header-files"
          "--no-man-pages"
          "--compress=2"
          "--output" (.getPath dest)]
         {})
    (assoc cfg :runtime dest)))

(defn launcher-script
  "POSIX launcher that starts the bundled JVM, which then starts the host."
  [{:keys [name main]}]
  (str "#!/bin/sh\n"
       "set -eu\n"
       "here=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n"
       "app_home=${CLJ_GPUI_APP_HOME:-$here}\n"
       "# Walk up from MacOS/ or bin/ to the runtime/jar layout.\n"
       "if [ -x \"$here/clj-gpui-host\" ]; then host=\"$here/clj-gpui-host\"\n"
       "elif [ -x \"$app_home/clj-gpui-host\" ]; then host=\"$app_home/clj-gpui-host\"\n"
       "elif [ -x \"$app_home/bin/clj-gpui-host\" ]; then host=\"$app_home/bin/clj-gpui-host\"\n"
       "else echo \"$0: bundled clj-gpui host not found\" >&2; exit 1; fi\n"
       "if [ -x \"$here/../runtime/bin/java\" ]; then java_home=\"$here/../runtime\"\n"
       "elif [ -x \"$here/../Resources/runtime/bin/java\" ]; then java_home=\"$here/../Resources/runtime\"\n"
       "elif [ -x \"$app_home/runtime/bin/java\" ]; then java_home=\"$app_home/runtime\"\n"
       "else echo \"$0: bundled Java runtime not found\" >&2; exit 1; fi\n"
       "if [ -f \"$here/../Resources/" name ".jar\" ]; then jar=\"$here/../Resources/" name ".jar\"\n"
       "elif [ -f \"$here/../lib/" name ".jar\" ]; then jar=\"$here/../lib/" name ".jar\"\n"
       "elif [ -f \"$app_home/lib/" name ".jar\" ]; then jar=\"$app_home/lib/" name ".jar\"\n"
       "elif [ -f \"$here/" name ".jar\" ]; then jar=\"$here/" name ".jar\"\n"
       "else echo \"$0: bundled application jar not found\" >&2; exit 1; fi\n"
       "export CLJ_GPUI_BIN=\"$host\"\n"
       "export CLJ_GPUI_APP_HOME=\"$(CDPATH= cd -- \"$(dirname -- \"$host\")\" && pwd)\n"
       "export JAVA_HOME=\"$java_home\"\n"
       "exec \"$java_home/bin/java\" -Djava.awt.headless=true -cp \"$jar\" gpui.prod " main "\n"))

(defn info-plist
  [{:keys [name title version id]}]
  (str "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
       "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" "
       "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n"
       "<plist version=\"1.0\">\n"
       "<dict>\n"
       "  <key>CFBundleName</key><string>" (xml-escape name) "</string>\n"
       "  <key>CFBundleDisplayName</key><string>" (xml-escape title) "</string>\n"
       "  <key>CFBundleIdentifier</key><string>" (xml-escape id) "</string>\n"
       "  <key>CFBundleVersion</key><string>" (xml-escape version) "</string>\n"
       "  <key>CFBundleShortVersionString</key><string>" (xml-escape version) "</string>\n"
       "  <key>CFBundleExecutable</key><string>" (xml-escape name) "</string>\n"
       "  <key>CFBundleIconFile</key><string>" (xml-escape name) "</string>\n"
       "  <key>CFBundlePackageType</key><string>APPL</string>\n"
       "  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>\n"
       "  <key>LSMinimumSystemVersion</key><string>12.0</string>\n"
       "  <key>NSHighResolutionCapable</key><true/>\n"
       "  <key>NSSupportsAutomaticTermination</key><false/>\n"
       "</dict>\n"
       "</plist>\n"))

(defn desktop-file
  [{:keys [name title description]}]
  (str "[Desktop Entry]\n"
       "Type=Application\n"
       "Name=" title "\n"
       "Comment=" description "\n"
       "Exec=" name "\n"
       "Icon=" name "\n"
       "Terminal=false\n"
       "Categories=Utility;Filesystem;\n"
       "StartupNotify=true\n"))

(defn- copy-host
  [^java.io.File exe ^java.io.File dest]
  (copy-file exe dest)
  (chmod-exec dest)
  (when (= (os-key) :linux)
    (try (sh! ["strip" (.getPath dest)] {}) (catch Exception _)))
  dest)

(defn- copy-icon
  [{:keys [icon name]} ^java.io.File dest-png]
  (when icon
    (let [src (io/file icon)]
      (when-not (.isFile src)
        (throw (ex-info (str "icon not found: " icon) {:icon icon})))
      (copy-file src dest-png)
      dest-png)))

(defn- maybe-icns
  [{:keys [name icon]} ^java.io.File resources]
  (when (and icon (= (os-key) :macos))
    (let [iconset (io/file resources (str name ".iconset"))
          png (io/file icon)
          icns (io/file resources (str name ".icns"))]
      (mkdirp iconset)
      (doseq [[out size] [["icon_16x16.png" 16]
                          ["icon_16x16@2x.png" 32]
                          ["icon_32x32.png" 32]
                          ["icon_32x32@2x.png" 64]
                          ["icon_128x128.png" 128]
                          ["icon_128x128@2x.png" 256]
                          ["icon_256x256.png" 256]
                          ["icon_256x256@2x.png" 512]
                          ["icon_512x512.png" 512]
                          ["icon_512x512@2x.png" 1024]]
              :let [dest (io/file iconset out)]]
        (try
          (sh! ["sips" "-z" (str size) (str size) (.getPath png) "--out" (.getPath dest)] {})
          (catch Exception _
            (copy-file png dest))))
      (try
        (sh! ["iconutil" "-c" "icns" (.getPath iconset) "-o" (.getPath icns)] {})
        (catch Exception _
          (println "[clj-gpui] iconutil not available; copying PNG as icon")))
      icns)))

(defn- write-launcher
  [cfg ^java.io.File dest]
  (spit dest (launcher-script cfg))
  (chmod-exec dest)
  dest)

(defn package-macos
  "Assemble `Name.app` under `target/package/`."
  [opts]
  (when (not= (os-key) :macos)
    (throw (ex-info "macOS .app packaging must run on macOS." {:os (os-key)})))
  (let [cfg (-> opts uberjar host jre)
        app (io/file (:target cfg) "package" (str (:name cfg) ".app"))
        contents (io/file app "Contents")
        macos (mkdirp (io/file contents "MacOS"))
        resources (mkdirp (io/file contents "Resources"))]
    (when (.exists app)
      (sh! ["rm" "-rf" (.getPath app)] {}))
    (mkdirp macos)
    (mkdirp resources)
    (spit (io/file contents "Info.plist") (info-plist cfg))
    (write-launcher cfg (io/file macos (:name cfg)))
    (copy-host (:host cfg) (io/file macos "clj-gpui-host"))
    (copy-file (:jar cfg) (io/file resources (str (:name cfg) ".jar")))
    (sh! ["cp" "-R" (.getPath ^java.io.File (:runtime cfg)) (.getPath (io/file resources "runtime"))] {})
    (copy-icon cfg (io/file resources (str (:name cfg) ".png")))
    (maybe-icns cfg resources)
    (println "[clj-gpui] wrote" (.getPath ^java.io.File app))
    (assoc cfg :app app :outputs [app])))

(defn linux-arch
  "Debian and AppImage architecture names for this JVM."
  []
  (let [a (str/lower-case (System/getProperty "os.arch" ""))]
    (cond
      (contains? #{"amd64" "x86_64"} a) {:deb "amd64" :appimage "x86_64"}
      (contains? #{"aarch64" "arm64"} a) {:deb "arm64" :appimage "aarch64"}
      :else {:deb a :appimage a})))

(defn- ensure-appimagetool
  ^java.io.File [^java.io.File target appimage-arch]
  (let [tool (io/file target (str "appimagetool-" appimage-arch))
        url (str "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-"
                 appimage-arch ".AppImage")]
    (if (.canExecute tool)
      tool
      (do
        (println "[clj-gpui] downloading appimagetool")
        (sh! ["curl" "-fsSL" "-o" (.getPath tool) url] {})
        (chmod-exec tool)
        tool))))

(defn package-appimage
  [opts]
  (when (not= (os-key) :linux)
    (throw (ex-info "AppImage packaging must run on Linux." {:os (os-key)})))
  (let [cfg (-> opts uberjar host jre)
        pkg (mkdirp (io/file (:target cfg) "package"))
        appdir (io/file pkg (str (:name cfg) ".AppDir"))
        usr (io/file appdir "usr")
        bin (mkdirp (io/file usr "bin"))
        share-app (mkdirp (io/file usr "share" "applications"))
        share-icon (mkdirp (io/file usr "share" "icons" "hicolor" "256x256" "apps"))
        arch (linux-arch)
        out (io/file pkg (str (:name cfg) "-" (:version cfg) "-" (:appimage arch) ".AppImage"))]
    (when (.exists appdir)
      (sh! ["rm" "-rf" (.getPath appdir)] {}))
    (mkdirp bin)
    (write-launcher cfg (io/file bin (:name cfg)))
    (copy-host (:host cfg) (io/file bin "clj-gpui-host"))
    (copy-file (:jar cfg) (io/file (mkdirp (io/file usr "lib")) (str (:name cfg) ".jar")))
    (sh! ["cp" "-R" (.getPath ^java.io.File (:runtime cfg)) (.getPath (io/file usr "runtime"))] {})
    (let [desktop (desktop-file cfg)]
      (spit (io/file share-app (str (:name cfg) ".desktop")) desktop)
      (spit (io/file appdir (str (:name cfg) ".desktop")) desktop))
    (when-let [png (copy-icon cfg (io/file share-icon (str (:name cfg) ".png")))]
      (copy-file png (io/file appdir (str (:name cfg) ".png")))
      (try
        (.delete (io/file appdir ".DirIcon"))
        (catch Exception _))
      (copy-file png (io/file appdir ".DirIcon")))
    (spit (io/file appdir "AppRun")
          (str "#!/bin/sh\n"
               "set -eu\n"
               "here=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n"
               "export CLJ_GPUI_APP_HOME=\"$here/usr/bin\"\n"
               "exec \"$here/usr/bin/" (:name cfg) "\" \"$@\"\n"))
    (chmod-exec (io/file appdir "AppRun"))
    (let [tool (ensure-appimagetool (:target cfg) (:appimage arch))]
      (println "[clj-gpui] appimagetool" (.getPath ^java.io.File out))
      (sh! [(.getPath tool) "--no-appstream" (.getPath appdir) (.getPath out)]
           {:env {"APPIMAGE_EXTRACT_AND_RUN" "1"
                  "ARCH" (:appimage arch)}}))
    (chmod-exec out)
    (println "[clj-gpui] wrote" (.getPath ^java.io.File out))
    (assoc cfg :appimage out :appdir appdir)))

(defn debian-control
  [{:keys [name version description maintainer arch]}]
  (str "Package: " name "\n"
       "Version: " version "\n"
       "Section: utils\n"
       "Priority: optional\n"
       "Architecture: " (or arch (:deb (linux-arch))) "\n"
       "Maintainer: " maintainer "\n"
       "Depends: libc6, libvulkan1, libxkbcommon0, libwayland-client0 | libx11-6\n"
       "Description: " description "\n"
       " Native GPUI application packaged with a bundled Java runtime\n"
       " and GPUI host. Development tools (Cargo, Clojure CLI, JDK) are\n"
       " not required at runtime.\n"))

(defn package-deb
  [opts]
  (when (not= (os-key) :linux)
    (throw (ex-info ".deb packaging must run on Linux." {:os (os-key)})))
  (let [cfg (if (:jar opts)
              opts
              (-> opts uberjar host jre))
        arch (:deb (linux-arch))
        pkg (mkdirp (io/file (:target cfg) "package"))
        root (io/file pkg (str (:name cfg) "_" (:version cfg) "_" arch))
        debian (mkdirp (io/file root "DEBIAN"))
        bin (mkdirp (io/file root "usr" "bin"))
        lib (mkdirp (io/file root "usr" "lib" (:name cfg)))
        lib-bin (mkdirp (io/file lib "bin"))
        share-app (mkdirp (io/file root "usr" "share" "applications"))
        share-icon (mkdirp (io/file root "usr" "share" "icons" "hicolor" "256x256" "apps"))
        deb (io/file pkg (str (:name cfg) "_" (:version cfg) "_" arch ".deb"))
        cfg (assoc cfg :arch arch)]
    (when (.exists root)
      (sh! ["rm" "-rf" (.getPath root)] {}))
    (mkdirp debian)
    (mkdirp lib-bin)
    (spit (io/file debian "control") (debian-control cfg))
    (write-launcher cfg (io/file lib-bin (:name cfg)))
    (copy-host (:host cfg) (io/file lib-bin "clj-gpui-host"))
    (copy-file (:jar cfg) (io/file (mkdirp (io/file lib "lib")) (str (:name cfg) ".jar")))
    (sh! ["cp" "-R" (.getPath ^java.io.File (:runtime cfg)) (.getPath (io/file lib "runtime"))] {})
    (spit (io/file bin (:name cfg))
          (str "#!/bin/sh\n"
               "set -eu\n"
               "export CLJ_GPUI_APP_HOME=/usr/lib/" (:name cfg) "/bin\n"
               "exec /usr/lib/" (:name cfg) "/bin/" (:name cfg) " \"$@\"\n"))
    (chmod-exec (io/file bin (:name cfg)))
    (spit (io/file share-app (str (:name cfg) ".desktop"))
          (desktop-file cfg))
    (copy-icon cfg (io/file share-icon (str (:name cfg) ".png")))
    (println "[clj-gpui] dpkg-deb" (.getPath ^java.io.File deb))
    (sh! ["fakeroot" "dpkg-deb" "--build" (.getPath root) (.getPath ^java.io.File deb)] {})
    (println "[clj-gpui] wrote" (.getPath ^java.io.File deb))
    (assoc cfg :deb deb :deb-root root)))

(defn package-linux
  "AppImage and .deb for the current Linux host."
  [opts]
  (let [cfg (package-appimage opts)
        cfg (package-deb cfg)]
    (println "[clj-gpui] linux packages:")
    (println " " (.getPath ^java.io.File (:appimage cfg)))
    (println " " (.getPath ^java.io.File (:deb cfg)))
    cfg))

(defn package
  "Build a native package for this OS. See `gpui.edn`."
  [opts]
  (case (os-key)
    :macos (package-macos opts)
    :linux (package-linux opts)
    (throw (ex-info (str "No packager for " (os-key)
                         ". clj-gpui currently packages macOS and Linux natively.")
                    {:os (os-key)}))))
