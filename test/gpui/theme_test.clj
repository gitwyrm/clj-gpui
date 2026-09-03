(ns gpui.theme-test
  (:require [catppuccin-violet.theme :as palette]
            [clojure.data.json :as json]
            [clojure.java.io :as io]
            [clojure.test :refer [deftest is]]
            [gpui.runtime :as runtime]
            [gpui.theme :as theme]
            [gpui.ui :as ui]))

(deftest theme-set-requires-name-and-variants
  (is (thrown-with-msg? clojure.lang.ExceptionInfo
                        #":name is required"
                        (theme/theme-set {:themes [{:name "X" :mode :dark :colors {}}]})))
  (is (thrown-with-msg? clojure.lang.ExceptionInfo
                        #":themes must contain"
                        (theme/theme-set {:name "X" :themes []})))
  (is (thrown-with-msg? clojure.lang.ExceptionInfo
                        #":mode must be"
                        (theme/theme-set {:name "X"
                                          :themes [{:name "X Dark" :mode :dim :colors {}}]})))
  (is (thrown-with-msg? clojure.lang.ExceptionInfo
                        #"hex string"
                        (theme/theme-set {:name "X"
                                          :themes [{:name "X Dark"
                                                    :mode :dark
                                                    :colors {:background "red"}}]})))
  (is (thrown-with-msg? clojure.lang.ExceptionInfo
                        #"cannot be :system"
                        (theme/theme-set {:name "Light"
                                          :themes [{:name "X Dark" :mode :dark :colors {}}]})))
  (is (thrown-with-msg? clojure.lang.ExceptionInfo
                        #"cannot be :system"
                        (theme/theme-set {:name "Mine"
                                          :themes [{:name "dark" :mode :dark :colors {}}]})))
  (let [ex (try
             (theme/theme-set {:name "X" :themes []})
             (catch clojure.lang.ExceptionInfo e e))]
    (is (= :themes (:gpui.theme/invalid (ex-data ex))))))

(deftest name-normalization-matches-host
  (doseq [n ["My Theme" "my-theme" "my_theme" "  My   Theme  " :my-theme]]
    (is (= "my theme" (theme/normalize-name n)) n))
  (is (= "catppuccin violet" (theme/normalize-name "Catppuccin Violet")))
  (is (= "catppuccin violet" (theme/normalize-name :catppuccin-violet)))
  (is (= "tokyo night" (theme/normalize-name "Tokyo_Night")))
  (is (= "system" (theme/normalize-name :system)))
  (is (= "light" (theme/normalize-name "Light"))))

(deftest equivalent-names-share-a-registry-slot
  (theme/clear!)
  (try
    (theme/register! {:name "My Theme"
                      :themes [{:name "My Theme Dark"
                                :mode :dark
                                :colors {:background "#aa0000"}}]})
    (theme/register! {:name "my-theme"
                      :themes [{:name "My Theme Dark"
                                :mode :dark
                                :colors {:background "#00aa00"}}]})
    (is (= 1 (count (theme/registered))))
    (is (= "my-theme" (:name (first (theme/registered))))
        "replacement keeps the slot and uses the new display name")
    (is (= "#00aa00"
           (get-in (theme/registered) [0 :themes 0 :colors "background"])))
    (is (= "my-theme" (:name (theme/unregister! :my_theme))))
    (is (empty? (theme/registered)))
    (finally
      (theme/clear!))))

(deftest json-str-resolves-normalized-names
  (theme/clear!)
  (try
    (theme/register! palette/catppuccin-violet)
    (doseq [n ["Catppuccin Violet" "catppuccin-violet" :catppuccin_violet
               "  Catppuccin   Violet  "]]
      (let [parsed (json/read-str (theme/json-str n))]
        (is (= "Catppuccin Violet" (get parsed "name")) n)
        (is (= "#cba6f7"
               (get-in parsed ["themes" 1 "colors" "primary.background"]))
            n)))
    (finally
      (theme/clear!))))

(deftest extra-theme-config-fields-are-kept
  (let [s (theme/theme-set
           {:name "Syntaxy"
            :themes [{:name "Syntaxy Dark"
                      :mode :dark
                      :font-size 15
                      :highlight {:editor.foreground "#cdd6f4"
                                  :syntax {:comment {:color "#6c7086"}}}
                      :colors {:background "#1e1e2e"}}]})
        variant (first (:themes s))
        parsed (json/read-str (theme/json-str s))]
    (is (= 15 (:font.size variant)))
    (is (= "#cdd6f4" (get-in variant [:highlight :editor.foreground])))
    (is (= 15 (get-in parsed ["themes" 0 "font.size"])))
    (is (= "#cdd6f4"
           (get-in parsed ["themes" 0 "highlight" "editor.foreground"])))
    (is (= "#6c7086"
           (get-in parsed ["themes" 0 "highlight" "syntax" "comment" "color"])))))

(deftest catppuccin-registers-on-theme-namespace-load
  (theme/clear!)
  (try
    (require 'catppuccin-violet.theme :reload)
    (is (= ["Catppuccin Violet"] (mapv :name (theme/registered))))
    (is (= "Catppuccin Violet"
           (get (json/read-str (theme/json-str :catppuccin-violet)) "name")))
    (finally
      (theme/clear!))))

(deftest app-render-does-not-register
  (require 'catppuccin-violet.app)
  (theme/clear!)
  (try
    (let [tree ((requiring-resolve 'catppuccin-violet.app/app))]
      (is (= :window (:type tree)))
      (is (empty? (theme/registered))
          "app [] must not register; catppuccin-violet.theme does at load"))
    (finally
      (theme/clear!))))

(deftest register-order-is-wire-order
  (theme/clear!)
  (try
    (theme/register! {:name "Later"
                      :themes [{:name "Dup"
                                :mode :dark
                                :colors {:background "#00ff00"}}]})
    (theme/register! {:name "Override"
                      :themes [{:name "Dup"
                                :mode :dark
                                :colors {:background "#ff00ff"}}]})
    (let [wire (theme/wire-sets)]
      (is (= ["Later" "Override"] (mapv :name wire)))
      (is (= "#00ff00"
             (get-in wire [0 :themes 0 :colors "background"]))))
    (theme/register! {:name "Later"
                      :themes [{:name "Dup"
                                :mode :dark
                                :colors {:background "#0000aa"}}]})
    (is (= ["Later" "Override"] (mapv :name (theme/wire-sets)))
        "re-register keeps the original slot")
    (is (= "#0000aa"
           (get-in (theme/wire-sets) [0 :themes 0 :colors "background"])))
    (finally
      (theme/clear!))))

(deftest catppuccin-violet-matches-utility-belt
  (let [set palette/catppuccin-violet
        light palette/light
        dark palette/dark]
    (is (= "Catppuccin Violet" (:name set)))
    (is (= "Catppuccin Violet Light" (:name light)))
    (is (= "light" (:mode light)))
    (is (= "Catppuccin Violet Dark" (:name dark)))
    (is (= "dark" (:mode dark)))
    (is (= "#eff1f5" (get (:colors light) "background")))
    (is (= "#4c4f69" (get (:colors light) "foreground")))
    (is (= "#7c3aed" (get (:colors light) "primary.background")))
    (is (= "#7c3aed" (get (:colors light) "ring")))
    (is (= "#7c3aed" (get (:colors light) "caret")))
    (is (= "#1e1e2e" (get (:colors dark) "background")))
    (is (= "#cdd6f4" (get (:colors dark) "foreground")))
    (is (= "#cba6f7" (get (:colors dark) "primary.background")))
    (is (= "#cba6f7" (get (:colors dark) "ring")))
    (is (= "#cba6f7" (get (:colors dark) "caret")))
    (is (= "#181825" (get (:colors dark) "sidebar.background")))
    (is (not (some #{"Catppuccin Violet" "Catppuccin Violet Dark"} ui/named-themes)))
    (is (= ["system" "light" "dark"] (take 3 ui/themes)))))

(deftest catppuccin-json-roundtrip
  (let [parsed (json/read-str (theme/json-str palette/catppuccin-violet))]
    (is (= "Catppuccin Violet" (get parsed "name")))
    (is (= "Catppuccin Violet Light" (get-in parsed ["themes" 0 "name"])))
    (is (= "light" (get-in parsed ["themes" 0 "mode"])))
    (is (= "Catppuccin Violet Dark" (get-in parsed ["themes" 1 "name"])))
    (is (= "dark" (get-in parsed ["themes" 1 "mode"])))
    (is (= "#cba6f7" (get-in parsed ["themes" 1 "colors" "primary.background"])))
    (is (= "#eff1f5" (get-in parsed ["themes" 0 "colors" "background"]))))
  (let [file (io/file "examples/themes/catppuccin-violet/themes/catppuccin-violet.json")]
    (is (.isFile file) "committed JSON example")
    (let [from-disk (json/read-str (slurp file))]
      (is (= (json/read-str (theme/json-str palette/catppuccin-violet))
             from-disk)
          "JSON file matches Clojure ThemeSet"))))

(deftest custom-theme-is-selected-by-theme-string
  (theme/clear!)
  (try
    (theme/register! palette/catppuccin-violet)
    (is (= ["Catppuccin Violet"
            "Catppuccin Violet Light"
            "Catppuccin Violet Dark"]
           (theme/available-names)))
    (runtime/reset-callbacks!)
    (let [exported (runtime/export-tree
                    (ui/window {:theme "Catppuccin Violet Dark"}
                               (ui/label "x")))]
      (is (= "Catppuccin Violet Dark" (:theme exported))))
    (let [exported (runtime/export-tree
                    (ui/window {:theme :catppuccin-violet}
                               (ui/label "x")))]
      (is (= "catppuccin-violet" (:theme exported))))
    (is (= 1 (count (theme/wire-sets))))
    (is (= "Catppuccin Violet" (:name (first (theme/wire-sets)))))
    (finally
      (theme/clear!))))

(deftest protocol-version-is-nine
  (is (= 9 ui/protocol-version))
  (is (= 9 runtime/protocol-version)))
