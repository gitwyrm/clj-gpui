(ns gpui.core
  "Compatibility namespace. Prefer `[gpui.ui :as ui]` in new code."
  (:require [gpui.ui :as ui]))

(doseq [[sym v] (ns-publics 'gpui.ui)]
  ;; `ui/list` would replace `clojure.core/list` in this ns.
  (when-not (= sym 'list)
    (intern *ns* (with-meta sym (merge (meta v) {:doc (or (:doc (meta v)) "")})) @v)))

(def ui-list
  "See `gpui.ui/list`. Not interned as `list` so `clojure.core/list` stays intact."
  ui/list)
