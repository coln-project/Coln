;; -*- lexical-binding: t; -*-

(TeX-add-style-hook
 "flir"
 (lambda ()
   (TeX-add-to-alist 'LaTeX-provided-class-options
                     '(("report" "") ("scrreprt" "" "fleqn") ("scrartcl" "")))
   (TeX-add-to-alist 'LaTeX-provided-package-options
                     '(("glossaries" "") ("amsmath" "") ("amsthm" "") ("syntax" "") ("prelude" "")))
   (TeX-run-style-hooks
    "latex2e"
    "scrartcl"
    "scrartcl10"
    "prelude")
   (LaTeX-add-labels
    "fig:lexical-structure"
    "fig:grammar"
    "fig:graph-schema"
    "def:primary-key"
    "fig:simple-digraph"
    "sec:unions"
    "sec:propositions"
    "def:term"
    "def:atomic-proposition"
    "def:equality-proposition"
    "sec:rules"
    "sec:laws"))
 :latex)

