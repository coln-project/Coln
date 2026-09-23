package io.github.treesitter.jtreesitter.coln;

import java.lang.foreign.*;

public final class TreeSitterColn {
    private static final ValueLayout VOID_PTR =
            ValueLayout.ADDRESS.withTargetLayout(MemoryLayout.sequenceLayout(Long.MAX_VALUE, ValueLayout.JAVA_BYTE));
    private static final FunctionDescriptor FUNC_DESC = FunctionDescriptor.of(VOID_PTR);
    private static final Linker LINKER = Linker.nativeLinker();
    private static final TreeSitterColn INSTANCE = new TreeSitterColn();

    private final Arena arena = Arena.ofAuto();
    private volatile SymbolLookup lookup = null;

    private TreeSitterColn() {}

    /**
     * Get the tree-sitter language for this grammar.
     */
    public static MemorySegment language() {
        if (INSTANCE.lookup == null)
            INSTANCE.lookup = INSTANCE.findLibrary();
        return language(INSTANCE.lookup);
    }

    /**
     * Get the tree-sitter language for this grammar.
     *
     * <strong>The {@linkplain Arena} used in the {@code lookup}
     * must not be closed while the language is being used.</strong>
     */
    public static MemorySegment language(SymbolLookup lookup) {
        return call(lookup, "tree_sitter_coln");
    }

    private SymbolLookup findLibrary() {
        try {
            var library = System.mapLibraryName("tree-sitter-coln");
            return SymbolLookup.libraryLookup(library, arena);
        } catch (IllegalArgumentException ex1) {
            try {
                System.loadLibrary("tree-sitter-coln");
                return SymbolLookup.loaderLookup();
            } catch (UnsatisfiedLinkError ex2) {
                ex1.addSuppressed(ex2);
                throw ex1;
            }
        }
    }

    private static UnsatisfiedLinkError unresolved(String name) {
        return new UnsatisfiedLinkError("Unresolved symbol: %s".formatted(name));
    }

    @SuppressWarnings("SameParameterValue")
    private static MemorySegment call(SymbolLookup lookup, String name) throws UnsatisfiedLinkError {
        var address = lookup.find(name).orElseThrow(() -> unresolved(name));
        try {
            var function = LINKER.downcallHandle(address, FUNC_DESC);
            return (MemorySegment) function.invokeExact();
        } catch (Throwable e) {
            throw new RuntimeException("Call to %s failed".formatted(name), e);
        }
    }
}
