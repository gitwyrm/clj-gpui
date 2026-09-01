(ns gpui.host
  "Locate, build, and spawn the native GPUI host.

  Shared by `gpui.dev` (Cargo auto-build) and `gpui.prod` (bundled binary)."
  (:require [clojure.data.json :as json]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [gpui.runtime :as runtime])
  (:import [java.io BufferedReader File InputStreamReader OutputStreamWriter]
           [java.lang ProcessBuilder$Redirect]
           [java.net ServerSocket]
           [java.nio.charset StandardCharsets]
           [java.util ArrayList]))

(set! *warn-on-reflection* true)

(def host-bin-names
  ["clj-gpui" "clj-gpui.exe" "clj-gpui-host" "clj-gpui-host.exe"])

(defn env
  ([k] (env k nil))
  ([k default]
   (or (System/getenv k) default)))

(defn library-root
  "Directory that contains this library's deps.edn and host/ crate.

  Works for a source checkout and for a Clojure git dep (`~/.gitlibs`).
  Returns nil when the code is loaded from a JAR (production)."
  ^java.io.File []
  (if-let [explicit (env "CLJ_GPUI_ROOT")]
    (io/file explicit)
    (when-let [res (io/resource "gpui/host.clj")]
      (try
        (let [uri (.toURI res)]
          (when (= "file" (.getScheme uri))
            (let [file (io/file uri)]
              (when (.exists file)
                (-> file .getParentFile .getParentFile .getParentFile)))))
        (catch Exception _
          nil)))))

(defn- cargo-target-dir
  "Ask Cargo where artifacts go (honours CARGO_TARGET_DIR and config)."
  ^java.io.File [^java.io.File host-dir]
  (try
    (let [args (doto (ArrayList.)
                 (.add "cargo")
                 (.add "metadata")
                 (.add "--format-version")
                 (.add "1")
                 (.add "--no-deps"))
          pb (doto (ProcessBuilder. args)
               (.directory host-dir)
               (.redirectError ProcessBuilder$Redirect/DISCARD))
          ^Process proc (.start pb)
          out (slurp (.getInputStream proc))
          code (.waitFor proc)]
      (when (zero? code)
        (when-let [dir (get (json/read-str out) "target_directory")]
          (io/file dir))))
    (catch Exception _
      nil)))

(defn- profile-bins
  [^java.io.File dir]
  (mapcat (fn [name]
            [(io/file dir "release" name)
             (io/file dir "debug" name)])
          host-bin-names))

(defn rustc-host-triple
  "The rustc host target triple, or nil if rustc is unavailable."
  []
  (try
    (let [args (doto (ArrayList.)
                 (.add "rustc")
                 (.add "-vV"))
          pb (doto (ProcessBuilder. args)
               (.redirectError ProcessBuilder$Redirect/DISCARD))
          ^Process proc (.start pb)
          out (slurp (.getInputStream proc))]
      (when (zero? (.waitFor proc))
        (some (fn [line]
                (when (str/starts-with? line "host: ")
                  (subs line 6)))
              (str/split-lines out))))
    (catch Exception _
      nil)))

(defn- parse-cargo-build-target
  [^java.io.File f]
  (when (and f (.isFile f))
    (try
      (let [in-build? (atom false)]
        (some (fn [line]
                (let [t (str/trim line)]
                  (cond
                    (re-matches #"\[build\]" t)
                    (do (reset! in-build? true) nil)
                    (re-find #"^\[" t)
                    (do (reset! in-build? false) nil)
                    (and @in-build? (re-find #"^target\s*=" t))
                    (or (second (re-find #"target\s*=\s*\"([^\"]+)\"" t))
                        (second (re-find #"target\s*=\s*'([^']+)'" t))))))
              (str/split-lines (slurp f))))
      (catch Exception _
        nil))))

(defn cargo-build-target
  "Configured Cargo `--target`, from `CARGO_BUILD_TARGET` or `.cargo/config.toml`."
  [^java.io.File host-dir]
  (or (env "CARGO_BUILD_TARGET")
      (when host-dir
        (some parse-cargo-build-target
              [(io/file host-dir ".cargo" "config.toml")
               (io/file host-dir ".cargo" "config")
               (when-let [parent (.getParentFile host-dir)]
                 (io/file parent ".cargo" "config.toml"))
               (when-let [parent (.getParentFile host-dir)]
                 (io/file parent ".cargo" "config"))]))))

(defn- target-subdirs
  [^java.io.File target-dir]
  (when (and target-dir (.isDirectory target-dir))
    (->> (or (.listFiles target-dir) (into-array java.io.File []))
         (filterv #(.isDirectory ^java.io.File %))
         (sort-by #(.getName ^java.io.File %)))))

(defn host-binary-candidates
  "Possible Cargo output paths for the clj-gpui host.

  Includes `target/release`, `target/debug`, and `target/<triple>/{release,debug}`
  so a `[build] target` in `.cargo/config.toml` or `CARGO_BUILD_TARGET` still
  resolves after `cargo build --release`.

  When several triples are present and `target/release` is missing, prefer the
  rustc host triple over directory-iteration order so a leftover cross-compile
  is not chosen first."
  ([target-dir]
   (host-binary-candidates target-dir nil))
  ([target-dir {:keys [configured-target host-triple]}]
   (let [configured (or configured-target (env "CARGO_BUILD_TARGET"))
         host (or host-triple (rustc-host-triple))
         children (or (target-subdirs target-dir) [])
         preferred (set (filter seq [configured host]))
         rest-children (remove (fn [^java.io.File d]
                                 (contains? preferred (.getName d)))
                               children)]
     (->> (concat
           (when (seq configured)
             (profile-bins (io/file target-dir configured)))
           (profile-bins target-dir)
           (when (seq host)
             (profile-bins (io/file target-dir host)))
           (mapcat profile-bins rest-children))
          (filter some?)
          distinct
          vec))))

(defn locate-host-binary
  "Return the first executable clj-gpui host under a Cargo target directory."
  (^java.io.File [^java.io.File target-dir]
   (locate-host-binary target-dir nil))
  (^java.io.File [^java.io.File target-dir prefs]
   (when target-dir
     (first (filter (fn [^java.io.File f]
                      (and (.isFile f) (.canExecute f)))
                    (host-binary-candidates target-dir prefs))))))

(defn host-input-files
  "Cargo manifest and Rust sources that should trigger a host rebuild."
  [^java.io.File host-dir]
  (let [named (->> [(io/file host-dir "Cargo.toml")
                    (io/file host-dir "Cargo.lock")
                    (io/file host-dir "build.rs")]
                   (filterv #(.isFile ^java.io.File %)))
        src (io/file host-dir "src")
        rust (if (.isDirectory src)
               (->> (file-seq src)
                    (filter (fn [^java.io.File f]
                              (and (.isFile f)
                                   (.endsWith ^String (.getName f) ".rs"))))
                    vec)
               [])
        themes-dir (io/file host-dir "themes")
        themes (if (.isDirectory themes-dir)
                 (->> (file-seq themes-dir)
                      (filter (fn [^java.io.File f]
                                (and (.isFile f)
                                     (.endsWith ^String (.getName f) ".json"))))
                      vec)
                 [])]
    (into named (concat rust themes))))

(defn host-stale?
  "True when `binary` is older than a host source file under `host-dir`."
  [^java.io.File host-dir ^java.io.File binary]
  (let [t (.lastModified binary)]
    (boolean (some (fn [^java.io.File f]
                     (> (.lastModified f) t))
                   (host-input-files host-dir)))))

(defn host-crate
  "The `host/` crate directory under a library checkout, or nil."
  ^java.io.File []
  (when-let [root (library-root)]
    (let [dir (io/file root "host")]
      (when (.exists (io/file dir "Cargo.toml"))
        dir))))

(defn- gcc-libstdc-dir
  "Directory containing `libstdc++.so` as reported by g++.

  rust-lld does not search GCC's private lib dir, so Ubuntu hosts that only
  ship `libstdc++.so.6` in `/usr/lib` fail to link gpui without this."
  []
  (try
    (let [args (doto (ArrayList.)
                 (.add "g++")
                 (.add "-print-file-name=libstdc++.so"))
          pb (doto (ProcessBuilder. args)
               (.redirectError ProcessBuilder$Redirect/DISCARD))
          ^Process proc (.start pb)
          out (str/trim (slurp (.getInputStream proc)))]
      (when (and (zero? (.waitFor proc)) (seq out) (not= out "libstdc++.so"))
        (let [f (io/file out)]
          (when (.exists f)
            (.getParentFile f)))))
    (catch Exception _
      nil)))

(defn build-host!
  "Run `cargo build --release` in the library `host/` crate."
  ([]
   (let [root (or (library-root)
                  (throw (ex-info "Could not locate clj-gpui. Set CLJ_GPUI_ROOT." {})))]
     (build-host! root)))
  ([^java.io.File root]
   (let [host-dir (io/file root "host")]
     (when-not (.exists (io/file host-dir "Cargo.toml"))
       (throw (ex-info (str "No host/ crate under " (.getPath root)
                            ". Set CLJ_GPUI_BIN to a clj-gpui binary, or CLJ_GPUI_ROOT to the clj-gpui checkout.")
                       {:root (.getPath root)})))
     (println "[clj-gpui] building native host with cargo")
     (let [args (doto (ArrayList.)
                  (.add "cargo")
                  (.add "build")
                  (.add "--release"))
           pb (doto (ProcessBuilder. args)
                (.directory host-dir)
                (.inheritIO))
           env-map (.environment pb)]
       (when-let [^File gcc-lib (gcc-libstdc-dir)]
         (let [cur (or (.get env-map "LIBRARY_PATH") "")]
           (.put env-map "LIBRARY_PATH"
                 (if (seq cur)
                   (str (.getPath gcc-lib) File/pathSeparator cur)
                   (.getPath gcc-lib)))))
       (let [^Process proc (.start pb)
             code (.waitFor proc)]
         (when-not (zero? code)
           (throw (ex-info (str "cargo build --release failed with exit " code)
                           {:dir (.getPath host-dir) :code code})))
         host-dir)))))

(defn ensure-dev-host
  "Development host: `CLJ_GPUI_BIN`, or Cargo-build `host/` when missing/stale."
  ^java.io.File []
  (if-let [explicit (env "CLJ_GPUI_BIN")]
    (let [f (io/file explicit)]
      (when-not (.canExecute f)
        (throw (ex-info (str "CLJ_GPUI_BIN is not executable: " explicit) {})))
      f)
    (let [^java.io.File root (or (library-root)
                                 (throw (ex-info "Could not locate clj-gpui. Set CLJ_GPUI_ROOT or CLJ_GPUI_BIN." {})))
          host-dir (io/file root "host")
          target-dir (or (cargo-target-dir host-dir) (io/file host-dir "target"))
          prefs {:configured-target (cargo-build-target host-dir)
                 :host-triple (rustc-host-triple)}
          bin (locate-host-binary target-dir prefs)]
      (if (and bin (not (host-stale? host-dir bin)))
        bin
        (do
          (println (if bin
                     "[clj-gpui] host sources changed, rebuilding native host"
                     "[clj-gpui] native host missing, building with cargo"))
          (build-host! root)
          (let [target-dir (or (cargo-target-dir host-dir) target-dir)]
            (or (locate-host-binary target-dir prefs)
                (throw (ex-info (str "Host build succeeded but clj-gpui binary was not found under "
                                     (.getPath target-dir)
                                     ". Cargo may have written a different name; set CLJ_GPUI_BIN.")
                                {:target-dir (.getPath target-dir)
                                 :candidates (mapv #(.getPath ^java.io.File %)
                                                   (host-binary-candidates target-dir prefs))})))))))))

(defn- executable-file
  ^java.io.File [x]
  (when x
    (let [f (io/file (str x))]
      (when (and (.isFile f) (.canExecute f))
        f))))

(defn- sibling-host
  [^java.io.File dir]
  (when (and dir (.isDirectory dir))
    (some (fn [name]
            (executable-file (io/file dir name)))
          host-bin-names)))

(defn locate-prod-host
  "Production host: never invokes Cargo.

  Lookup order:
  1. `CLJ_GPUI_BIN`
  2. `CLJ_GPUI_APP_HOME` (package layout)
  3. Directory of the running JAR / `user.dir`"
  ^java.io.File []
  (or (executable-file (env "CLJ_GPUI_BIN"))
      (sibling-host (some-> (env "CLJ_GPUI_APP_HOME") io/file))
      (sibling-host (some-> (env "CLJ_GPUI_APP_HOME") io/file (io/file "bin")))
      (sibling-host (io/file (System/getProperty "user.dir")))
      (when-let [home (System/getProperty "java.home")]
        (or (sibling-host (io/file home ".." "bin"))
            (sibling-host (io/file home "bin"))))))

(defn require-prod-host
  ^java.io.File []
  (or (locate-prod-host)
      (throw (ex-info
              (str "clj-gpui host binary not found. A packaged app sets CLJ_GPUI_BIN. "
                   "For development use `clj -M:dev`, not gpui.prod.")
              {}))))

(defn spawn-host!
  "Start the native host, pointing it at the Clojure TCP listener."
  [^java.io.File exe port protocol-test?]
  (let [cmd (doto (ArrayList.)
              (.add (.getCanonicalPath exe))
              (cond-> protocol-test? (.add "--protocol-test")))
        pb (doto (ProcessBuilder. cmd)
             (.inheritIO))
        env-map (.environment pb)]
    (.put env-map "CLJ_GPUI_PORT" (str port))
    (.put env-map "CLJ_GPUI_HOST" "127.0.0.1")
    (when-let [icd (env "VK_ICD_FILENAMES")]
      (.put env-map "VK_ICD_FILENAMES" icd))
    (println (str "[clj-gpui] starting host " (.getPath exe)))
    (.start pb)))

(defn run-bridge!
  "Bind a local TCP port, spawn the host, and serve the UI protocol.

  Blocks until the host disconnects. Exits the JVM when the window closes
  (or when the protocol-test host process exits)."
  [{:keys [^java.io.File exe protocol-test?]}]
  (let [server (doto (ServerSocket. 0)
                 (.setReuseAddress true)
                 (.setSoTimeout 60000))
        port (.getLocalPort server)]
    (println (str "[clj-gpui] waiting for host on 127.0.0.1:" port))
    (let [^Process proc (spawn-host! exe port protocol-test?)
          sock (.accept server)
          in (BufferedReader.
              (InputStreamReader. (.getInputStream sock) StandardCharsets/UTF_8))
          out (OutputStreamWriter. (.getOutputStream sock) StandardCharsets/UTF_8)]
      (.setSoTimeout server 0)
      (runtime/bind-connection! {:socket sock :in in :out out})
      (runtime/send-ready!)
      (if protocol-test?
        (let [reader (future (runtime/read-loop in))
              code (.waitFor proc)]
          (future-cancel reader)
          (println (str "[clj-gpui] protocol test host exit " code))
          (System/exit (int code)))
        (try
          (runtime/read-loop in)
          (println "[clj-gpui] host disconnected")
          (finally
            (.destroy proc)
            (when (.isAlive proc)
              (.destroyForcibly proc)
              (.waitFor proc))
            (System/exit 0)))))))
