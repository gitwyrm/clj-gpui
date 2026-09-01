(ns gpui.platform-test
  (:require [clojure.test :refer [deftest is testing]]
            [gpui.platform :as platform]
            [gpui.runtime :as runtime]))

(deftest pick-directory-registers-and-delivers
  (platform/clear-pending!)
  (let [buf (java.io.StringWriter.)
        got (atom nil)]
    (runtime/bind-connection! {:out buf})
    (try
      (let [id (platform/pick-directory
                {:title "Choose a folder"}
                #(reset! got %))]
        (is (= [id] (platform/pending-picks)))
        (is (re-find #"pick-directory" (str buf)))
        (is (re-find #"Choose a folder" (str buf)))
        (platform/deliver-pick! {:request-id id :path "/tmp/data"})
        (is (= {:path "/tmp/data"} @got))
        (is (empty? (platform/pending-picks))))
      (finally
        (runtime/bind-connection! nil)
        (platform/clear-pending!)))))

(deftest pick-directory-cancelled-and-error
  (platform/clear-pending!)
  (let [buf (java.io.StringWriter.)
        got (atom nil)]
    (runtime/bind-connection! {:out buf})
    (try
      (testing "cancelled"
        (let [id (platform/pick-directory #(reset! got %))]
          (platform/deliver-pick! {:request-id id :cancelled true})
          (is (= {:cancelled true} @got))))
      (testing "error"
        (let [id (platform/pick-directory #(reset! got %))]
          (platform/deliver-pick! {:request-id id :error "no portal"})
          (is (= {:error "no portal"} @got))))
      (finally
        (runtime/bind-connection! nil)
        (platform/clear-pending!)))))

(deftest reveal-and-open-send-ops
  (let [buf (java.io.StringWriter.)]
    (runtime/bind-connection! {:out buf})
    (try
      (platform/reveal-path! "/tmp/a")
      (platform/open-path! "/tmp/b")
      (let [s (str buf)]
        (is (re-find #"reveal-path" s))
        (is (re-find #"open-path" s))
        (is (re-find #"tmp" s)))
      (finally
        (runtime/bind-connection! nil)))))

(deftest directory-picked-reaches-platform
  (platform/clear-pending!)
  (let [buf (java.io.StringWriter.)
        got (atom nil)]
    (runtime/bind-connection! {:out buf})
    (try
      (let [id (platform/pick-directory #(reset! got %))]
        (runtime/handle {:op "directory-picked" :id 9 :request-id id :path "/var/tmp"})
        (is (= {:path "/var/tmp"} @got))
        (is (re-find #"\"op\":\"response\"" (str buf))))
      (finally
        (runtime/bind-connection! nil)
        (platform/clear-pending!)))))
