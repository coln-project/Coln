import assert from "node:assert/strict";
import test from "node:test";
import { EncodedTupleWriter, EncodedTupleReader, RowId } from "../src/value.js"
import { randomBytes } from "node:crypto"

function randomHash(): string {
    return randomBytes(32).toString("base64")
}

test("encode decode roundtrip", () => {
    const w = new EncodedTupleWriter(4, 200)
    const h0 = randomHash()
    const i0 = new RowId({ tag: "existing", hash: h0 }, 0)
    const i1 = new RowId({ tag: "inprogress", id: 3 }, 1)
    w.writeString(0, "hello")
    w.writeInt(1, 3)
    w.writeRowId(2, i0)
    w.writeRowId(3, i1)
    const r = new EncodedTupleReader(w.finish())
    assert(r.readString(0) == "hello")
    assert(r.readInt(1) == 3)
    assert(r.readRowId(2).equals(i0))
    assert(r.readRowId(3).equals(i1))
})
