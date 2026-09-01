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
        app (io/file (:target cfg) "package" (str (:name cfg) ".app"))]
    (when (.exists app)
      (sh! ["rm" "-rf" (.getPath app)] {}))
    (let [contents (io/file app "Contents")
          macos (mkdirp (io/file contents "MacOS"))
          resources (mkdirp (io/file contents "Resources"))]
      (spit (io/file contents "Info.plist") (info-plist cfg))
      (write-launcher cfg (io/file macos (:name cfg)))
      (copy-host (:host cfg) (io/file macos "clj-gpui-host"))
      (copy-file (:jar cfg) (io/file resources (str (:name cfg) ".jar")))
      (sh! ["cp" "-R" (.getPath ^java.io.File (:runtime cfg)) (.getPath (io/file resources "runtime"))] {})
      (copy-icon cfg (io/file resources (str (:name cfg) ".png")))
      (maybe-icns cfg resources)
      (println "[clj-gpui] wrote" (.getPath ^java.io.File app))
      (assoc cfg :app app :outputs [app]))))

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
        appdir (io/file pkg (str (:name cfg) ".AppDir"))]
    (when (.exists appdir)
      (sh! ["rm" "-rf" (.getPath appdir)] {}))
    (let [usr (io/file appdir "usr")
          bin (mkdirp (io/file usr "bin"))
          share-app (mkdirp (io/file usr "share" "applications"))
          share-icon (mkdirp (io/file usr "share" "icons" "hicolor" "256x256" "apps"))
          arch (linux-arch)
          out (io/file pkg (str (:name cfg) "-" (:version cfg) "-" (:appimage arch) ".AppImage"))]
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
      (assoc cfg :appimage out :appdir appdir))))

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
        deb (io/file pkg (str (:name cfg) "_" (:version cfg) "_" arch ".deb"))
        cfg (assoc cfg :arch arch)]
    (when (.exists root)
      (sh! ["rm" "-rf" (.getPath root)] {}))
    (let [debian (mkdirp (io/file root "DEBIAN"))
          bin (mkdirp (io/file root "usr" "bin"))
          lib (mkdirp (io/file root "usr" "lib" (:name cfg)))
          lib-bin (mkdirp (io/file lib "bin"))
          share-app (mkdirp (io/file root "usr" "share" "applications"))
          share-icon (mkdirp (io/file root "usr" "share" "icons" "hicolor" "256x256" "apps"))]
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
      (assoc cfg :deb deb :deb-root root))))

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
