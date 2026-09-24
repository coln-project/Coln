import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    Key: runtime.MutableSet<runtime.RowId<"root.Key">>,
    key: runtime.MutableRef<runtime.RowId<"root.Key">>,
    E: (a: runtime.RowId<"root.Key">) => runtime.MutableSet<runtime.RowId<"root.E">>,
    f: (x: runtime.RowId<"root.Key">) => runtime.MutableRef<runtime.RowId<"root.E">>,
    point: runtime.MutableRef<{}>,
    boxed: (a: {
      image: runtime.RowId<"root.E">
    }) => runtime.MutableSet<runtime.RowId<"root.boxed">>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      Key: (new runtime.BaseSet(mstore, "root.Key", [])),
      key: (new runtime.BaseTableRef(
        mstore,
        "root.key",
        [],
        [0, 1],
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
      )),
      E: (a: runtime.RowId<"root.Key">) => {
        return (new runtime.BaseSet(mstore, "root.E", [a]));
      },
      f: (x: runtime.RowId<"root.Key">) => {
        return (new runtime.BaseTableRef(
          mstore,
          "root.f",
          [x],
          [1, 2],
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
      },
      point: (new runtime.BaseTableRef(
        mstore,
        "root.point",
        [],
        [0],
        {
          flatten: (a: {}) => {
            return [];
          },
          reconstruct: (result: runtime.WireTuple) => {
            return {};
          }
        }
      )),
      boxed: (a: { image: runtime.RowId<"root.E"> }) => {
        return (new runtime.BaseSet(mstore, "root.boxed", [a.image]));
      }
    };
  }
}