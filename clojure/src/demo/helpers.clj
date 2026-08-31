(ns demo.helpers
  "Ordinary Clojure helpers required by the demo app.
  Exists to prove that real namespaces and require work across the bridge.")

(defn bullet
  [s]
  (str "• " s))

(defn lots?
  [n]
  (> n 5))
