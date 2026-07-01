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

pub(crate) fn parse_meta_command(input: &str) -> Result<Command, String> {
    let parts = shlex::split(input).ok_or_else(|| "could not parse input".to_string())?;
    let Some(command) = parts.first() else {
        return Err("empty command".to_string());
    };

    match command.as_str() {
        ".help" => {
            if parts.len() == 1 {
                Ok(Command::Help)
            } else {
                Err("usage: .help".to_string())
            }
        }
        ".exit" | ".quit" => {
            if parts.len() == 1 {
                Ok(Command::Exit)
            } else {
                Err(format!("usage: {command}"))
            }
        }
        ".load" => match parts.as_slice() {
            [_, path] => Ok(Command::Load { path: path.clone() }),
            _ => Err("usage: .load <schema-json-path>".to_string()),
        },
        ".open" => match parts.as_slice() {
            [_, path] => Ok(Command::Open { path: path.clone() }),
            _ => Err("usage: .open <store-path>".to_string()),
        },
        ".save" => match parts.as_slice() {
            [_, path] => Ok(Command::Save { path: path.clone() }),
            _ => Err("usage: .save <store-path>".to_string()),
        },
        ".tables" => {
            if parts.len() == 1 {
                Ok(Command::Tables)
            } else {
                Err("usage: .tables".to_string())
            }
        }
        ".rules" => {
            if parts.len() == 1 {
                Ok(Command::Rules)
            } else {
                Err("usage: .rules".to_string())
            }
        }
        ".schema" => match parts.as_slice() {
            [_] => Ok(Command::Schema { table: None }),
            [_, table] => Ok(Command::Schema {
                table: Some(table.clone()),
            }),
            _ => Err("usage: .schema [table]".to_string()),
        },
        ".dump" => match parts.as_slice() {
            [_, table] => Ok(Command::Dump {
                table: table.clone(),
            }),
            _ => Err("usage: .dump <table>".to_string()),
        },
        _ => Err(format!(
            "unknown meta command: {command}. Type `.help` for commands."
        )),
    }
}
