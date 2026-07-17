// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    Load { path: String },
    Open { path: String },
    Schema { table: Option<String> },
    Tables,
    Rules,
    Exit,
    Dump { table: String },
    Save { path: String },
}

pub(crate) fn parse_meta_command(input: &str) -> anyhow::Result<Command> {
    let parts = shlex::split(input).ok_or_else(|| anyhow::anyhow!("could not parse input"))?;
    let Some(command) = parts.first() else {
        anyhow::bail!("empty command");
    };

    match command.as_str() {
        ".help" => {
            if parts.len() == 1 {
                Ok(Command::Help)
            } else {
                anyhow::bail!("usage: .help")
            }
        }
        ".exit" | ".quit" => {
            if parts.len() == 1 {
                Ok(Command::Exit)
            } else {
                anyhow::bail!("usage: {command}")
            }
        }
        ".load" => match parts.as_slice() {
            [_, path] => Ok(Command::Load { path: path.clone() }),
            _ => anyhow::bail!("usage: .load <schema-json-path>"),
        },
        ".open" => match parts.as_slice() {
            [_, path] => Ok(Command::Open { path: path.clone() }),
            _ => anyhow::bail!("usage: .open <store-path>"),
        },
        ".save" => match parts.as_slice() {
            [_, path] => Ok(Command::Save { path: path.clone() }),
            _ => anyhow::bail!("usage: .save <store-path>"),
        },
        ".tables" => {
            if parts.len() == 1 {
                Ok(Command::Tables)
            } else {
                anyhow::bail!("usage: .tables")
            }
        }
        ".rules" => {
            if parts.len() == 1 {
                Ok(Command::Rules)
            } else {
                anyhow::bail!("usage: .rules")
            }
        }
        ".schema" => match parts.as_slice() {
            [_] => Ok(Command::Schema { table: None }),
            [_, table] => Ok(Command::Schema {
                table: Some(table.clone()),
            }),
            _ => anyhow::bail!("usage: .schema [table]"),
        },
        ".dump" => match parts.as_slice() {
            [_, table] => Ok(Command::Dump {
                table: table.clone(),
            }),
            _ => anyhow::bail!("usage: .dump <table>"),
        },
        _ => anyhow::bail!("unknown meta command: {command}. Type `.help` for commands."),
    }
}
