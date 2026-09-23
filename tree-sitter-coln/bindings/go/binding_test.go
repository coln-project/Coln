package tree_sitter_coln_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_coln "github.com/coln-project/coln/bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_coln.Language())
	if language == nil {
		t.Errorf("Error loading Coln grammar")
	}
}
