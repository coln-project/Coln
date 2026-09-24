import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    Key: runtime.MutableSet<runtime.RowId<"root.Key">>,
    f: (a: runtime.RowId<"root.Key">) => runtime.MutableRef<runtime.RowId<"root.Key">>,
    boxed: (a: {
      key: runtime.RowId<"root.Key">,
      value: string
    }) => runtime.MutableSet<runtime.RowId<"root.boxed">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      Key: (new runtime.BaseSet(mstore, "root.Key", [])),
      f: (a: runtime.RowId<"root.Key">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.f",
          [a],
          [1, 2],
          {
            flatten: (a: runtime.RowId<"root.Key">) => {
              return [a];
            },
            reconstruct: (result: runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as runtime.WireRowId },
                "root.Key"
              ));
            }
          }
        ));
      },
      boxed: (a: { key: runtime.RowId<"root.Key">, value: string }) => {
        return (new runtime.BaseSet(mstore, "root.boxed", [a.key, a.value]));
      }
    };
  }
}