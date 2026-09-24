import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    outer: { inner: runtime.MutableRef<{ point: runtime.RowId<"root.X"> }> }
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      outer: {
        inner: (new runtime.BaseTableRef(
          mstore,
          "root.outer.inner",
          [],
          [0, 1],
          {
            flatten: (a: { point: runtime.RowId<"root.X"> }) => {
              return [a.point];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return {
                point: (new runtime.RowId(
                  { type: "Existing", value: result[0] as runtime.WireRowId },
                  "root.X"
                ))
              };
            }
          }
        ))
      }
    };
  }
}