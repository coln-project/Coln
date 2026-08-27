export type Commit
    = { tag: "existing", hash: string }
    | { tag: "inprogress", id: number }

export function commitEquals(c0: Commit, c1: Commit): boolean {
    if (c0.tag == "existing" && c1.tag == "existing") {
        return c0.hash == c1.hash
    } else if (c0.tag == "inprogress" && c1.tag == "inprogress") {
        return c0.id == c1.id
    } else {
        return false;
    }
}

export class RowId {
    constructor(public commit: Commit, public counter: number) {}

    equals(other: RowId): boolean {
        return (commitEquals(this.commit, other.commit) && this.counter == other.counter)
    }
}

export type Scalar = string | number | RowId

export function scalarEqual(v0: Scalar, v1: Scalar): boolean {
    if (typeof (v0) == "string" && typeof (v1) == "string") {
        return v0 == v1
    } else if (typeof (v0) == "number" && typeof (v1) == "number") {
        return v0 == v1
    } else if (v0 instanceof RowId && v1 instanceof RowId) {
        return v0.equals(v1)
    } else {
        return false
    }
}

const STRING_TAG = 0
const INT_TAG = 1
const EXISTING_ROW_ID_TAG = 2
const INPROGRESS_ROW_ID_TAG = 3

const HASH_LEN = 32

export class EncodedTuple {
    constructor(protected data: Buffer, protected items: number) {}

    tag(i: number): number {
        return this.data[4 + i]
    }

    set_tag(i: number, tag: number) {
        this.data[4 + i] = tag
    }

    value_32_0(i: number): number {
        return this.data.readUint32LE(4 + this.items + i * 8)
    }

    set_value_32_0(i: number, v: number) {
        this.data.writeUint32LE(v, 4 + this.items + i * 8)
    }

    value_32_1(i: number): number {
        return this.data.readUint32LE(4 + this.items + i * 8 + 4)
    }

    set_value_32_1(i: number, v: number) {
        this.data.writeUint32LE(v, 4 + this.items + i * 8 + 4)
    }

    storage_slice(start: number, len: number): Buffer {
        const realStart = 4 + this.items * 9 + start
        return this.data.subarray(realStart, realStart + len)
    }
}

export class EncodedTupleReader extends EncodedTuple {
    private decoder: TextDecoder

    constructor(data: Buffer) {
        const items = data.readUint32LE(0)
        super(data, items)
        this.decoder = new TextDecoder()
    }

    readString(i: number): string {
        const start = this.value_32_0(i)
        const len = this.value_32_1(i)
        return this.decoder.decode(this.storage_slice(start, len))
    }

    readInt(i: number): number {
        return this.value_32_0(i)
    }

    readRowId(i: number): RowId {
        const tag = this.tag(i)
        var commit: Commit;
        if (tag == EXISTING_ROW_ID_TAG) {
            const hashstart = this.value_32_0(i)
            commit = {
                tag: "existing",
                hash: this.storage_slice(hashstart, HASH_LEN).toString("base64")
            }
        } else if (tag == INPROGRESS_ROW_ID_TAG) {
            commit = {
                tag: "inprogress",
                id: this.value_32_0(i)
            }
        } else {
            throw "bad row id"
        }
        return new RowId(commit, this.value_32_1(i))
    }
}

export class EncodedTupleWriter extends EncodedTuple {
    private storagepos: number
    private encoder: TextEncoder

    constructor(items: number, storageSize: number) {
        const data = Buffer.alloc(4 + items * 9 + storageSize)
        data.writeUint32LE(items, 0)
        super(data, items)
        this.storagepos = 0
        this.encoder = new TextEncoder()
    }

    private remainingStorage(): Buffer {
        return this.data.subarray(4 + this.items * 9 + this.storagepos)
    }

    writeString(i: number, v: string) {
        const len = this.encoder.encodeInto(v, this.remainingStorage()).written
        this.set_tag(i, STRING_TAG)
        this.set_value_32_0(i, this.storagepos)
        this.set_value_32_1(i, len)
        this.storagepos += len
    }

    writeInt(i: number, v: number) {
        this.set_tag(i, INT_TAG)
        this.set_value_32_0(i, v)
    }

    writeRowId(i: number, v: RowId) {
        if (v.commit.tag == "existing") {
            const decoded = Buffer.from(v.commit.hash, "base64")
            this.remainingStorage().set(decoded)
            this.set_tag(i, EXISTING_ROW_ID_TAG)
            this.set_value_32_0(i, this.storagepos)
            this.storagepos += HASH_LEN
        } else if (v.commit.tag == "inprogress") {
            this.set_tag(i, INPROGRESS_ROW_ID_TAG)
            this.set_value_32_0(i, v.commit.id)
        }
        this.set_value_32_1(i, v.counter)
    }

    finish(): Buffer {
        return this.data.subarray(0, 4 + this.items * 9 + this.storagepos)
    }
}
