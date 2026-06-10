// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use cli_table::{
    Cell, CellStruct, Table,
    format::{Border, HorizontalLine, Justify, Separator},
};
use dbsp::{ZWeight, utils::Tup2};
use std::fmt::Debug;

pub trait AsCliTableRow {
    fn cli_table_header() -> Vec<CellStruct>;
    fn as_cli_table_row(&self) -> Vec<CellStruct>;
}

impl<T: ?Sized + AsCliTableRow> AsCliTableRow for &T {
    fn cli_table_header() -> Vec<CellStruct> {
        T::cli_table_header()
    }
    fn as_cli_table_row(&self) -> Vec<CellStruct> {
        T::as_cli_table_row(*self)
    }
}

pub trait ToCliTable {
    fn to_cli_table(self) -> impl std::fmt::Display;
}

impl<Iter> ToCliTable for Iter
where
    Iter: IntoIterator,
    Iter::Item: AsCliTableRow,
{
    fn to_cli_table(self) -> impl std::fmt::Display {
        let double_h_line = HorizontalLine::new('=', '=', '≠', '=');
        let single_h_line = HorizontalLine::new('-', '-', '+', '-');
        self.into_iter()
            .map(|row| row.as_cli_table_row())
            .table()
            .border(
                Border::builder()
                    .top(double_h_line)
                    .bottom(double_h_line)
                    .build(),
            )
            .separator(Separator::builder().title(Some(single_h_line)).build())
            .title(Iter::Item::cli_table_header())
            .display()
            .expect("Table build error")
    }
}

impl<K: Debug, V: Debug, ZWeight: Debug> AsCliTableRow for (K, V, ZWeight) {
    fn cli_table_header() -> Vec<CellStruct> {
        vec!["z-weight".cell(), "key".cell(), "value".cell()]
    }
    fn as_cli_table_row(&self) -> Vec<CellStruct> {
        vec![
            format!("{:?}", self.2).cell().justify(Justify::Right),
            format!("{:?}", self.0).cell().justify(Justify::Left),
            format!("{:?}", self.1).cell().justify(Justify::Left),
        ]
    }
}

impl<K: Debug> AsCliTableRow for Tup2<K, ZWeight> {
    fn cli_table_header() -> Vec<CellStruct> {
        vec!["z-weight".cell(), "key".cell()]
    }
    fn as_cli_table_row(&self) -> Vec<CellStruct> {
        vec![
            format!("{:?}", self.1).cell().justify(Justify::Right),
            format!("{:?}", self.0).cell().justify(Justify::Left),
        ]
    }
}
