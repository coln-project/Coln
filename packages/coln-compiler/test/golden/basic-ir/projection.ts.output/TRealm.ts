import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    X: runtime.MutableSet<runtime.RowId<"root.X">>,
    E: (a: runtime.RowId<"root.X">) => runtime.MutableSet<runtime.RowId<"root.E">>,
    r: (p: {
      first: runtime.RowId<"root.X">,
      second: runtime.RowId<"root.X">
    }) => runtime.MutableRef<runtime.RowId<"root.E">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      X: (new runtime.BaseSet(mstore, "root.X", [])),
      E: (a: runtime.RowId<"root.X">) => {
        return (new runtime.BaseSet(mstore, "root.E", [a]));
      },
      r: (p: {
        first: runtime.RowId<"root.X">,
        second: runtime.RowId<"root.X">
      }) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.r",
          [p.first, p.second],
          [2, 3],
          {
            flatten: (a: runtime.RowId<"root.E">) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.E"
              ));
            }
          }
        ));
      }
    };
  }
}