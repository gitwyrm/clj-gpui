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
                        #"ThemeSet :name"
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
  (let [ex (try
             (theme/theme-set {:name "X" :themes []})
             (catch clojure.lang.ExceptionInfo e e))]
    (is (= :themes (:gpui.theme/invalid (ex-data ex))))))

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

(deftest protocol-version-is-two
  (is (= 2 ui/protocol-version))
  (is (= 2 runtime/protocol-version)))
