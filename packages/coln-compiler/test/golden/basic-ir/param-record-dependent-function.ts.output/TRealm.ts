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

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        Key: (new runtime.BaseSet(mbouncer, "root.Key", [])),
        key: (new runtime.BaseTableRef(
          mbouncer,
          "root.key",
          [],
          [0, 1],
          {
            flatten: (a: Runtime.RowId<"root.Key">) => {
              return [a];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return (new runtime.RowId(
                { type: "Existing", value: result[0] as Runtime.PublicRowId },
                "root.Key"
              ));
            }
          }
        )),
        E: (a: Runtime.RowId<"root.Key">) => {
          return (new runtime.BaseSet(mbouncer, "root.E", [a]));
        },
        f: (x: Runtime.RowId<"root.Key">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.f",
            [x],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.E">) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.E"
                ));
              }
            }
          ));
        },
        point: (new runtime.BaseTableRef(
          mbouncer,
          "root.point",
          [],
          [0],
          {
            flatten: (a: {}) => {
              return [];
            },
            reconstruct: (result: Runtime.PublicTuple) => {
              return {};
            }
          }
        )),
        boxed: (a: { image: Runtime.RowId<"root.E"> }) => {
          return (new runtime.BaseSet(mbouncer, "root.boxed", [a.image]));
        }
      };
    }
  };
}