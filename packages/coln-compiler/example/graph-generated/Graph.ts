import * as runtime from "@coln-project/runtime"

interface View {
  vertex: runtime.ColnSet.View;
  edge: (src: runtime.Value) => (tgt: runtime.Value) => runtime.ColnSet.View;
}

interface Transaction {
  vertex: runtime.ColnSet.Transaction;
  edge: (src: runtime.Value) => (tgt: runtime.Value) => runtime.ColnSet.Transaction;
}
