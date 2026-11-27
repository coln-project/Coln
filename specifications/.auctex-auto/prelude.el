;; -*- lexical-binding: t; -*-

(TeX-add-style-hook
 "prelude"
 (lambda ()
   (TeX-add-to-alist 'LaTeX-provided-package-options
                     '(("amsmath" "") ("amsthm" "") ("syntax" "")))
   (TeX-run-style-hooks
    "amsmath"
    "amsthm"
    "syntax")
   (TeX-add-symbols
    '("defcase" "Text"))
   (LaTeX-add-environments
    "definition"
    "example"))
 :latex)

