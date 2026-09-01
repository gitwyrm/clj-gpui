(ns gpui.theme
  "Register gpui-component ThemeSets from ordinary Clojure data.

  A ThemeSet is the same JSON object gpui-component uses on disk: `:name`,
  optional `:author` / `:url`, and `:themes` (each with `:name`, `:mode`,
  and `:colors`). Color keys are gpui-component tokens such as
  `:primary.background` and `:sidebar.foreground`.

  `(register!)` keeps sets in this process. The runtime sends them on each
  render; UI nodes still refer to a palette by `:theme` name (a string).

  `(ui/themes)` remains the palettes *shipped* with clj-gpui. Use
  `registered` / `available-names` for sets registered here."
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

(defn- normalize-colors
  [colors]
  (when-not (or (nil? colors) (map? colors))
    (invalid! :colors colors "theme :colors must be a map of token → hex"))
  (into (sorted-map)
        (for [[k v] colors]
          [(as-str k) (as-hex v)])))

(defn- normalize-theme
  [theme]
  (when-not (map? theme)
    (invalid! :theme theme "each ThemeSet :themes entry must be a map"))
  (let [n (as-str (:name theme))
        mode (as-mode (:mode theme))]
    (when (str/blank? n)
      (invalid! :name (:name theme) "theme :name is required"))
    (cond-> {:name n
             :mode mode
             :colors (normalize-colors (or (:colors theme) {}))}
      (contains? theme :is_default) (assoc :is_default (boolean (:is_default theme)))
      (some? (:font-size theme)) (assoc :font.size (:font-size theme))
      (some? (:font.size theme)) (assoc :font.size (:font.size theme)))))

(defn theme-set
  "Return a gpui-component ThemeSet map. Throws `ex-info` with
  `:gpui.theme/invalid` when required fields are missing."
  [m]
  (when-not (map? m)
    (invalid! :theme-set m "theme-set must be a map"))
  (let [n (as-str (:name m))
        themes (mapv normalize-theme (:themes m))]
    (when (str/blank? n)
      (invalid! :name (:name m) "ThemeSet :name is required"))
    (when (empty? themes)
      (invalid! :themes (:themes m) "ThemeSet :themes must contain at least one palette"))
    (cond-> {:name n
             :themes themes}
      (some? (:author m)) (assoc :author (as-str (:author m)))
      (some? (:url m)) (assoc :url (as-str (:url m))))))

(defn- set-key
  [theme-set]
  (str/lower-case (as-str (:name theme-set))))

(defn register!
  "Remember a ThemeSet for the native host. Replaces a previous set with
  the same `:name` (same slot). Does not request a render; the next
  export includes it.

  Call this from application code (often at the top of a theme namespace
  so `:reload` picks up edits)."
  [m]
  (let [s (theme-set m)
        k (set-key s)]
    (swap! registry*
           (fn [xs]
             (if (some #(= k (set-key %)) xs)
               (mapv #(if (= k (set-key %)) s %) xs)
               (conj xs s))))
    s))

(defn unregister!
  "Drop a previously registered ThemeSet by its `:name`."
  [name]
  (let [k (str/lower-case (as-str name))
        dropped (some #(when (= k (set-key %)) %) @registry*)]
    (swap! registry* (fn [xs] (filterv #(not= k (set-key %)) xs)))
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
    (or (some #(when (= (str/lower-case (as-str theme-set-or-name))
                        (set-key %))
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
