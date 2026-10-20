use crate::theme::{self, Palette};
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

pub fn rio_theme(p: &Palette) -> String {
    format!(
        "[colors]\n\
background = '{}'\n\
foreground = '{}'\n\
selection-background = '{}'\n\
selection-foreground = '{}'\n\
cursor = '{}'\n\
tabs = '{}'\n\
tabs-active = '{}'\n\
black = '{}'\n\
red = '{}'\n\
green = '{}'\n\
yellow = '{}'\n\
blue = '{}'\n\
magenta = '{}'\n\
cyan = '{}'\n\
white = '{}'\n\
light_black = '{}'\n\
light_red = '{}'\n\
light_green = '{}'\n\
light_yellow = '{}'\n\
light_blue = '{}'\n\
light_magenta = '{}'\n\
light_cyan = '{}'\n\
light_white = '{}'\n",
        p[0],
        p[15],
        p[8],
        p[15],
        p[6],
        p[0],
        p[8],
        p[0],
        p[1],
        p[2],
        p[3],
        p[4],
        p[5],
        p[6],
        p[7],
        p[8],
        p[9],
        p[10],
        p[11],
        p[12],
        p[13],
        p[14],
        p[15]
    )
}

pub fn rio_settings(source: &str, theme_name: &str) -> String {
    let source = source.trim_start_matches('\u{feff}');
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let entry = format!("theme = \"{theme_name}\"");
    if source.trim().is_empty() {
        return format!("{entry}{newline}");
    }
    let mut lines: Vec<String> = source.split_inclusive('\n').map(str::to_owned).collect();
    let section = |s: &str| s.split('#').next().unwrap_or("").trim().to_owned();
    let first_section = lines
        .iter()
        .position(|s| {
            let sec = section(s);
            sec.starts_with('[') && sec.ends_with(']')
        })
        .unwrap_or(lines.len());
    let mut found = false;
    for line in &mut lines[..first_section] {
        let code = line.split('#').next().unwrap_or("");
        if code
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
            *line = format!("{entry}{ending}");
            found = true;
        }
    }
    if !found {
        lines.insert(0, format!("{entry}{newline}"));
    }
    lines.concat()
}

const FF_START: &str = "/* GLAZEBAR-THEME-START */";
const FF_END: &str = "/* GLAZEBAR-THEME-END */";

fn replace_delimited_block(source: &str, block: &str) -> String {
    let source = source.trim_start_matches('\u{feff}');
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let formatted_block = if newline == "\r\n" {
        block.replace('\n', "\r\n")
    } else {
        block.to_owned()
    };
    if let (Some(start), Some(end)) = (source.find(FF_START), source.find(FF_END)) {
        if start <= end {
            let mut tail_idx = end + FF_END.len();
            if source[tail_idx..].starts_with("\r\n") {
                tail_idx += 2;
            } else if source[tail_idx..].starts_with('\n') {
                tail_idx += 1;
            }
            return format!(
                "{}{}{}",
                &source[..start],
                formatted_block,
                &source[tail_idx..]
            );
        }
    }
    if source.trim().is_empty() {
        formatted_block
    } else {
        format!(
            "{}{newline}{newline}{}",
            source.trim_end_matches(['\r', '\n']),
            formatted_block
        )
    }
}

pub fn firefox_user_chrome(source: &str, p: &Palette, light: bool) -> String {
    let acc = theme::accent(p, light);
    let block = format!(
        "{FF_START}\n\
:root {{\n\
  --lwt-accent-color: {bg} !important;\n\
  --lwt-text-color: {fg} !important;\n\
  --toolbar-bgcolor: {bg} !important;\n\
  --toolbar-color: {fg} !important;\n\
  --toolbar-field-background-color: {bg} !important;\n\
  --toolbar-field-color: {fg} !important;\n\
  --toolbar-field-focus-background-color: {bg} !important;\n\
  --toolbar-field-focus-color: {fg} !important;\n\
  --toolbar-field-border-focus: {acc} !important;\n\
  --urlbar-box-bgcolor: {bg} !important;\n\
  --urlbar-box-text-color: {fg} !important;\n\
  --tab-selected-bgcolor: {bg} !important;\n\
  --tab-selected-textcolor: {fg} !important;\n\
  --tab-line-color: {acc} !important;\n\
  --focus-outline-color: {acc} !important;\n\
}}\n\
{FF_END}\n",
        bg = p[0],
        fg = p[15],
        acc = acc
    );
    replace_delimited_block(source, &block)
}

pub fn firefox_user_js(source: &str, light: bool) -> String {
    let scheme_override = if light { 1 } else { 0 };
    let dark_theme = if light { 0 } else { 1 };
    let block = format!(
        "{FF_START}\n\
user_pref(\"toolkit.legacyUserProfileCustomizations.stylesheets\", true);\n\
user_pref(\"layout.css.prefers-color-scheme.content-override\", {scheme_override});\n\
user_pref(\"ui.systemUsesDarkTheme\", {dark_theme});\n\
user_pref(\"browser.theme.content-theme\", {scheme_override});\n\
user_pref(\"browser.theme.toolbar-theme\", {scheme_override});\n\
{FF_END}\n"
    );
    replace_delimited_block(source, &block)
}

pub fn glazewm_set_scalar(source: &str, section_path: &[&str], key: &str, new_val: &str) -> String {
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let mut stack: Vec<(usize, String)> = vec![];
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
        let prefix = format!("{key}:");
        if stack
            .iter()
            .map(|(_, s)| s.as_str())
            .eq(section_path.iter().copied())
            && trimmed.starts_with(&prefix)
        {
            let indent = &original[..depth];
            let comment = original
                .find('#')
                .map(|i| format!(" {}", &original[i..].trim_end_matches('\r')))
                .unwrap_or_default();
            let cr = if original.ends_with('\r') { "\r" } else { "" };
            *line = format!("{indent}{key}: {new_val}{comment}{cr}");
        }
        if let Some((k, rest)) = trimmed.split_once(':') {
            if !k.is_empty() && rest.split('#').next().unwrap_or("").trim().is_empty() {
                stack.push((depth, k.into()));
            }
        }
    }
    lines.join("\n")
}

pub fn glazewm_set_binding_command(source: &str, binding_marker: &str, new_cmd: &str) -> String {
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    for i in 1..lines.len() {
        if lines[i].contains(binding_marker) {
            // Look backwards for `- commands:` within the same keybinding item
            for j in (i.saturating_sub(3)..i).rev() {
                let trimmed = lines[j].trim();
                if trimmed.starts_with("- commands:") {
                    let depth = lines[j].len() - lines[j].trim_start_matches(' ').len();
                    let indent = &lines[j][..depth];
                    let cr = if lines[j].ends_with('\r') { "\r" } else { "" };
                    lines[j] = format!("{indent}- commands: ['{new_cmd}']{cr}");
                    break;
                }
            }
        }
    }
    lines.join("\n")
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
    #[test]
    fn rio_settings_updates_top_level_theme_and_preserves_comments() {
        let source = "# Rio config\ntheme = \"everforest_dark\"\n# theme = \"rose-pine-dawn\"\nconfirm-before-quit = true\n\n[renderer]\nperformance = \"High\"\n";
        let updated = rio_settings(source, "glazebar-aether");
        assert!(updated.contains("theme = \"glazebar-aether\""));
        assert!(updated.contains("# theme = \"rose-pine-dawn\""));
        assert!(updated.contains("[renderer]\nperformance = \"High\""));
        assert_eq!(rio_settings(&updated, "glazebar-aether"), updated);
    }
    #[test]
    fn firefox_blocks_are_idempotent_and_preserve_user_content() {
        let p: Palette = std::array::from_fn(|_| "#123456".into());
        let existing_css = "/* user custom css */\n#nav-bar { background: transparent; }\n";
        let css1 = firefox_user_chrome(existing_css, &p, false);
        assert!(css1.starts_with("/* user custom css */"));
        assert!(css1.contains("--toolbar-bgcolor: #123456 !important;"));
        let css2 = firefox_user_chrome(&css1, &p, true);
        assert_eq!(css2.matches(FF_START).count(), 1);

        let js1 = firefox_user_js("user_pref(\"existing\", true);\n", false);
        assert!(js1.contains("user_pref(\"existing\", true);"));
        assert!(js1.contains("user_pref(\"ui.systemUsesDarkTheme\", 1);"));
        let js2 = firefox_user_js(&js1, true);
        assert_eq!(js2.matches(FF_START).count(), 1);
        assert!(js2.contains("user_pref(\"ui.systemUsesDarkTheme\", 0);"));
    }
}
