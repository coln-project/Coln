import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      X: Runtime.MutableSet<Runtime.RowId<"root.X">>,
      result: (expected: Runtime.RowId<"root.X">) => Runtime.MutableRef<{
        actual: Runtime.RowId<"root.X">,
        evidence: { proof: null }
      }>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        X: (new runtime.BaseSet(mstore, "root.X", [])),
        result: (expected: Runtime.RowId<"root.X">) => {
          return (new runtime.BaseRef(
            mstore,
            "root.result",
            [expected],
            [1, 2],
            {
              flatten: (a: {
                actual: Runtime.RowId<"root.X">,
                evidence: { proof: null }
              }) => {
                return [a.actual];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return {
                  actual: (new runtime.RowId(
                    { type: "Existing", value: result[0] as Runtime.WireRowId },
                    "root.X"
                  )),
                  evidence: { proof: null }
                };
              }
            }
          ));
        }
      };
    }
  };
}