import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: {
      A: Runtime.MutableProp,
      B: (a: null) => Runtime.MutableProp,
      C: (a: null) => (b: null) => Runtime.MutableSet<Runtime.RowId<"root.C">>,
      f: (a: null) => (b: null) => Runtime.Ref<Runtime.RowId<"root.C">>
    };

    constructor(mstore: Runtime.ManagedStore) {
      this.root = {
        A: (new runtime.ViewProp(mstore, "root.A", [])),
        B: (a: null) => {
          return (new runtime.ViewProp(mstore, "root.B", []));
        },
        C: (a: null) => {
          return (b: null) => {
            return (new runtime.InductiveViewSet(mstore, "root.C", []));
          };
        },
        f: (a: null) => {
          return (b: null) => {
            return (new runtime.BaseTableRef(
              mstore,
              "root.f",
              [],
              [0, 1],
              {
                flatten: (a: Runtime.RowId<"root.C">) => {
                  return [a];
                },
                reconstruct: (result: Runtime.WireTuple) => {
                  return (new runtime.RowId(
                    { type: "Existing", value: result[0] as Runtime.WireRowId },
                    "root.C"
                  ));
                }
              }
            ));
          };
        }
      };
    }
  };
}