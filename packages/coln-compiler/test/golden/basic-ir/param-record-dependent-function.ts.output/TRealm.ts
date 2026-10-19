import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      Key: Runtime.MutableSet<Runtime.RowId<"root.Key">>,
      key: Runtime.MutableRef<Runtime.RowId<"root.Key">>,
      E: (a: Runtime.RowId<"root.Key">) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      f: (x: Runtime.RowId<"root.Key">) => Runtime.MutableRef<Runtime.RowId<"root.E">>,
      point: Runtime.MutableRef<{}>,
      boxed: (a: {
        image: Runtime.RowId<"root.E">
      }) => Runtime.MutableSet<Runtime.RowId<"root.boxed">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        Key: (new runtime.BaseSet(mstore, "root.Key", [])),
        key: (new runtime.BaseTableRef(
          mstore,
          "root.key",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.Key">) => {
              return [a];
            },
            reconstruct: (result: Runtime.WireTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.WireRowId },
                "root.Key"
              ));
            }
          }
        )),
        E: (a: Runtime.RowId<"root.Key">) => {
          return (new runtime.BaseSet(mstore, "root.E", [a]));
        },
        f: (x: Runtime.RowId<"root.Key">) => {
          return (new runtime.BaseTableRef(
            mstore,
            "root.f",
            [x],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.E">) => {
                return [a];
              },
              reconstruct: (result: Runtime.WireTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.WireRowId },
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
            reconstruct: (result: Runtime.WireTuple) => {
              return {};
            }
          }
        )),
        boxed: (a: { image: Runtime.RowId<"root.E"> }) => {
          return (new runtime.BaseSet(mstore, "root.boxed", [a.image]));
        }
      };
    }
  };
}