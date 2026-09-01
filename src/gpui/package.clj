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
            [gpui.host :as host]
            [gpui.package-launch :as launch])
  (:import [java.lang ProcessBuilder$Redirect]
           [java.util ArrayList]))

(set! *warn-on-reflection* true)

(def launcher-script launch/launcher-script)

(load "package_build")
(load "package_native")
