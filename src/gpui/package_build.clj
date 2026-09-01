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

(defn jlink-executable
  "Absolute `jlink` from JAVA_HOME / java.home. Never PATH-only lookup.

  Packaging must invoke this path, not the bare `jlink` name: on macOS
  the running JVM often has a valid java.home whose bin directory is
  not on the shell PATH."
  ^String []
  (let [jlink (io/file (java-home) "bin" "jlink")]
    (when-not (.canExecute jlink)
      (throw (ex-info (str "jlink not found at " (.getPath jlink)
                           ". Packaging needs a JDK, not a JRE.")
                      {:jlink (.getPath jlink)})))
    (.getPath jlink)))

(defn jre
  "Build a reduced JRE with jlink into `target/runtime`."
  [opts]
  (let [cfg (if (loaded-config? opts) opts (load-config opts))
        dest (io/file (:target cfg) "runtime")
        jlink (jlink-executable)
        modules (or (:modules opts) default-jlink-modules)]
    (when (.exists dest)
      (sh! ["rm" "-rf" (.getPath dest)] {}))
    (println "[clj-gpui] jlink" jlink "->" (.getPath dest))
    (sh! [jlink
          "--add-modules" (str/join "," modules)
          "--strip-debug"
          "--no-header-files"
          "--no-man-pages"
          "--compress=2"
          "--output" (.getPath dest)]
         {})
    (assoc cfg :runtime dest)))
