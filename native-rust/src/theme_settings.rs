//! Span-based JSONC edits preserve comments and unrelated settings.
use crate::theme::{self, Palette};
use anyhow::{Result, bail, ensure};
use serde_json::Value;
#[derive(Clone)]
struct Node {
    start: usize,
    end: usize,
    kind: Kind,
}
#[derive(Clone)]
enum Kind {
    Object(Vec<(String, Node)>),
    Array(Vec<Node>),
    Scalar(Value),
}
struct Parser<'a> {
    text: &'a str,
    pos: usize,
}
impl Parser<'_> {
    fn skip(&mut self) -> Result<()> {
        loop {
            while self
                .text
                .as_bytes()
                .get(self.pos)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.pos += 1;
            }
            let rest = &self.text[self.pos..];
            if rest.starts_with("//") {
                self.pos += rest.find('\n').unwrap_or(rest.len());
            } else if rest.starts_with("/*") {
                self.pos += rest
                    .find("*/")
                    .ok_or_else(|| anyhow::anyhow!("Unclosed JSON comment"))?
                    + 2;
            } else {
                return Ok(());
            }
        }
    }
    fn string(&mut self) -> Result<String> {
        let start = self.pos;
        ensure!(
            self.text.as_bytes().get(start) == Some(&b'"'),
            "Expected JSON string"
        );
        self.pos += 1;
        while let Some(&b) = self.text.as_bytes().get(self.pos) {
            self.pos += 1;
            if b == b'\\' {
                ensure!(self.pos < self.text.len(), "Truncated string");
                self.pos += 1;
            } else if b == b'"' {
                return Ok(serde_json::from_str(&self.text[start..self.pos])?);
            }
        }
        bail!("Unclosed JSON string")
    }
    fn node(&mut self, depth: usize) -> Result<Node> {
        ensure!(depth < 128, "Settings nesting exceeds limit");
        self.skip()?;
        let start = self.pos;
        let b = *self
            .text
            .as_bytes()
            .get(self.pos)
            .ok_or_else(|| anyhow::anyhow!("Expected JSON value"))?;
        let kind = match b {
            b'{' | b'[' => {
                self.pos += 1;
                let close = if b == b'{' { b'}' } else { b']' };
                let mut members = vec![];
                let mut elements = vec![];
                loop {
                    self.skip()?;
                    if self.text.as_bytes().get(self.pos) == Some(&close) {
                        self.pos += 1;
                        break;
                    }
                    if b == b'{' {
                        let key = self.string()?;
                        self.skip()?;
                        ensure!(
                            self.text.as_bytes().get(self.pos) == Some(&b':'),
                            "Expected colon"
                        );
                        self.pos += 1;
                        members.push((key, self.node(depth + 1)?));
                    } else {
                        elements.push(self.node(depth + 1)?);
                    }
                    self.skip()?;
                    match self.text.as_bytes().get(self.pos) {
                        Some(v) if *v == close => {
                            self.pos += 1;
                            break;
                        }
                        Some(b',') => self.pos += 1,
                        _ => bail!("Expected comma or closing delimiter"),
                    }
                }
                if b == b'{' {
                    Kind::Object(members)
                } else {
                    Kind::Array(elements)
                }
            }
            b'"' => Kind::Scalar(Value::String(self.string()?)),
            _ => {
                while self
                    .text
                    .as_bytes()
                    .get(self.pos)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !b",]} /".contains(b))
                {
                    self.pos += 1;
                }
                Kind::Scalar(serde_json::from_str(&self.text[start..self.pos])?)
            }
        };
        Ok(Node {
            start,
            end: self.pos,
            kind,
        })
    }
}
impl Node {
    fn child(&self, key: &str) -> Option<&Node> {
        match &self.kind {
            Kind::Object(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            Kind::Array(a) => key.parse::<usize>().ok().and_then(|i| a.get(i)),
            _ => None,
        }
    }
    fn at(&self, path: &[&str]) -> Option<&Node> {
        let mut n = self;
        for key in path {
            n = n.child(key)?;
        }
        Some(n)
    }
    fn string_value(&self) -> Option<&str> {
        if let Kind::Scalar(v) = &self.kind {
            v.as_str()
        } else {
            None
        }
    }
}
struct Document(String);
impl Document {
    fn parse(&self) -> Result<Node> {
        let mut parser = Parser {
            text: &self.0,
            pos: 0,
        };
        let n = parser.node(0)?;
        parser.skip()?;
        ensure!(parser.pos == self.0.len(), "Unexpected data after settings");
        ensure!(
            matches!(n.kind, Kind::Object(_)),
            "settings root must be an object"
        );
        Ok(n)
    }
    fn new(text: &str) -> Result<Self> {
        let d = Self(text.trim_start_matches('\u{feff}').to_owned());
        d.parse()?;
        Ok(d)
    }
    fn object(&mut self, path: &[&str]) -> Result<()> {
        for len in 1..=path.len() {
            let root = self.parse()?;
            if let Some(n) = root.at(&path[..len]) {
                ensure!(
                    matches!(n.kind, Kind::Object(_)),
                    "{} must be an object",
                    path[len - 1]
                );
            } else {
                self.set(&path[..len], &serde_json::json!({}))?;
            }
        }
        Ok(())
    }
    fn set(&mut self, path: &[&str], value: &Value) -> Result<()> {
        ensure!(!path.is_empty(), "Cannot replace settings root");
        let root = self.parse()?;
        let parent = root
            .at(&path[..path.len() - 1])
            .ok_or_else(|| anyhow::anyhow!("Missing parent object"))?;
        let key = path[path.len() - 1];
        let text = serde_json::to_string_pretty(value)?;
        if let Some(old) = parent.child(key) {
            self.0.replace_range(old.start..old.end, &text);
        } else {
            ensure!(matches!(parent.kind, Kind::Object(_)), "Expected object");
            self.insert(parent, &format!("{}: {text}", serde_json::to_string(key)?))?;
        }
        Ok(())
    }
    fn insert(&mut self, node: &Node, text: &str) -> Result<()> {
        let last = match &node.kind {
            Kind::Object(m) => m.last().map(|(_, n)| n.end),
            Kind::Array(a) => a.last().map(|n| n.end),
            _ => bail!("Not a container"),
        };
        let mut added = 0;
        if let Some(end) = last {
            let mut parser = Parser {
                text: &self.0,
                pos: end,
            };
            parser.skip()?;
            if self.0.as_bytes().get(parser.pos) != Some(&b',') {
                self.0.insert(end, ',');
                added = 1;
            }
        }
        self.0
            .insert_str(node.end - 1 + added, &format!("\n{text}\n"));
        Ok(())
    }
}
pub fn terminal_settings(text: &str, p: &Palette) -> Result<String> {
    let mut d = Document::new(text)?;
    if d.parse()?.child("schemes").is_none() {
        d.set(&["schemes"], &serde_json::json!([]))?;
    }
    let root = d.parse()?;
    let schemes = root.child("schemes").unwrap();
    let Kind::Array(array) = &schemes.kind else {
        bail!("schemes must be an array");
    };
    let matches: Vec<_> = array
        .iter()
        .enumerate()
        .filter(|(_, v)| v.child("name").and_then(Node::string_value) == Some(theme::NAME))
        .map(|(i, _)| i)
        .collect();
    let scheme = theme::terminal(p);
    if matches.is_empty() {
        d.insert(schemes, &serde_json::to_string_pretty(&scheme)?)?;
    } else {
        for i in matches {
            d.set(&["schemes", &i.to_string()], &scheme)?;
        }
    }
    d.object(&["profiles", "defaults"])?;
    d.set(
        &["profiles", "defaults", "colorScheme"],
        &Value::String(theme::NAME.into()),
    )?;
    if let Some(Node {
        kind: Kind::Array(list),
        ..
    }) = d.parse()?.at(&["profiles", "list"])
    {
        let indices: Vec<_> = list
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.kind, Kind::Object(_)))
            .map(|(i, _)| i)
            .collect();
        for i in indices {
            d.set(
                &["profiles", "list", &i.to_string(), "colorScheme"],
                &Value::String(theme::NAME.into()),
            )?;
        }
    }
    d.parse()?;
    Ok(d.0)
}
pub fn vscode_settings(text: &str, p: &Palette) -> Result<String> {
    let mut d = Document::new(text)?;
    for (key, values) in [
        ("workbench.colorCustomizations", theme::code_colors(p)),
        ("editor.tokenColorCustomizations", theme::code_tokens(p)),
    ] {
        d.object(&[key])?;
        for (name, value) in values.as_object().unwrap() {
            d.set(&[key, name], value)?;
        }
    }
    d.parse()?;
    Ok(d.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn value(n: &Node) -> Value {
        match &n.kind {
            Kind::Object(m) => {
                Value::Object(m.iter().map(|(k, v)| (k.clone(), value(v))).collect())
            }
            Kind::Array(a) => Value::Array(a.iter().map(value).collect()),
            Kind::Scalar(v) => v.clone(),
        }
    }
    #[test]
    fn settings_match_go_reference() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../fixtures/theme-settings-go.json")).unwrap();
        for case in cases {
            let p: Palette = serde_json::from_value(case["palette"].clone()).unwrap();
            let source = case["source"].as_str().unwrap();
            for (key, result) in [
                ("terminal", terminal_settings(source, &p)),
                ("vscode", vscode_settings(source, &p)),
            ] {
                let result = result.unwrap();
                assert_eq!(
                    value(&Document::new(&result).unwrap().parse().unwrap()),
                    case[key]
                );
            }
        }
    }
    fn palette() -> Palette {
        theme::readable(&vec!["#123456".into(); 16]).unwrap()
    }
    #[test]
    fn terminal_keeps_comments_other_schemes_and_profile_fields() {
        let source = "{ // retain me\n\"defaultProfile\":\"pwsh\",\"schemes\":[{\"name\":\"Existing\",\"red\":\"#ff0000\"}],\"profiles\":{\"defaults\":{},\"list\":[{\"guid\":\"pwsh\",\"colorScheme\":\"Existing\"}]},}";
        let result = terminal_settings(source, &palette()).unwrap();
        assert!(result.contains("// retain me"));
        assert!(result.contains("{\"name\":\"Existing\",\"red\":\"#ff0000\"}"));
        let root = Document::new(&result).unwrap().parse().unwrap();
        assert_eq!(
            root.at(&["profiles", "list", "0", "guid"])
                .and_then(Node::string_value),
            Some("pwsh")
        );
        assert_eq!(
            root.at(&["profiles", "list", "0", "colorScheme"])
                .and_then(Node::string_value),
            Some(theme::NAME)
        );
        assert_eq!(terminal_settings(&result, &palette()).unwrap(), result);
    }
    #[test]
    fn vscode_preserves_comments_and_custom_keys() {
        let source = "\u{feff}{\"editor.fontSize\":15,\"workbench.colorCustomizations\":{\"custom.keep\":\"#123456\" /* keep */}, // comment\n}";
        let result = vscode_settings(source, &palette()).unwrap();
        for keep in [
            "\"editor.fontSize\":15",
            "\"custom.keep\":\"#123456\"",
            "/* keep */",
            "// comment",
        ] {
            assert!(result.contains(keep));
        }
        assert_eq!(vscode_settings(&result, &palette()).unwrap(), result);
    }
    #[test]
    fn malformed_settings_are_rejected() {
        for source in [
            "[]",
            "{broken",
            "{\"x\":1 2}",
            "{ /* unclosed",
            "{\"profiles\":null}",
            "{\"schemes\":{}}",
        ] {
            assert!(terminal_settings(source, &palette()).is_err(), "{source}");
        }
    }
    #[test]
    fn insertion_after_line_comments_and_trailing_commas() {
        for source in [
            "{\"a\":1 // tail\n}",
            "{\"a\":1, // tail\n}",
            "{/* comment */}",
        ] {
            let result = terminal_settings(source, &palette()).unwrap();
            assert_eq!(terminal_settings(&result, &palette()).unwrap(), result);
        }
    }
}
