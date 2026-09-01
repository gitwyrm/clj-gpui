(ns gpui.core
  "Compatibility namespace. Prefer `[gpui.ui :as ui]` in new code."
  (:require [gpui.ui :as ui]))

(doseq [[sym v] (ns-publics 'gpui.ui)]
  (intern *ns* (with-meta sym (merge (meta v) {:doc (or (:doc (meta v)) "")})) @v))
