import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{
      payload: { name: string, inner: { rank: number } }
    }>;

    constructor(mbouncer: Runtime.ManagedBouncer) {
      this.root = (new runtime.BaseTableRef(
        mbouncer,
        "root",
        [],
        [0, 1, 2],
        {
          flatten: (a: {
            payload: { name: string, inner: { rank: number } }
          }) => {
            return [a.payload.name, a.payload.inner.rank];
          },
          reconstruct: (result: Runtime.PublicTuple) => {
            return { payload: { name: result[0], inner: { rank: result[1] } } };
          }
        }
      ));
    }
  };
}