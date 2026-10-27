import type * as Runtime from "@coln-project/interface";

export function createRealm(runtime: typeof Runtime) {
  return class TRealm {
    root: Runtime.MutableRef<{
      payload: { name: string, inner: { rank: number } }
    }>;

    constructor(mstore: Runtime.ManagedStore) {
      this.root = (new runtime.BaseRef(
        mstore,
        "root",
        [],
        [0, 1, 2],
        {
          flatten: (a: {
            payload: { name: string, inner: { rank: number } }
          }) => {
            return [a.payload.name, a.payload.inner.rank];
          },
          reconstruct: (result: Runtime.WireTuple) => {
            return {
              payload: {
                name: result[0] as string,
                inner: { rank: result[1] as number }
              }
            };
          }
        }
      ));
    }
  };
}