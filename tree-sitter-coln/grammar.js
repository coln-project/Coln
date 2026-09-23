/**
 * @file A data-oriented proof assistant
 * @author Benno Lossin <bl603@cam.ac.uk>
 * @license Apache-2.0 OR MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

export default grammar({
  name: "coln",

  rules: {
    // TODO: add the actual grammar rules
    source_file: $ => "hello"
  }
});
