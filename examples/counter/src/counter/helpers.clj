(ns counter.helpers
  "Ordinary Clojure helpers required by the example app.
  Exists to prove that real namespaces and require work across the bridge.")

(defn incomplete [items]
  (vec (remove :done items)))

(defn bullet
  [s]
  (str "• " s))
