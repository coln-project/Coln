import io.github.treesitter.jtreesitter.Language;
import io.github.treesitter.jtreesitter.coln.TreeSitterColn;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;

public class TreeSitterColnTest {
    @Test
    public void testCanLoadLanguage() {
        assertDoesNotThrow(() -> new Language(TreeSitterColn.language()));
    }
}
