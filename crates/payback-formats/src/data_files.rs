//! The selection rules of `crates/payback/data_files.txt`: which files of
//! the Amiga data root and the Mac `Payback` folder the port reads. Shared
//! by `crates/payback/build.rs` (staging next to the executable) and
//! `payback-extract` (extracting next to it); `tools/dist/stage_data.py`
//! and `tools/web/pack_web_data.py` read the same file.

/// The rules, names lower-cased.
#[derive(Default, Debug)]
pub struct Rules {
    skip_dir: Vec<String>,
    skip_file: Vec<String>,
    skip_suffix: Vec<String>,
    skip_prefix: Vec<String>,
}

impl Rules {
    /// Parses `data_files.txt`; an unknown rule kind is an error.
    pub fn parse(text: &str) -> Result<Rules, String> {
        let mut r = Rules::default();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let (kind, name) = line.split_once(' ').unwrap_or((line, ""));
            let name = name.trim().to_lowercase();
            match kind {
                "skip-dir" => r.skip_dir.push(name),
                "skip-file" => r.skip_file.push(name),
                "skip-suffix" => r.skip_suffix.push(name),
                "skip-prefix" => r.skip_prefix.push(name),
                "film" => {}
                _ => return Err(format!("data_files.txt: bad rule {line:?}")),
            }
        }
        Ok(r)
    }

    /// A directory of this name is left out with everything in it.
    pub fn skip_dir(&self, name: &str) -> bool {
        self.skip_dir.contains(&name.to_lowercase())
    }

    /// A file of this name is left out.
    pub fn skip_file(&self, name: &str) -> bool {
        let low = name.to_lowercase();
        self.skip_file.contains(&low)
            || self.skip_suffix.iter().any(|s| low.ends_with(s.as_str()))
            || self.skip_prefix.iter().any(|p| low.starts_with(p.as_str()))
    }

    /// The file at the `/`-separated `rel` path is selected: no folder on
    /// the way is skipped, nor the file itself.
    pub fn keep(&self, rel: &str) -> bool {
        let mut parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
        let Some(file) = parts.pop() else { return false };
        !parts.iter().any(|d| self.skip_dir(d)) && !self.skip_file(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_select_like_the_staging() {
        let r = Rules::parse("# c\nskip-dir docs\nskip-file payback (os 9)\nskip-suffix .rsrc\nskip-prefix .fbc\nfilm x.mpg\n")
            .unwrap();
        assert!(r.keep("Maps/00.map"));
        assert!(!r.keep("Docs/readme"));
        assert!(!r.keep("a/DOCS/b/c"));
        assert!(!r.keep("Payback (OS 9)"));
        assert!(!r.keep("Payback (OS X).rsrc"));
        assert!(!r.keep(".FBCIndex"));
        assert!(r.keep("x.mpg"));
        assert!(Rules::parse("nonsense x").is_err());
    }
}
