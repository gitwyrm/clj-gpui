(ns gpui.ratom
  "Reagent-style reactive atoms for ClojureGPUI.

  Require this namespace as `r` and write `(r/atom ...)` instead of
  wrapping a normal atom. The result is a real `clojure.core/Atom`:
  `swap!`, `reset!`, `deref`, and `@` are Clojure's, not a parallel
  state system. The only extra behavior is a watch that asks GPUI to
  rerender the window."
  (:refer-clojure :exclude [atom])
  (:require [gpui.core :as ui]))

(def atom
  "Like `clojure.core/atom`, but GPUI rerenders when the value changes.

  (require '[gpui.ratom :as r])
  (defonce count (r/atom 0))
  (swap! count inc)
  @count"
  ui/ratom)

(defn atom?
  "True when `x` is a Clojure atom watched for GPUI rerenders."
  [x]
  (boolean
   (and (instance? clojure.lang.IAtom x)
        (contains? (.getWatches ^clojure.lang.IRef x) :gpui.core/gpui-render))))
