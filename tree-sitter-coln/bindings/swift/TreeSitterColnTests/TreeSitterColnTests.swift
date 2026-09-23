import XCTest
import SwiftTreeSitter
import TreeSitterColn

final class TreeSitterColnTests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_coln())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading Coln grammar")
    }
}
