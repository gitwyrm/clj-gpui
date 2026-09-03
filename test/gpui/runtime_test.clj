(ns gpui.runtime-test
  (:require [clojure.java.io :as io]
            [clojure.string :as str]
            [clojure.test :refer [deftest is testing]]
            [gpui.ratom :as r]
            [gpui.runtime :as runtime]
            [gpui.ui :as ui]))

(def ^:private widgets-file
  (io/file "test/gpui/reload_probe/widgets.clj"))

(def ^:private widgets-original
  (slurp widgets-file))

(defn- probe-tree-text [tree]
  (->> (tree-seq :children :children tree)
       (keep :text)
       (str/join "\n")))

(defn- restore-probe! []
  (spit widgets-file widgets-original)
  (reset! @#'runtime/load-error* nil)
  (try
    (require 'gpui.reload-probe.widgets :reload)
    (require 'gpui.reload-probe.app :reload)
    (catch Exception _)))

(defn- request-render-on-wire?
  [buf]
  (str/includes? (str buf) "request-render"))

(defn- wait-for-request-render
  "schedule-render! debounce is 16ms on a future; GHA macOS can start that
  later than a fixed Thread/sleep."
  [buf timeout-ms]
  (let [deadline (+ (System/currentTimeMillis) timeout-ms)]
    (loop []
      (cond
        (request-render-on-wire? buf) true
        (>= (System/currentTimeMillis) deadline) false
        :else (do (Thread/sleep 5) (recur))))))

(deftest require-reload-on-root-does-not-reload-deps
  (restore-probe!)
  (require 'gpui.reload-probe.app :reload)
  (is (= "probe-old" ((requiring-resolve 'gpui.reload-probe.widgets/banner))))
  (try
    (spit widgets-file "(ns gpui.reload-probe.widgets)\n(defn banner [] \"probe-new\")\n")
    (require 'gpui.reload-probe.app :reload)
    (is (= "probe-old" ((requiring-resolve 'gpui.reload-probe.widgets/banner)))
        "(require app :reload) must not reload already-loaded helper namespaces")
    (finally
      (restore-probe!))))

(deftest reload-app-picks-up-helper-namespace
  (restore-probe!)
  (runtime/set-app-symbol! 'gpui.reload-probe.app/app)
  (try
    (runtime/load-app!)
    (is (str/includes? (probe-tree-text (runtime/export-tree)) "probe-old-0"))
    (spit widgets-file "(ns gpui.reload-probe.widgets)\n(defn banner [] \"probe-new\")\n")
    (runtime/reload-app! [widgets-file])
    (is (str/includes? (probe-tree-text (runtime/export-tree)) "probe-new-0"))
    (swap! @(requiring-resolve 'gpui.reload-probe.app/!state) assoc :n 7)
    (runtime/reload-app! [widgets-file])
    (is (str/includes? (probe-tree-text (runtime/export-tree)) "probe-new-7")
        "defonce state survives helper + app reload")
    (finally
      (restore-probe!))))

(deftest reload-compile-error-shows-in-tree-until-fixed
  (restore-probe!)
  (runtime/set-app-symbol! 'gpui.reload-probe.app/app)
  (try
    (runtime/load-app!)
    (spit widgets-file "(ns gpui.reload-probe.widgets)\n(defn banner []\n")
    (is (thrown? Exception (runtime/reload-app! [widgets-file])))
    (let [text (probe-tree-text (runtime/export-tree))]
      (is (str/includes? text "Clojure error"))
      (is (re-find #"reload_probe/widgets" text)))
    (spit widgets-file "(ns gpui.reload-probe.widgets)\n(defn banner [] \"probe-fixed\")\n")
    (runtime/reload-app! [widgets-file])
    (let [text (probe-tree-text (runtime/export-tree))]
      (is (not (str/includes? text "Clojure error")))
      (is (str/includes? text "probe-fixed")))
    (finally
      (restore-probe!))))

(deftest error-tree-includes-compiler-location
  (let [tree (runtime/export-tree
              (fn []
                (throw (ex-info "Syntax error compiling at (my/widgets.clj:4:1)."
                                {:clojure.error/source "my/widgets.clj"
                                 :clojure.error/line 4
                                 :clojure.error/column 1}))))
        text (probe-tree-text tree)]
    (is (str/includes? text "Clojure error"))
    (is (str/includes? text "my/widgets.clj:4:1"))))

(deftest skip-reload-ns-covers-package-fragments
  (is (#'runtime/skip-reload-ns? 'gpui.package))
  (is (#'runtime/skip-reload-ns? 'gpui.package_build))
  (is (#'runtime/skip-reload-ns? 'gpui.package-native))
  (is (#'runtime/skip-reload-ns? 'gpui.package-launch))
  (is (not (#'runtime/skip-reload-ns? 'gpui.ui)))
  (is (not (#'runtime/skip-reload-ns? 'gpui.platform))))

(deftest ns-from-file-uses-path-and-ns-form
  (let [src (doto (io/file (System/getProperty "java.io.tmpdir")
                           (str "clj-gpui-ns-" (random-uuid)))
              (.mkdirs))
        f (io/file src "my" "widgets.clj")]
    (try
      (.mkdirs (.getParentFile f))
      (spit f "(ns my.widgets)\n(defn x [] 1)\n")
      (is (= 'my.widgets (runtime/ns-from-file src f)))
      (spit f "(defn broken")
      (is (= 'my.widgets (runtime/ns-from-file src f))
          "cached ns is used when the file no longer reads")
      (finally
        (.delete f)
        (.delete (.getParentFile f))
        (.delete src)))))

(deftest changed-clj-files-detects-new-and-updated
  (let [a "/tmp/a.clj"
        b "/tmp/b.clj"]
    (is (= #{a b} (set (map #(.getPath %)
                            (runtime/changed-clj-files {a 1} {a 2 b 1})))))
    (is (= #{a} (set (map #(.getPath %)
                          (runtime/changed-clj-files {a 1 b 1} {a 2 b 1})))))
    (is (empty? (runtime/changed-clj-files {a 1} {a 1})))))

(deftest atom-callback-does-not-enqueue-second-render
  (let [buf (java.io.StringWriter.)
        !state (r/atom {:n 0})]
    (runtime/bind-connection! {:out buf})
    (runtime/install-render-hook!)
    (try
      (testing "swap! outside a host callback still requests a render"
        (swap! !state update :n inc)
        (is (wait-for-request-render buf 2000)))
      (let [before (str buf)
            exported (runtime/export-tree
                      (fn []
                        (ui/button "+" #(swap! !state update :n inc))))
            id (:on-click exported)]
        (runtime/invoke-callback! id)
        (is (= 2 (:n @!state)))
        (Thread/sleep 40)
        (is (= before (str buf))
            "r/atom watch must not send request-render during callback; the host already renders"))
      (let [before (str buf)
            exported (runtime/export-tree
                      (fn []
                        (ui/button "noop" (fn [] :ok))))
            id (:on-click exported)]
        (runtime/invoke-callback! id)
        (Thread/sleep 40)
        (is (= before (str buf))))
      (finally
        (runtime/bind-connection! nil)
        (ui/set-request-render! nil)))))

(deftest export-between-callbacks-does-not-reuse-ids
  (runtime/reset-callbacks!)
  (let [b-fired (atom 0)
        x-fired (atom 0)
        !shift (atom false)
        tree (fn []
               (ui/vstack
                (ui/button "A" #(reset! !shift true))
                (if @!shift
                  (ui/button "X" #(swap! x-fired inc))
                  (ui/button "B" #(swap! b-fired inc)))))
        gen1 (runtime/export-tree tree)
        id-a (get-in gen1 [:children 0 :on-click])
        id-b (get-in gen1 [:children 1 :on-click])]
    (is (string? id-a))
    (is (string? id-b))
    (runtime/invoke-callback! id-a)
    (let [gen2 (runtime/export-tree tree)
          id-x (get-in gen2 [:children 1 :on-click])]
      (is (= "X" (get-in gen2 [:children 1 :text])))
      (is (not= id-b id-x) "X must not reuse B's previous id")
      (is (= {:ok false :error (str "unknown callback " id-b)}
             (runtime/invoke-callback! id-b)))
      (is (zero? @x-fired) "stale id must not invoke X")
      (is (zero? @b-fired)))))

(deftest callback-ids-are-monotonic-and-stale-ids-fail-closed
  (runtime/reset-callbacks!)
  (let [a-fired (atom 0)
        b-fired (atom 0)
        id1 (:on-click (runtime/export-tree (ui/button "A" #(swap! a-fired inc))))
        id2 (:on-click (runtime/export-tree (ui/button "B" #(swap! b-fired inc))))]
    (is (string? id1))
    (is (string? id2))
    (is (not= id1 id2) "ids are not reused across exports")
    (is (= {:ok false :error (str "unknown callback " id1)}
           (runtime/invoke-callback! id1)))
    (is (zero? @a-fired) "stale id must not run A")
    (is (zero? @b-fired))
    (is (= {:ok true :id id2} (runtime/invoke-callback! id2)))
    (is (= 1 @b-fired) "current id still runs B")
    (is (zero? @a-fired))))

(deftest callback-batch-keeps-generation-when-tree-would-shift
  (runtime/reset-callbacks!)
  (let [b-fired (atom 0)
        x-fired (atom 0)
        !shift (atom false)
        tree (fn []
               (ui/vstack
                (ui/button "A" #(reset! !shift true))
                (if @!shift
                  (ui/button "X" #(swap! x-fired inc))
                  (ui/button "B" #(swap! b-fired inc)))))
        gen1 (runtime/export-tree tree)
        id-a (get-in gen1 [:children 0 :on-click])
        id-b (get-in gen1 [:children 1 :on-click])]
    (is (= {:ok true :results [{:ok true :id id-a} {:ok true :id id-b}]}
           (runtime/invoke-callback-batch! [{:id id-a} {:id id-b}])))
    (is (true? @!shift))
    (is (= 1 @b-fired) "B ran against the pre-export registry")
    (is (zero? @x-fired))))

(deftest callback-batch-stops-on-first-failure
  (runtime/reset-callbacks!)
  (let [b-fired (atom 0)
        exported (runtime/export-tree (ui/button "B" #(swap! b-fired inc)))
        id-b (:on-click exported)
        result (runtime/invoke-callback-batch!
                [{:id "cb-missing"} {:id id-b}])]
    (is (false? (:ok result)))
    (is (= "unknown callback cb-missing" (get-in result [:results 0 :error])))
    (is (zero? @b-fired) "later callbacks do not run after a failed prerequisite")))

(deftest callback-batch-throw-stops-later-items
  (runtime/reset-callbacks!)
  (let [b-fired (atom 0)
        exported (runtime/export-tree
                  (ui/vstack
                   (ui/button "A" #(throw (ex-info "boom" {})))
                   (ui/button "B" #(swap! b-fired inc))))
        id-a (get-in exported [:children 0 :on-click])
        id-b (get-in exported [:children 1 :on-click])]
    (is (thrown? clojure.lang.ExceptionInfo
                 (runtime/invoke-callback-batch! [{:id id-a} {:id id-b}])))
    (is (zero? @b-fired) "a thrown handler is a failed prerequisite")))

(deftest list-dialog-menu-batch-order
  (runtime/reset-callbacks!)
  (let [log (atom [])
        exported (runtime/export-tree
                  (ui/vstack
                   (ui/list [{:id :alpha :label "Alpha"}]
                            {:on-change #(swap! log conj [:change %])
                             :on-confirm #(swap! log conj [:confirm %])})
                   (ui/dialog true {:on-ok #(swap! log conj :ok)
                                    :on-close #(swap! log conj :close)
                                    :on-open-change #(swap! log conj [:open %])}
                              (ui/label "x"))
                   (ui/dropdown-menu [{:id :copy :label "Copy" :on-click #(swap! log conj :item)}]
                                     {:on-change #(swap! log conj [:menu %])}
                                     (ui/button "Edit"))))
        children (:children exported)
        list-change (get-in children [0 :on-change])
        list-confirm (get-in children [0 :on-confirm])
        on-ok (get-in children [1 :on-ok])
        on-close (get-in children [1 :on-close])
        on-open (get-in children [1 :on-open-change])
        item-click (get-in children [2 :items 0 :on-click])
        menu-change (get-in children [2 :on-change])]
    (is (every? string? [list-change list-confirm on-ok on-close on-open item-click menu-change]))
    (is (:ok (runtime/invoke-callback-batch!
              [{:id list-change :value "alpha"}
               {:id list-confirm :value "alpha"}])))
    (is (= [[:change :alpha] [:confirm :alpha]] @log))
    (reset! log [])
    (is (:ok (runtime/invoke-callback-batch!
              [{:id on-ok} {:id on-close} {:id on-open :value false}])))
    (is (= [:ok :close [:open false]] @log))
    (reset! log [])
    (is (:ok (runtime/invoke-callback-batch!
              [{:id item-click} {:id menu-change :value "copy"}])))
    (is (= [:item [:menu :copy]] @log))
    (runtime/reset-callbacks!)
    (let [cancel-log (atom [])
          cancel-tree (runtime/export-tree
                       (ui/dialog true {:on-cancel #(swap! cancel-log conj :cancel)
                                        :on-close #(swap! cancel-log conj :close)
                                        :on-open-change #(swap! cancel-log conj [:open %])}
                                  (ui/label "x")))]
      (is (:ok (runtime/invoke-callback-batch!
                [{:id (:on-cancel cancel-tree)}
                 {:id (:on-close cancel-tree)}
                 {:id (:on-open-change cancel-tree) :value false}])))
      (is (= [:cancel :close [:open false]] @cancel-log)))))

(defn- exported-table
  [tree]
  (->> (tree-seq :children :children tree)
       (filter #(= "data-table" (:type %)))
       first))

(deftest table-double-click-batch-keeps-generation-when-tree-would-shift
  (runtime/reset-callbacks!)
  (let [confirm-fired (atom 0)
        x-fired (atom 0)
        seen (atom [])
        !shift (atom false)
        tree (fn []
               (ui/vstack
                ;; Two leading widgets so a render between A and B would
                ;; assign confirm's old cb-N to X, not back to the table.
                (when @!shift (ui/button "pad" (fn [])))
                (when @!shift (ui/button "X" #(swap! x-fired inc)))
                (ui/data-table {:columns [{:id :n :label "N"}]
                                :rows [{:id :ada :cells ["Ada"]}]
                                :on-change (fn [id]
                                             (reset! !shift true)
                                             (swap! seen conj [:change id]))
                                :on-confirm (fn [id]
                                              (swap! confirm-fired inc)
                                              (swap! seen conj [:confirm id]))})))
        gen1 (runtime/export-tree tree)
        table (exported-table gen1)
        id-change (:on-change table)
        id-confirm (:on-confirm table)]
    (is (string? id-change))
    (is (string? id-confirm))
    (is (:ok (runtime/invoke-callback-batch!
              [{:id id-change :value "ada"}
               {:id id-confirm :value "ada"}])))
    (is (= [[:change :ada] [:confirm :ada]] @seen))
    (is (= 1 @confirm-fired) "confirm ran against the pre-export registry")
    (is (zero? @x-fired) "X must not run during the same-generation batch")
    (let [gen2 (runtime/export-tree tree)
          new-table (exported-table gen2)
          x-id (get-in gen2 [:children 1 :on-click])]
      (is (= "pad" (get-in gen2 [:children 0 :text])))
      (is (= "X" (get-in gen2 [:children 1 :text])))
      (is (not= id-confirm x-id)
          "after a render, confirm's old id is not reused by X")
      (is (not= id-confirm (:on-confirm new-table)))
      (is (= {:ok false :error (str "unknown callback " id-confirm)}
             (runtime/invoke-callback! id-confirm)))
      (is (zero? @x-fired) "stale confirm id must not invoke X")
      (is (= 1 @confirm-fired) "stale confirm id no longer invokes confirm"))))

(deftest table-empty-batch-is-ok
  (runtime/reset-callbacks!)
  (is (= {:ok true :results []} (runtime/invoke-callback-batch! []))))

(deftest table-single-click-is-only-on-change
  (runtime/reset-callbacks!)
  (let [log (atom [])
        table (exported-table
               (runtime/export-tree
                (ui/data-table {:columns [{:id :n :label "N"}]
                                :rows [{:id :ada :cells ["Ada"]}
                                       {:id :grace :cells ["Grace"]}]
                                :on-change #(swap! log conj [:change %])
                                :on-confirm #(swap! log conj [:confirm %])})))]
    (is (:ok (runtime/invoke-callback-batch!
              [{:id (:on-change table) :value "grace"}])))
    (is (= [[:change :grace]] @log))))

(deftest table-double-click-alias-batches-with-on-change
  (runtime/reset-callbacks!)
  (let [log (atom [])
        table (exported-table
               (runtime/export-tree
                (ui/data-table {:columns [{:id :n :label "N"}]
                                :rows [{:id :ada :cells ["Ada"]}]
                                :on-change #(swap! log conj [:change %])
                                :on-double-click #(swap! log conj [:dbl %])})))]
    (is (nil? (:on-confirm table)))
    (is (:ok (runtime/invoke-callback-batch!
              [{:id (:on-change table) :value "ada"}
               {:id (:on-double-click table) :value "ada"}])))
    (is (= [[:change :ada] [:dbl :ada]] @log))))

(deftest table-confirm-only-still-fires
  (runtime/reset-callbacks!)
  (let [got (atom nil)
        table (exported-table
               (runtime/export-tree
                (ui/data-table {:columns [{:id :n :label "N"}]
                                :rows [{:id :ada :cells ["Ada"]}]
                                :on-confirm #(reset! got %)})))]
    (is (nil? (:on-change table)))
    (is (:ok (runtime/invoke-callback-batch!
              [{:id (:on-confirm table) :value "ada"}])))
    (is (= :ada @got))))

(deftest table-change-only-double-click-is-one-callback
  (runtime/reset-callbacks!)
  (let [log (atom [])
        table (exported-table
               (runtime/export-tree
                (ui/data-table {:columns [{:id :n :label "N"}]
                                :rows [{:id :ada :cells ["Ada"]}]
                                :on-change #(swap! log conj %)})))]
    (is (nil? (:on-confirm table)))
    (is (:ok (runtime/invoke-callback-batch!
              [{:id (:on-change table) :value "ada"}])))
    (is (= [:ada] @log))))

(deftest table-host-style-double-click-is-one-export
  (let [buf (java.io.StringWriter.)
        exports (atom 0)
        seen (atom [])
        !shift (atom false)
        tree (fn []
               (swap! exports inc)
               (ui/vstack
                (when @!shift (ui/button "pad" (fn [])))
                (when @!shift (ui/button "X" (fn [] :x)))
                (ui/data-table {:columns [{:id :n :label "N"}]
                                :rows [{:id :ada :cells ["Ada"]}]
                                :on-change (fn [id]
                                             (reset! !shift true)
                                             (swap! seen conj [:change id]))
                                :on-confirm (fn [id]
                                              (swap! seen conj [:confirm id]))})))]
    (runtime/bind-connection! {:out buf})
    (try
      (runtime/reset-callbacks!)
      (let [table (exported-table (runtime/export-tree tree))
            id-change (:on-change table)
            id-confirm (:on-confirm table)]
        (reset! exports 0)
        (runtime/handle {:op "callback" :callback-id id-change
                         :value "ada" :defer-render true :id 1})
        (runtime/handle {:op "callback" :callback-id id-confirm
                         :value "ada" :defer-render true :id 2})
        (is (zero? @exports))
        (is (= [[:change :ada] [:confirm :ada]] @seen))
        (runtime/export-tree tree)
        (is (= 1 @exports)))
      (finally
        (runtime/reset-callbacks!)
        (runtime/bind-connection! nil)))))

(deftest host-style-batch-does-not-export-between-items
  (let [buf (java.io.StringWriter.)
        exports (atom 0)
        b-fired (atom 0)
        x-fired (atom 0)
        !shift (atom false)
        tree (fn []
               (swap! exports inc)
               (ui/vstack
                (ui/button "A" #(reset! !shift true))
                (if @!shift
                  (ui/button "X" #(swap! x-fired inc))
                  (ui/button "B" #(swap! b-fired inc)))))]
    (runtime/bind-connection! {:out buf})
    (try
      (runtime/reset-callbacks!)
      (let [gen1 (runtime/export-tree tree)
            id-a (get-in gen1 [:children 0 :on-click])
            id-b (get-in gen1 [:children 1 :on-click])]
        (reset! exports 0)
        (runtime/handle {:op "callback" :callback-id id-a :defer-render true :id 1})
        (runtime/handle {:op "callback" :callback-id id-b :defer-render true :id 2})
        (is (zero? @exports) "callback RPCs must not export-tree")
        (is (= 1 @b-fired) "B is still the function that owned cb-2")
        (is (zero? @x-fired))
        (is (some? (runtime/lookup-callback id-b)))
        (runtime/export-tree tree)
        (is (= 1 @exports) "exactly one tree after the completed batch"))
      (finally
        (runtime/reset-callbacks!)
        (runtime/bind-connection! nil)))))

(deftest defer-render-holds-request-render-between-batch-items
  (let [buf (java.io.StringWriter.)
        !n (r/atom 0)
        !shift (atom false)]
    (runtime/bind-connection! {:out buf})
    (runtime/install-render-hook!)
    (try
      (runtime/reset-callbacks!)
      (let [tree (fn []
                   (ui/vstack
                    (ui/button "A" #(do (reset! !shift true) (swap! !n inc)))
                    (if @!shift
                      (ui/button "X" (fn [] :x))
                      (ui/button "B" #(swap! !n inc)))))
            gen (runtime/export-tree tree)
            id-a (get-in gen [:children 0 :on-click])
            id-b (get-in gen [:children 1 :on-click])]
        (runtime/handle {:op "callback" :callback-id id-a :defer-render true})
        (Thread/sleep 40)
        (is (false? (request-render-on-wire? buf))
            "defer-render keeps hold so the first callback cannot enqueue render")
        (runtime/handle {:op "callback" :callback-id id-b :defer-render true})
        (is (= 2 @!n))
        (Thread/sleep 40)
        (is (false? (request-render-on-wire? buf))
            "hold stays through the last batch item until the host render RPC"))
      (finally
        (runtime/reset-callbacks!)
        (runtime/bind-connection! nil)
        (ui/set-request-render! nil)))))

(deftest single-callback-handle-still-clears-hold
  (let [buf (java.io.StringWriter.)
        !n (r/atom 0)]
    (runtime/bind-connection! {:out buf})
    (runtime/install-render-hook!)
    (try
      (runtime/reset-callbacks!)
      (let [id (:on-click (runtime/export-tree (ui/button "A" #(swap! !n inc))))]
        (runtime/handle {:op "callback" :callback-id id})
        (is (= 1 @!n))
        (Thread/sleep 40)
        (is (false? (request-render-on-wire? buf))
            "callback-depth covers the single RPC; host still fetches the tree")
        (swap! !n inc)
        (is (wait-for-request-render buf 2000)
            "a later atom watch is not stuck behind a leftover batch hold"))
      (finally
        (runtime/reset-callbacks!)
        (runtime/bind-connection! nil)
        (ui/set-request-render! nil)))))

(deftest callback-batch-null-value-is-present
  (runtime/reset-callbacks!)
  (let [seen (atom ::unset)
        id (:on-change (runtime/export-tree
                        (ui/list [{:id :alpha :label "Alpha"}]
                                 {:on-change #(reset! seen %)})))]
    (is (:ok (runtime/invoke-callback-batch! [{:id id :value nil}])))
    (is (nil? @seen) "explicit JSON null is (f nil), not a 0-arg call")))

(def ^:private tiny-png
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")

(defn- png-header?
  [s]
  (when (string? s)
    (let [payload (if (str/starts-with? s "data:image/png;base64,")
                    (subs s (count "data:image/png;base64,"))
                    s)
          decoded (.decode (java.util.Base64/getDecoder) ^String payload)]
      (and (>= (alength decoded) 8)
           (= (vec (take 8 decoded))
              [-119 80 78 71 13 10 26 10])))))

(deftest preview-png-var-exists-and-is-nil-without-host
  (runtime/bind-connection! nil)
  (is (var? (resolve 'gpui.runtime/preview-png)))
  (is (nil? (runtime/preview-png))))

(deftest preview-png-roundtrip-via-host
  (let [buf (java.io.StringWriter.)]
    (runtime/bind-connection! {:out buf})
    (try
      (let [fut (future (#'runtime/preview-png* 2000))]
        (loop [n 0]
          (let [wire (str buf)
                id (second (re-find #"\"request-id\":\"(cap-[^\"]+)\"" wire))]
            (cond
              id (do
                   (runtime/handle {:op "preview-captured" :id 1 :request-id id :png tiny-png})
                   (let [got @fut]
                     (is (= tiny-png got))
                     (is (png-header? got))))
              (> n 50) (is false "host never received capture-preview")
              :else (do (Thread/sleep 20) (recur (inc n)))))))
      (finally
        (runtime/bind-connection! nil)))))

(deftest preview-png-timeout-is-nil
  (let [buf (java.io.StringWriter.)]
    (runtime/bind-connection! {:out buf})
    (try
      (is (nil? (#'runtime/preview-png* 50)))
      (finally
        (runtime/bind-connection! nil)))))
