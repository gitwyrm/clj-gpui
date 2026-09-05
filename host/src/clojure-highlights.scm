; Supplement the grammar's literal/comment captures with Clojure forms.
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
((sym_lit) @keyword
 (#match? @keyword "^(ns|def|defn|defn-|defmacro|fn|let|letfn|if|if-not|when|when-not|do|loop|recur|try|catch|finally|throw|quote|var|set!|new)$"))
