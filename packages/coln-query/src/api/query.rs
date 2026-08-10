// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module converts coln's flattened lowered intermediate representation
//! (FLIR) into a query program expressed in [`Statements`](crate::host::stmt::Stmt),
//! using [`HostExprs`](crate::host::expr::Expr) and [`RelExprs`](crate::relational::expr::RelExpr).

use crate::api::schema::{Column, TableRef, TableSchema};
use crate::error::SyntaxError;
use crate::host::Code;
use coln_flir_rs::ir::{
    self, Atom, EntityVariant, FlatRealm, Prop, Rule, RuleEntry, TableEntry, Term,
};
use std::collections::HashMap;

type BaseTableName = TableRef;
type DerivedViewName = TableRef;

/// An identifier that uniquely identifies a table (globally across the store).
#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub struct RuleName {
    inner: String,
}

impl<T: Into<String>> From<T> for RuleName {
    fn from(value: T) -> Self {
        RuleName {
            inner: value.into(),
        }
    }
}

struct QueryProgramBuilder {
    program: Code,
    base_tables: HashMap<BaseTableName, TableSchema>,
    derived_views: HashMap<DerivedViewName, TableSchema>,
    rules: HashMap<RuleName, ir::RuleVariant>,
}

impl QueryProgramBuilder {
    fn new() -> Self {
        Self {
            program: Vec::new(),
            base_tables: HashMap::new(),
            derived_views: HashMap::new(),
            rules: HashMap::new(),
        }
    }
    pub fn from_flat_realm(flat_realm: &FlatRealm) -> Result<Self, SyntaxError> {
        let mut builder = QueryProgramBuilder::new();
        for table in &flat_realm.tables {
            builder.table_declaration(table)?;
        }
        for rule in &flat_realm.rules {
            builder.rule_declaration(rule)?;
        }
        Ok(builder)
    }

    fn table_declaration(&mut self, table_entry: &TableEntry) -> Result<(), SyntaxError> {
        let name = table_entry.path.to_string();
        let schema = &table_entry.table;

        match &schema.entity_variant {
            EntityVariant::Table => self.base_table(BaseTableName::from(name), schema),
            EntityVariant::View(materialization) => {
                unimplemented!("[Initial models] Materialized views defined through a query");
            }
            EntityVariant::Index { method, columns } => {
                unimplemented!("[Not-yet specified] Indexes")
            }
        }
    }
    fn base_table(&mut self, name: BaseTableName, schema: &ir::Schema) -> Result<(), SyntaxError> {
        let columns = schema.columns.iter().map(Column::from).collect();
        let primary_key = schema
            .primary_key
            .as_ref()
            .map_or(Ok(Vec::new()), |compound_primary_key| {
                compound_primary_key
                    .iter()
                    .map(|primary_key_column| {
                        schema
                            .columns
                            .iter()
                            .position(|column| column.path == *primary_key_column)
                            .ok_or_else(|| {
                                SyntaxError::new(format!(
                                    "Primary key column {primary_key_column} not found in base table {name}",
                                ))
                            })
                    })
                    .collect::<Result<Vec<usize>, SyntaxError>>()
            })?;
        let table_schema = TableSchema::new(name.clone(), columns, vec![primary_key]);
        self.base_tables
            .insert(name.clone(), table_schema)
            .ok_or_else(|| SyntaxError::new(format!("Base table {name} defined multiple times")))
            .map(|_old_entry| ())
    }

    fn rule_declaration(&mut self, rule_entry: &RuleEntry) -> Result<(), SyntaxError> {
        let name = rule_entry.path.to_string();
        let rule = &rule_entry.rule;
        self.rules
            .insert(RuleName::from(&name), rule.rule_variant.clone())
            .ok_or_else(|| SyntaxError::new(format!("Rule {name} defined multiple times")))?;
        match &rule.rule_variant {
            ir::RuleVariant::Enforced => {
                todo!()
            }
            ir::RuleVariant::Monitored => todo!(),
            ir::RuleVariant::Chased => todo!(
                // TODO: clarify
                "Chased rules produce a materialized view; how are they different from a materialized view defined in the table section?"
            ),
        }
    }
    fn rule(&mut self, rule: &Rule) -> Result<(), SyntaxError> {
        todo!()
    }
    fn prop(&mut self, prop: &Prop) -> Result<(), SyntaxError> {
        match prop {
            Prop::Atom { atom } => todo!(),
            Prop::Eq { left, right } => todo!(),
        }
    }
    fn atom(&mut self, atom: &Atom) -> Result<(), SyntaxError> {
        todo!()
    }
    fn term(&mut self, term: &Term) -> Result<(), SyntaxError> {
        todo!()
    }
}
