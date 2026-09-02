(ns gpui.core-test
  "Compatibility: gpui.core re-exports gpui.ui."
  (:require [clojure.test :refer [deftest is]]
            [gpui.core :as core]
            [gpui.ui :as ui]))

(deftest core-aliases-ui
  (is (identical? ui/label core/label))
  (is (identical? ui/button core/button))
  (is (identical? ui/text-field core/text-field))
  (is (identical? ui/window core/window))
  (is (= ui/window-title core/window-title))
  (is (= ui/protocol-version core/protocol-version))
  (is (= ui/named-themes core/named-themes))
  (is (identical? ui/list core/ui-list)))
