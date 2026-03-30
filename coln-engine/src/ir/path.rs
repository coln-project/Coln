use std::{convert::Infallible, ops::Deref, str::FromStr};

use crate::ir::{Path, QName};

impl Deref for Path {
    type Target = Vec<QName>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Dotted textual form: each `.` separates one [`QName`]; within each dot segment, `/`
/// separates parts of that [`QName`] (e.g. `"G.V"` → `[["G"], ["V"]]`, `"G.A/B"` →
/// `[["G"], ["A", "B"]]`). Dot segments and `/` parts are trimmed; empty pieces are
/// skipped. An empty or whitespace-only string yields an empty path.
///
/// Use [`Path::from`] on `&str` / [`String`], or [`str::parse`].
fn parse_path_from_str(s: &str) -> Path {
    Path(
        s.split('.')
            .map(str::trim)
            .filter(|seg| !seg.is_empty())
            .filter_map(|seg| {
                let q = qname_from_slash_segment(seg);
                (!q.is_empty()).then_some(q)
            })
            .collect(),
    )
}

impl From<&str> for Path {
    fn from(s: &str) -> Self {
        parse_path_from_str(s)
    }
}

impl From<String> for Path {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl FromStr for Path {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(parse_path_from_str(s))
    }
}

fn qname_from_slash_segment(seg: &str) -> QName {
    seg.split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

#[cfg(test)]
mod path_parse_tests {
    use super::*;

    #[test]
    fn g_dot_v() {
        assert_eq!(
            Path::from("G.V"),
            Path(vec![vec!["G".to_string()], vec!["V".to_string()]])
        );
    }

    #[test]
    fn triple_segment() {
        assert_eq!(
            Path::from("Hom.E.foreignKeys"),
            Path(vec![
                vec!["Hom".to_string()],
                vec!["E".to_string()],
                vec!["foreignKeys".to_string()],
            ])
        );
    }

    #[test]
    fn dot_separated_path_slash_separated_qname() {
        assert_eq!(
            Path::from("G.A/B"),
            Path(vec![
                vec!["G".to_string()],
                vec!["A".to_string(), "B".to_string()]
            ])
        );
    }

    #[test]
    fn slash_only_in_one_segment() {
        assert_eq!(
            Path::from("Hom.E/D.foreignKeys"),
            Path(vec![
                vec!["Hom".to_string()],
                vec!["E".to_string(), "D".to_string()],
                vec!["foreignKeys".to_string()],
            ])
        );
    }

    #[test]
    fn qname_with_slashes_no_dots() {
        assert_eq!(
            Path::from("A/B/C"),
            Path(vec![vec![
                "A".to_string(),
                "B".to_string(),
                "C".to_string()
            ]])
        );
    }

    #[test]
    fn from_string() {
        assert_eq!(Path::from("G.V".to_string()), Path::from("G.V"));
    }

    #[test]
    fn from_str_parse() {
        let p: Path = "G.V".parse().unwrap();
        assert_eq!(p, Path::from("G.V"));
    }
}
