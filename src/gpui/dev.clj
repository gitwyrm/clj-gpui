(ns gpui.dev
  "Clojure-first entry point for clj-gpui.

  Starts nREPL, watches app sources, listens for the native host, then
  launches the GPUI binary. Application authors run:

    clj -M:dev my.app/app

  The host is built with Cargo when `host/` is present and either the
  binary is missing or a host source file is newer than the binary."
  (:require [clojure.data.json :as json]
            [clojure.java.io :as io]
            [clojure.string :as str]
            [gpui.runtime :as runtime])
  (:import [java.io BufferedReader InputStreamReader OutputStreamWriter]
           [java.lang ProcessBuilder$Redirect]
           [java.net ServerSocket]
           [java.nio.charset StandardCharsets]
           [java.util ArrayList]))

(set! *warn-on-reflection* true)

(def ^:private host-bin-names
  ["clj-gpui" "clj-gpui.exe"])

(defn- env
  ([k] (env k nil))
  ([k default]
   (or (System/getenv k) default)))

(defn- library-root
  "Directory that contains this library's deps.edn and host/ crate."
  ^java.io.File []
  (if-let [explicit (env "CLJ_GPUI_ROOT")]
    (io/file explicit)
    (when-let [res (io/resource "gpui/dev.clj")]
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

(defn- rustc-host-triple
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
               [])]
    (into named rust)))

(defn host-stale?
  "True when `binary` is older than a host source file under `host-dir`."
  [^java.io.File host-dir ^java.io.File binary]
  (let [t (.lastModified binary)]
    (boolean (some (fn [^java.io.File f]
                     (> (.lastModified f) t))
                   (host-input-files host-dir)))))

(defn- build-host!
  [^java.io.File root]
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
          ^Process proc (.start pb)
          code (.waitFor proc)]
      (when-not (zero? code)
        (throw (ex-info (str "cargo build --release failed with exit " code)
                        {:dir (.getPath host-dir) :code code}))))))

(defn- ensure-host
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

(defn- spawn-host!
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

(defn- parse-args
  [args]
  (let [protocol-test? (boolean (some #{"--protocol-test"} args))
        rest (vec (remove #{"--protocol-test"} args))
        app (or (first rest) (env "CLJ_GPUI_APP"))]
    {:protocol-test? protocol-test?
     :app app}))

(defn -main
  [& args]
  (let [{:keys [protocol-test? app]} (parse-args args)]
    (when-not app
      (binding [*out* *err*]
        (println "Usage: clojure -M -m gpui.dev [ --protocol-test ] my.app/app")
        (println "Example: cd examples/counter && clojure -M:dev"))
      (System/exit 2))
    (runtime/set-app-symbol! app)
    (runtime/install-render-hook!)
    (try
      (runtime/load-app!)
      (catch Exception e
        (binding [*out* *err*]
          (println "[clj-gpui] failed to load" app ":" (.getMessage e)))
        (when protocol-test?
          (.printStackTrace e)
          (System/exit 1))))
    (when-not protocol-test?
      (runtime/start-nrepl!)
      (runtime/start-watcher!)
      (println (str "[clj-gpui] nREPL 127.0.0.1:" (runtime/nrepl-port)))
      (println (str "[clj-gpui] hot reload watching " (env "CLJ_GPUI_SRC" "src")))
      (println "[clj-gpui] root UI var" (runtime/app-symbol)))
    (let [exe (ensure-host)
          server (doto (ServerSocket. 0)
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
              (System/exit 0))))))))
