// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    commit::{DataCommit, RootCommit, error::CodecError, frame::Frame, header::ChunkType, wire},
    ir::Path,
    table::TableMeta,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chunk {
    Root(RootCommit),
    Data(DataCommit),
}

impl Chunk {
    pub(crate) fn from_frame<'s, F>(frame: Frame, table_meta_for: F) -> Result<Self, CodecError>
    where
        F: Fn(&Path) -> Option<TableMeta<'s>>,
    {
        let (header, bytes) = frame.into_parts();
        match header.chunk_type {
            ChunkType::RootCommit => {
                let _root = wire::deserialize_root(&bytes)?;
                Ok(Self::Root(RootCommit::from_root_payload(header, bytes)))
            }
            ChunkType::DataCommit => {
                let data = wire::deserialize(&bytes, table_meta_for)?;
                Ok(Self::Data(DataCommit::from_commit_payload(
                    header, bytes, data,
                )))
            }
        }
    }
}
