use anyhow::{Result, ensure};

pub fn focused_border(source: &str, accent: &str) -> Result<String> {
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let mut stack: Vec<(usize, String)> = vec![];
    let mut changed = 0;
    for line in &mut lines {
        let original = line.clone();
        let trimmed = original.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let depth = original.len() - original.trim_start_matches(' ').len();
        while stack.last().is_some_and(|(d, _)| depth <= *d) {
            stack.pop();
        }
        if stack
            .iter()
            .map(|(_, s)| s.as_str())
            .eq(["window_effects", "focused_window", "border"])
            && trimmed.starts_with("color:")
        {
            let colon = original.find(':').unwrap();
            let tail = &original[colon + 1..];
            let start = colon + 1 + tail.len() - tail.trim_start().len();
            let value = &original[start..];
            let end = match value.as_bytes().first() {
                Some(b'\'') | Some(b'"') => {
                    let quote = value.as_bytes()[0] as char;
                    value[1..].find(quote).map(|n| n + 2)
                }
                _ => Some(
                    value
                        .find(|c: char| c.is_whitespace() || c == '#')
                        .unwrap_or(value.len()),
                ),
            }
            .filter(|n| *n > 0)
            .ok_or_else(|| anyhow::anyhow!("unsupported focused border color"))?;
            *line = format!(
                "{}'{accent}'{}",
                &original[..start],
                &original[start + end..]
            );
            changed += 1;
        }
        if let Some((key, rest)) = trimmed.split_once(':') {
            if !key.is_empty() && rest.split('#').next().unwrap_or("").trim().is_empty() {
                stack.push((depth, key.into()));
            }
        }
    }
    ensure!(
        changed == 1,
        "expected one GlazeWM focused border color, got {changed}"
    );
    Ok(lines.join("\n"))
}

pub fn codex_settings(source: &str) -> String {
    let source = source.trim_start_matches('\u{feff}');
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let theme = "theme = \"glazebar-aether\"";
    let mut lines: Vec<String> = source.split_inclusive('\n').map(str::to_owned).collect();
    let section = |s: &str| s.split('#').next().unwrap_or("").trim().to_owned();
    let Some(start) = lines.iter().position(|s| section(s) == "[tui]") else {
        return format!(
            "{}{newline}{newline}[tui]{newline}{theme}{newline}",
            source.trim_end_matches(['\r', '\n'])
        );
    };
    let end = (start + 1..lines.len())
        .find(|i| {
            let s = section(&lines[*i]);
            s.starts_with('[') && s.ends_with(']')
        })
        .unwrap_or(lines.len());
    let mut found = false;
    for line in &mut lines[start + 1..end] {
        if line
            .trim_start_matches([' ', '\t'])
            .strip_prefix("theme")
            .is_some_and(|s| s.trim_start_matches([' ', '\t']).starts_with('='))
        {
            let ending = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            *line = format!("{theme}{ending}");
            found = true;
        }
    }
    if !found {
        if !lines[start].ends_with('\n') {
            lines[start].push_str(newline);
        }
        lines.insert(start + 1, format!("{theme}{newline}"));
    }
    lines.concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focused_border_preserves_unrelated_yaml_exactly() {
        let source = "# keep\r\nwindow_effects:\r\n  focused_window:\r\n    border:\r\n      enabled: true\r\n      color: '#8dbcff' # focus\r\n  other_windows:\r\n    border:\r\n      color: '#a1a1a1'\r\n";
        assert_eq!(
            focused_border(source, "#123456").unwrap(),
            source.replacen("'#8dbcff'", "'#123456'", 1)
        );
        assert!(focused_border("general: {}", "#123456").is_err());
        assert!(focused_border(&format!("{source}{source}"), "#123456").is_err());
    }
    #[test]
    fn codex_preserves_sections_and_line_endings() {
        for source in [
            "model = \"example\"\n[tui]\nstatus_line = []\ntheme = \"old\"\n[windows]\nsandbox = \"elevated\"\n",
            "[tui]\nscreen_reader = true\n[features]\na = true\n",
            "model = \"example\"\r\n",
            "[tui]",
            "[tui]\r\nother = true\r\n",
        ] {
            let result = codex_settings(source);
            assert_eq!(result.matches("theme = \"glazebar-aether\"").count(), 1);
            assert_eq!(codex_settings(&result), result);
            assert!(!result.contains("\r\r"));
            if source.contains("[windows]") {
                assert!(result.ends_with("[windows]\nsandbox = \"elevated\"\n"));
            }
        }
    }
}
