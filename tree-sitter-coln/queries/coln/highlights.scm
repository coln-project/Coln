[
  (t_sig)
  (t_struct)
  (t_theory)
  (t_realm)
  (t_def)
] @keyword.type
[(t_let) (t_end) (t_init)] @keyword

[(t_import) (t_open)] @keyword.import

[(t_showtype) (t_showtypeb) (t_showlevel)] @keyword

[(t_expand)] @keyword
[(t_ind)] @keyword.modifier

[(t_Set) (t_Prop) (t_Int) (t_String)] @type.builtin

[(t_Inductive) (t_pure)] @keyword

[
  (t_colon)
  (t_comma)
  (t_dot)
  (t_slash)
  (t_semi)
  (t_caret)
  (t_single_quote)
] @punctuation.delimiter

[
  (t_dash)
  (t_less)
  (t_greater)
  (t_plus)
  (t_star)
  (t_tilde)
  (t_equals)
  (t_at)
  (t_colon_equals)
] @operator

[
  (t_paren_opn)
  (t_paren_cls)
  (t_brack_opn)
  (t_brack_cls)
  (t_brace_opn)
  (t_brace_cls)
] @punctuation.bracket

(comment) @comment
(string) @string
(number) @number
[(ident) (quoted_name_seg)] @variable
