//! `Phrases/<level>.<Language>.phrases` — mission dialogue text.
//!
//! ```text
//! # comment
//! <number> = "<text>"
//! ```
//!
//! `<...>` inside the text marks tutorial hints. The file name's first two
//! digits are the level script number (`00`, `01`, `10`, ...).

use crate::{Error, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Phrases {
    pub by_number: BTreeMap<u32, String>,
}

impl Phrases {
    /// Lenient, because the shipped translations are irregular: quoted text
    /// may span lines, some entries lack the opening or closing quote, and
    /// anything after the closing quote (e.g. `# changed`) is ignored.
    pub fn parse(data: &[u8]) -> Result<Phrases> {
        let text: String = data.iter().map(|&b| b as char).collect();
        let text = text.replace('\r', "");
        let mut by_number = BTreeMap::new();
        let mut rest = text.as_str();
        let mut line_no = 1;
        while !rest.is_empty() {
            let (line, tail) = rest.split_once('\n').unwrap_or((rest, ""));
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                rest = tail;
                line_no += 1;
                continue;
            }
            let err = |detail: &str| Error::Parse {
                what: "phrases",
                detail: format!("line {line_no}: {detail}"),
            };
            let (num, after_eq) = line.split_once('=').ok_or_else(|| err("missing '='"))?;
            let num = num.trim().parse().map_err(|_| err("bad phrase number"))?;
            let after_eq = after_eq.trim_start();

            let body;
            if let Some(quoted) = after_eq.strip_prefix('"') {
                // Find the closing quote in the remainder of the file.
                let start = text.len() - rest.len() + (line.len() - quoted.len());
                let all = &text[start..];
                let end = all.find('"').unwrap_or(all.find('\n').unwrap_or(all.len()));
                body = &all[..end];
                let consumed = &text[start + end..];
                // Resume after the line holding the closing quote.
                rest = consumed.split_once('\n').map_or("", |(_, t)| t);
                line_no += 1 + body.matches('\n').count();
            } else {
                body = after_eq.trim_end().trim_end_matches('"');
                rest = tail;
                line_no += 1;
            }
            by_number.insert(num, body.to_string());
        }
        Ok(Phrases { by_number })
    }

    pub fn get(&self, n: u32) -> Option<&str> {
        self.by_number.get(&n).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let p = Phrases::parse(b"# c\n\n3 = \"Hi <there>\"\n10 = \"x\"\n").unwrap();
        assert_eq!(p.get(3), Some("Hi <there>"));
        assert_eq!(p.get(10), Some("x"));
        assert_eq!(p.by_number.len(), 2);
    }

    #[test]
    fn tolerates_translation_quirks() {
        let src = b"1 = \"a\" # changed\r\n2 = \"multi\nline\"\n3 = no open quote\"\n4 = \"x\".\n";
        let p = Phrases::parse(src).unwrap();
        assert_eq!(p.get(1), Some("a"));
        assert_eq!(p.get(2), Some("multi\nline"));
        assert_eq!(p.get(3), Some("no open quote"));
        assert_eq!(p.get(4), Some("x"));
    }
}
