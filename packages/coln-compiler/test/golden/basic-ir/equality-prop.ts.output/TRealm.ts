import * as runtime from "@coln-project/interface";

export class TRealm {
  root: {
    P: runtime.MutableProp,
    x: runtime.MutableRef<null>,
    y: runtime.MutableRef<null>,
    eq: runtime.MutableRef<null>
  };

  constructor(mstore: runtime.ManagedStore) {
    this.root = {
      P: (new runtime.BaseProp(mstore, "root.P", [])),
      x: (new runtime.ConstRef(null)),
      y: (new runtime.ConstRef(null)),
      eq: (new runtime.ConstRef(null))
    };
  }
}