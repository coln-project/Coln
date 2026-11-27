import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableSet<Runtime.RowId<"root.A">>,
      B: Runtime.MutableSet<Runtime.RowId<"root.B">>,
      C: Runtime.MutableSet<Runtime.RowId<"root.C">>,
      E: (a: Runtime.RowId<"root.C">) => Runtime.MutableSet<Runtime.RowId<"root.E">>,
      first: (a: Runtime.RowId<"root.A">) => Runtime.MutableRef<Runtime.RowId<"root.B">>,
      second: (a: Runtime.RowId<"root.B">) => Runtime.MutableRef<Runtime.RowId<"root.C">>,
      edge: (x: Runtime.RowId<"root.A">) => Runtime.MutableRef<Runtime.RowId<"root.E">>
    };

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = {
        A: (new runtime.BaseSet(mbouncer, "root.A", [])),
        B: (new runtime.BaseSet(mbouncer, "root.B", [])),
        C: (new runtime.BaseSet(mbouncer, "root.C", [])),
        E: (a: Runtime.RowId<"root.C">) => {
          return (new runtime.BaseSet(mbouncer, "root.E", [a]));
        },
        first: (a: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.first",
            [a],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.B">) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.B"
                ));
              }
            }
          ));
        },
        second: (a: Runtime.RowId<"root.B">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.second",
            [a],
            [1, 2],
            {
              flatten: (a: Runtime.RowId<"root.C">) => {
                return [a];
              },
              reconstruct: (result: Runtime.PublicTuple) => {
                return (new runtime.RowId(
                  { type: "Existing", value: result[0] as Runtime.PublicRowId },
                  "root.C"
                ));
              }
            }
          ));
        },
        edge: (x: Runtime.RowId<"root.A">) => {
          return (new runtime.BaseTableRef(
            mbouncer,
            "root.edge",
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
        }
      };
    }
  };
}