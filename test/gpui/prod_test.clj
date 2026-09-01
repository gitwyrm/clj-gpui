(ns gpui.prod-test
  (:require [clojure.test :refer [deftest is]]
            [gpui.host :as host]
            [gpui.prod :as prod]))

(deftest prod-does-not-auto-build
  (is (nil? (host/locate-prod-host))
      "without CLJ_GPUI_BIN, production lookup must not invent a cargo target"))

(deftest prod-has-main
  (is (ifn? (ns-resolve 'gpui.prod '-main))))
