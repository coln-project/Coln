// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use serde::{Deserialize, Serialize};
use specta::{
    Type, Types,
    datatype::{DataType, Reference},
};

use crate::hash::CommitHash;

/// One encoded commit, for sending to another replica (e.g. via subduction).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize, Type)]
pub struct Fragment {
    pub hash: CommitHash,
    pub parents: Vec<CommitHash>,
    // Serialized as bytes rather than a sequence, so that serializers with
    // native byte buffers (e.g. napi) produce a `Uint8Array`.
    #[serde(with = "serde_bytes")]
    #[specta(type = Uint8Array)]
    pub bytes: Vec<u8>,
}

/// Exported as TypeScript's `Uint8Array`.
struct Uint8Array;

impl Type for Uint8Array {
    fn definition(_: &mut Types) -> DataType {
        let reference: Reference = specta_typescript::define("Uint8Array");
        reference.into()
    }
}
