/**
 * @file A data-oriented proof assistant
 * @author Benno Lossin <bl603@cam.ac.uk>
 * @license Apache-2.0 OR MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

export default grammar({
  name: "coln",

  word: $ => $.ident,
  
  extras: ($) => [
    /\s/,
    $.comment,
  ],

  rules: {
    file: $ => repeat(choice(
      $._ident_keyword,
      $._ident_symbol,
      $._symbol_token,
      $._dynamic_token,
      $._composite_symbol,
    )),
    
    // ident keywords

    _ident_keyword: $ => choice(
      $.t_sig,
      $.t_struct,
      $.t_theory,
      $.t_realm,
      $.t_def,
      $.t_let,
      $.t_open,
      $.t_import,
      $.t_showtypeb,
      $.t_showtype,
      $.t_showlevel,
      $.t_expand,
      $.t_ind,
      $.t_end,
      $.t_Set,
      $.t_Prop,
      $.t_Int,
      $.t_String,
      $.t_Inductive,
      $.t_pure,
      $.t_init,
    ),
    t_sig: $ => "sig",
    t_struct: $ => "struct",
    t_theory: $ => "theory",
    t_realm: $ => "realm",
    t_def: $ => "def",
    t_let: $ => "let",
    t_open: $ => "open",
    t_import: $ => "import",
    t_showtypeb: $ => "showtypeb",
    t_showtype: $ => "showtype",
    t_showlevel: $ => "showlevel",
    t_expand: $ => "expand",
    t_ind: $ => "ind",
    t_end: $ => "end",
    t_Set: $ => "Set",
    t_Prop: $ => "Prop",
    t_Int: $ => "Int",
    t_String: $ => "String",
    t_Inductive: $ => "Inductive",
    t_pure: $ => "pure",
    t_init: $ => "init",

    // 'ident' symbol (parts of name segments)

    _ident_symbol: $ => choice(
      $.t_less,
      $.t_greater,
      $.t_dash,
      $.t_plus,
      $.t_slash,
      $.t_star,
      $.t_tilde,
      $.t_colon,
      $.t_equals,
      $.t_at,
    ),
    t_less: $ => "<",
    t_greater: $ => ">",
    t_dash: $ => "-",
    t_plus: $ => "+",
    t_slash: $ => "/",
    t_star: $ => "*",
    t_tilde: $ => "~",
    t_colon: $ => ":",
    t_equals: $ => "=",
    t_at: $ => "@",

    // symbol tokens

    _symbol_token: $ => choice(
      $.t_paren_opn,
      $.t_paren_cls,
      $.t_brack_opn,
      $.t_brack_cls,
      $.t_brace_opn,
      $.t_brace_cls,
      $.t_comma,
      $.t_semi,
      $.t_dot,
      $.t_single_quote,
      $.t_caret,
    ),
    t_paren_opn: $ => "(",
    t_paren_cls: $ => ")",
    t_brack_opn: $ => "[",
    t_brack_cls: $ => "]",
    t_brace_opn: $ => "{",
    t_brace_cls: $ => "}",
    t_comma: $ => ",",
    t_semi: $ => ";",
    t_dot: $ => ".",
    t_single_quote: $ => "'",
    t_caret: $ => "^",

    // composite symbol tokens

    _composite_symbol: $ => choice(
      $.t_colon_equals,
    ),
    t_colon_equals: $ => ":=",

    // dynamically sized tokens

    _dynamic_token: $ => choice(
      $.number,
      $.ident,
      $.quoted_name_seg,
      $.comment,
      $.string,
    ),
    number: $ => /[0-9]+/,
    ident: $ => /[a-zA-Z_][a-zA-Z_0-9-]*/,
    quoted_name_seg: $ => /\`[^"`"\n\r\t]*\`/,
    comment: $ => /#.*/,
    string: $ => /\"[^"]*\"/,
  }
});
