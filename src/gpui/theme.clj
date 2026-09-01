(ns gpui.theme
  "Register gpui-component ThemeSets from ordinary Clojure data.

  Required on each variant: `:name`, `:mode` (`:light` / `:dark`), and
  `:colors` (gpui-component tokens such as `:primary.background` → hex).
  Other ThemeConfig keys (`:highlight`, `:font.family`, `:font.size`,
  `:radius`, `:shadow`, `:is_default`, …) are kept and serialized with
  the same JSON field names gpui-component uses on disk.

  `(register!)` keeps sets in this process. The runtime sends them on each
  render; UI nodes still refer to a palette by `:theme` name (a string).

  Names are identified the same way as the host: trim, lowercase, treat
  `-` / `_` as spaces, collapse whitespace. `(ui/themes)` remains the
  palettes *shipped* with clj-gpui. Use `registered` / `available-names`
  for sets registered here."
  (:require [clojure.data.json :as json]
            [clojure.string :as str]
            [clojure.java.io :as io]))

;; Insertion order is the wire order. First ThemeSet wins on a duplicate
;; variant name once the host installs the array.
(defonce ^:private registry*
  (atom []))

(defn- as-str
  [x]
  (cond
    (keyword? x) (name x)
    (string? x) x
    (symbol? x) (name x)
    (nil? x) nil
    :else (str x)))

(defn normalize-name
  "Identity key for a theme name. Matches host `catalog::normalize`:
  trim, lowercase, `-`/`_` → space, collapse whitespace.

  Does not change the display `:name` stored on a ThemeSet."
  [name]
  (let [s (-> (or (as-str name) "")
              str/trim
              str/lower-case
              (str/replace #"[_-]" " ")
              str/trim)]
    (->> (str/split s #"\s+")
         (remove str/blank?)
         (str/join " "))))

(defn- name-key
  [theme-set-or-name]
  (normalize-name (if (map? theme-set-or-name)
                    (:name theme-set-or-name)
                    theme-set-or-name)))

(defn- invalid!
  [k value hint]
  (throw (ex-info hint {:gpui.theme/invalid k :value value})))

(defn- as-mode
  [x]
  (let [s (some-> x as-str str/lower-case)]
    (case s
      "light" "light"
      "dark" "dark"
      (invalid! :mode x "theme :mode must be :light or :dark"))))

(defn- as-hex
  [x]
  (let [s (as-str x)]
    (when-not (and (string? s) (re-matches #"#?[0-9a-fA-F]{3,8}" s))
      (invalid! :color x (str "theme color must be a hex string, got " (pr-str x))))
    (if (str/starts-with? s "#") s (str "#" s))))

(defn- assert-usable-name
  [n origin]
  (when (str/blank? n)
    (invalid! :name origin "theme :name is required"))
  (when (#{"system" "light" "dark"} (normalize-name n))
    (invalid! :name origin "theme :name cannot be :system, :light, or :dark")))

(def ^:private config-field-aliases
  "Kebab aliases → gpui-component ThemeConfig JSON field names."
  {"font-size" "font.size"
   "font-family" "font.family"
   "mono-font-family" "mono_font.family"
   "mono-font-size" "mono_font.size"
   "radius-lg" "radius.lg"
   "is-default" "is_default"})

(defn- config-field-key
  [k]
  (let [s (as-str k)]
    (get config-field-aliases s s)))

(defn- normalize-colors
  [colors]
  (when-not (or (nil? colors) (map? colors))
    (invalid! :colors colors "theme :colors must be a map of token → hex"))
  (into (sorted-map)
        (for [[k v] colors]
          [(as-str k) (as-hex v)])))

(defn- extra-config
  [theme]
  (into {}
        (for [[k v] theme
              :let [jk (config-field-key k)]
              :when (and (some? v) (not (#{"name" "mode" "colors"} jk)))]
          [(keyword jk) (if (= jk "is_default") (boolean v) v)])))

(defn- normalize-theme
  [theme]
  (when-not (map? theme)
    (invalid! :theme theme "each ThemeSet :themes entry must be a map"))
  (let [n (as-str (:name theme))
        mode (as-mode (:mode theme))]
    (assert-usable-name n (:name theme))
    (merge (extra-config theme)
           {:name n
            :mode mode
            :colors (normalize-colors (or (:colors theme) {}))})))

(defn theme-set
  "Return a gpui-component ThemeSet map. Throws `ex-info` with
  `:gpui.theme/invalid` when required fields are missing.

  Validates `:name`, `:mode`, and color hex values. Other ThemeConfig
  fields are preserved under gpui-component's JSON names (`:font.size`,
  `:highlight`, …)."
  [m]
  (when-not (map? m)
    (invalid! :theme-set m "theme-set must be a map"))
  (let [n (as-str (:name m))
        themes (mapv normalize-theme (:themes m))]
    (assert-usable-name n (:name m))
    (when (empty? themes)
      (invalid! :themes (:themes m) "ThemeSet :themes must contain at least one palette"))
    (cond-> {:name n
             :themes themes}
      (some? (:author m)) (assoc :author (as-str (:author m)))
      (some? (:url m)) (assoc :url (as-str (:url m))))))

(defn register!
  "Remember a ThemeSet for the native host. Replaces a previous set whose
  `:name` normalizes to the same key (same slot). Does not request a
  render; the next export includes it.

  Call this from a theme namespace so `:reload` of that namespace picks
  up palette edits. Do not call it from `app` on every render."
  [m]
  (let [s (theme-set m)
        k (name-key s)]
    (swap! registry*
           (fn [xs]
             (if (some #(= k (name-key %)) xs)
               (mapv #(if (= k (name-key %)) s %) xs)
               (conj xs s))))
    s))

(defn unregister!
  "Drop a previously registered ThemeSet by its `:name` (any spelling
  the host would treat as the same name)."
  [name]
  (let [k (name-key name)
        dropped (some #(when (= k (name-key %)) %) @registry*)]
    (swap! registry* (fn [xs] (filterv #(not= k (name-key %)) xs)))
    dropped))

(defn clear!
  "Drop every ThemeSet registered in this process. Intended for tests."
  []
  (reset! registry* [])
  nil)

(defn registered
  "ThemeSets registered in this process, in registration order.
  Not bundled palettes and not JSON files from `./themes`."
  []
  @registry*)

(defn wire-sets
  "ThemeSets as they go on a render response (`:themes`). Always a vector."
  []
  (registered))

(defn available-names
  "Variant names (`Catppuccin Violet Dark`) plus ThemeSet family names.

  Does not include JSON files the host loads from `./themes` / `CLJ_GPUI_THEMES`."
  []
  (->> (registered)
       (mapcat (fn [s]
                 (cons (:name s) (map :name (:themes s)))))
       distinct
       vec))

(defn- resolve-set
  [theme-set-or-name]
  (if (map? theme-set-or-name)
    (theme-set theme-set-or-name)
    (or (some #(when (= (name-key theme-set-or-name)
                        (name-key %))
                 %)
              @registry*)
        (invalid! :name theme-set-or-name "unknown registered ThemeSet"))))

(defn json-str
  "gpui-component theme-file JSON for a ThemeSet (or a registered name)."
  [theme-set-or-name]
  (json/write-str (resolve-set theme-set-or-name) :escape-slash false))

(defn write-json
  "Write a ThemeSet as gpui-component JSON to `path`."
  [theme-set-or-name path]
  (let [file (io/file path)
        body (with-out-str
               (json/pprint (resolve-set theme-set-or-name)
                            :escape-slash false))]
    (when-let [parent (.getParentFile file)]
      (.mkdirs parent))
    (spit file body)
    file))
