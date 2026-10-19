;;;coln.el --- basic emacs mode for Coln

(require 'font-lock)

(defun coln-setup ()
  (setq-local tab-width 2))

(define-generic-mode coln-mode
  (list ?#)
  (list "def" "theory" "sig" "struct" "realm" "showtypeb" "showtype"
  "showlevel" "ind" "end" "Set" "Prop" "Int" "String" "init")
  '(("'\\w+" . 'font-lock-constant-face))
  (list "\\.coln$")
  (list 'coln-setup)
  "Major mode for coln files.")

(sp-with-modes '(coln-mode)
  ;; Disable ' because it is used in value names and types
  (sp-local-pair "'" nil :actions nil))

(provide 'coln)

;;; coln.el ends here
