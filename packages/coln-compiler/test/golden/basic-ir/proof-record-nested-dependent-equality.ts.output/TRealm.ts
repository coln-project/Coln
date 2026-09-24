import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    result: (expected: runtime.RowId<"root.X">) => runtime.MutableRef<{
      actual: runtime.RowId<"root.X">,
      evidence: { proof: null }
    }>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      result: (expected: runtime.RowId<"root.X">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.result",
          [expected],
          [1, 2],
          {
            flatten: (a: {
              actual: runtime.RowId<"root.X">,
              evidence: { proof: null }
            }) => {
              return [a.actual];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return {
                actual: (new runtime.RowId(
                  { type: "Existing", value: result[0] as runtime.WireRowId },
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
}