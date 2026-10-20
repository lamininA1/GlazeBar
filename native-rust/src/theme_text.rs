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
const TOML_START: &str = "# GLAZEBAR-THEME-START";
const TOML_END: &str = "# GLAZEBAR-THEME-END";
const LUA_START: &str = "-- GLAZEBAR-THEME-START";
const LUA_END: &str = "-- GLAZEBAR-THEME-END";
const JS_START: &str = "// GLAZEBAR-THEME-START";
const JS_END: &str = "// GLAZEBAR-THEME-END";

fn replace_delimited_block_with(
    source: &str,
    start_marker: &str,
    end_marker: &str,
    block: &str,
) -> String {
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
    if let (Some(start), Some(end)) = (source.find(start_marker), source.find(end_marker)) {
        if start <= end {
            let mut tail_idx = end + end_marker.len();
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

fn replace_delimited_block(source: &str, block: &str) -> String {
    replace_delimited_block_with(source, FF_START, FF_END, block)
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

pub fn wezterm_theme(p: &Palette) -> String {
    format!(
        "# Generated by GlazeBar\n\
[metadata]\n\
name = \"{name}\"\n\
author = \"GlazeBar\"\n\
\n\
[colors]\n\
foreground = \"{fg}\"\n\
background = \"{bg}\"\n\
cursor_bg = \"{fg}\"\n\
cursor_fg = \"{bg}\"\n\
cursor_border = \"{fg}\"\n\
selection_fg = \"{fg}\"\n\
selection_bg = \"{sel}\"\n\
ansi = [\n\
  \"{c0}\",\n\
  \"{c1}\",\n\
  \"{c2}\",\n\
  \"{c3}\",\n\
  \"{c4}\",\n\
  \"{c5}\",\n\
  \"{c6}\",\n\
  \"{c7}\",\n\
]\n\
brights = [\n\
  \"{c8}\",\n\
  \"{c9}\",\n\
  \"{c10}\",\n\
  \"{c11}\",\n\
  \"{c12}\",\n\
  \"{c13}\",\n\
  \"{c14}\",\n\
  \"{c15}\",\n\
]\n",
        name = theme::NAME,
        bg = p[0],
        fg = p[15],
        sel = p[8],
        c0 = p[0],
        c1 = p[1],
        c2 = p[2],
        c3 = p[3],
        c4 = p[4],
        c5 = p[5],
        c6 = p[6],
        c7 = p[7],
        c8 = p[8],
        c9 = p[9],
        c10 = p[10],
        c11 = p[11],
        c12 = p[12],
        c13 = p[13],
        c14 = p[14],
        c15 = p[15],
    )
}

pub fn wezterm_settings(source: &str, scheme_name: &str) -> String {
    let source = source.trim_start_matches('\u{feff}');
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if source.trim().is_empty() {
        return format!(
            "local wezterm = require 'wezterm'{newline}\
local config = wezterm.config_builder and wezterm.config_builder() or {{}}{newline}\
{LUA_START}{newline}\
config.color_scheme = \"{scheme_name}\"{newline}\
{LUA_END}{newline}\
return config{newline}"
        );
    }
    if source.contains(LUA_START) && source.contains(LUA_END) {
        let block = format!(
            "{LUA_START}\nconfig.color_scheme = \"{scheme_name}\"\n{LUA_END}\n"
        );
        return replace_delimited_block_with(source, LUA_START, LUA_END, &block);
    }
    let mut lines: Vec<String> = source.split_inclusive('\n').map(str::to_owned).collect();
    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with("--") {
            continue;
        }
        for prefix in ["config.color_scheme", "color_scheme"] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                if rest.trim_start().starts_with('=') {
                    let indent_len = line.len() - line.trim_start_matches([' ', '\t']).len();
                    let indent = &line[..indent_len];
                    let has_comma = trimmed.ends_with(',');
                    let suffix = if has_comma { "," } else { "" };
                    let nl = if line.ends_with("\r\n") {
                        "\r\n"
                    } else if line.ends_with('\n') {
                        "\n"
                    } else {
                        ""
                    };
                    *line = format!("{indent}{prefix} = \"{scheme_name}\"{suffix}{nl}");
                    return lines.concat();
                }
            }
        }
    }
    if let Some(pos) = lines
        .iter()
        .rposition(|l| l.trim_start().starts_with("return ") && !l.trim_start().starts_with("return {"))
    {
        let block = format!(
            "{LUA_START}{newline}config.color_scheme = \"{scheme_name}\"{newline}{LUA_END}{newline}"
        );
        lines.insert(pos, block);
        return lines.concat();
    }
    if let Some(pos) = lines
        .iter()
        .position(|l| l.trim_start().starts_with("return {"))
    {
        lines.insert(
            pos + 1,
            format!("  color_scheme = \"{scheme_name}\",{newline}"),
        );
        return lines.concat();
    }
    let block = format!(
        "{LUA_START}\nconfig.color_scheme = \"{scheme_name}\"\n{LUA_END}\n"
    );
    replace_delimited_block_with(source, LUA_START, LUA_END, &block)
}

pub fn alacritty_theme(p: &Palette) -> String {
    format!(
        "{TOML_START}\n\
[colors.primary]\n\
background = \"{bg}\"\n\
foreground = \"{fg}\"\n\
\n\
[colors.cursor]\n\
text = \"{bg}\"\n\
cursor = \"{fg}\"\n\
\n\
[colors.selection]\n\
text = \"{fg}\"\n\
background = \"{sel}\"\n\
\n\
[colors.normal]\n\
black = \"{c0}\"\n\
red = \"{c1}\"\n\
green = \"{c2}\"\n\
yellow = \"{c3}\"\n\
blue = \"{c4}\"\n\
magenta = \"{c5}\"\n\
cyan = \"{c6}\"\n\
white = \"{c7}\"\n\
\n\
[colors.bright]\n\
black = \"{c8}\"\n\
red = \"{c9}\"\n\
green = \"{c10}\"\n\
yellow = \"{c11}\"\n\
blue = \"{c12}\"\n\
magenta = \"{c13}\"\n\
cyan = \"{c14}\"\n\
white = \"{c15}\"\n\
{TOML_END}\n",
        bg = p[0],
        fg = p[15],
        sel = p[8],
        c0 = p[0],
        c1 = p[1],
        c2 = p[2],
        c3 = p[3],
        c4 = p[4],
        c5 = p[5],
        c6 = p[6],
        c7 = p[7],
        c8 = p[8],
        c9 = p[9],
        c10 = p[10],
        c11 = p[11],
        c12 = p[12],
        c13 = p[13],
        c14 = p[14],
        c15 = p[15],
    )
}

pub fn alacritty_settings(source: &str, p: &Palette) -> String {
    let block = alacritty_theme(p);
    replace_delimited_block_with(source, TOML_START, TOML_END, &block)
}

pub fn hyper_settings(source: &str, p: &Palette) -> String {
    let [sr, sg, sb] = theme::rgb(&p[4]);
    let block = format!(
        "{JS_START}\n\
if (typeof module !== \"undefined\" && module.exports && module.exports.config) {{\n\
  Object.assign(module.exports.config, {{\n\
    foregroundColor: \"{fg}\",\n\
    backgroundColor: \"{bg}\",\n\
    borderColor: \"{sel}\",\n\
    cursorColor: \"{fg}\",\n\
    cursorAccentColor: \"{bg}\",\n\
    selectionColor: \"rgba({sr}, {sg}, {sb}, 0.3)\",\n\
    colors: {{\n\
      black: \"{c0}\",\n\
      red: \"{c1}\",\n\
      green: \"{c2}\",\n\
      yellow: \"{c3}\",\n\
      blue: \"{c4}\",\n\
      magenta: \"{c5}\",\n\
      cyan: \"{c6}\",\n\
      white: \"{c7}\",\n\
      lightBlack: \"{c8}\",\n\
      lightRed: \"{c9}\",\n\
      lightGreen: \"{c10}\",\n\
      lightYellow: \"{c11}\",\n\
      lightBlue: \"{c12}\",\n\
      lightMagenta: \"{c13}\",\n\
      lightCyan: \"{c14}\",\n\
      lightWhite: \"{c15}\"\n\
    }}\n\
  }});\n\
}}\n\
{JS_END}\n",
        bg = p[0],
        fg = p[15],
        sel = p[8],
        c0 = p[0],
        c1 = p[1],
        c2 = p[2],
        c3 = p[3],
        c4 = p[4],
        c5 = p[5],
        c6 = p[6],
        c7 = p[7],
        c8 = p[8],
        c9 = p[9],
        c10 = p[10],
        c11 = p[11],
        c12 = p[12],
        c13 = p[13],
        c14 = p[14],
        c15 = p[15],
    );
    let base = if source.trim().is_empty() || source.trim() == "{}" {
        "module.exports = {\n  config: {},\n  plugins: []\n};\n"
    } else {
        source
    };
    replace_delimited_block_with(base, JS_START, JS_END, &block)
}

pub fn putty_colours(p: &Palette) -> [(&'static str, String); 22] {
    let rgb_str = |hex: &str| {
        let [r, g, b] = theme::rgb(hex);
        format!("{r},{g},{b}")
    };
    [
        ("Colour0", rgb_str(&p[15])),
        ("Colour1", rgb_str(&p[15])),
        ("Colour2", rgb_str(&p[0])),
        ("Colour3", rgb_str(&p[8])),
        ("Colour4", rgb_str(&p[0])),
        ("Colour5", rgb_str(&p[15])),
        ("Colour6", rgb_str(&p[0])),
        ("Colour7", rgb_str(&p[8])),
        ("Colour8", rgb_str(&p[1])),
        ("Colour9", rgb_str(&p[9])),
        ("Colour10", rgb_str(&p[2])),
        ("Colour11", rgb_str(&p[10])),
        ("Colour12", rgb_str(&p[3])),
        ("Colour13", rgb_str(&p[11])),
        ("Colour14", rgb_str(&p[4])),
        ("Colour15", rgb_str(&p[12])),
        ("Colour16", rgb_str(&p[5])),
        ("Colour17", rgb_str(&p[13])),
        ("Colour18", rgb_str(&p[6])),
        ("Colour19", rgb_str(&p[14])),
        ("Colour20", rgb_str(&p[7])),
        ("Colour21", rgb_str(&p[15])),
    ]
}

pub fn putty_reg(session_name: &str, p: &Palette) -> String {
    let mut out = format!(
        "Windows Registry Editor Version 5.00\r\n\r\n\
[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\{session_name}]\r\n"
    );
    for (name, val) in putty_colours(p) {
        out.push_str(&format!("\"{name}\"=\"{val}\"\r\n"));
    }
    out
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
    #[test]
    fn wezterm_alacritty_hyper_and_putty_themes_are_idempotent() {
        let p: Palette = std::array::from_fn(|_| "#123456".into());

        // WezTerm
        let wt_toml = wezterm_theme(&p);
        assert!(wt_toml.contains("name = \"GlazeBar Aether\""));
        let wt_lua = wezterm_settings(
            "local wezterm = require 'wezterm'\nlocal config = {}\nreturn config\n",
            theme::NAME,
        );
        assert!(wt_lua.contains("config.color_scheme = \"GlazeBar Aether\""));
        assert_eq!(wezterm_settings(&wt_lua, theme::NAME), wt_lua);
        let wt_existing = "local config = {\n  color_scheme = 'Dracula',\n}\nreturn config\n";
        let wt_updated = wezterm_settings(wt_existing, theme::NAME);
        assert!(wt_updated.contains("color_scheme = \"GlazeBar Aether\","));
        assert_eq!(wezterm_settings(&wt_updated, theme::NAME), wt_updated);

        // Alacritty
        let alac1 = alacritty_settings("[window]\nopacity = 0.9\n", &p);
        assert!(alac1.starts_with("[window]\nopacity = 0.9"));
        assert!(alac1.contains("[colors.primary]\nbackground = \"#123456\""));
        assert_eq!(alacritty_settings(&alac1, &p), alac1);

        // Hyper
        let hyper1 = hyper_settings("module.exports = { config: { fontSize: 14 } };\n", &p);
        assert!(hyper1.contains("fontSize: 14"));
        assert!(hyper1.contains("backgroundColor: \"#123456\""));
        assert_eq!(hyper_settings(&hyper1, &p), hyper1);

        // PuTTY
        let cols = putty_colours(&p);
        assert_eq!(cols.len(), 22);
        assert_eq!(cols[0], ("Colour0", "18,52,86".to_owned()));
        let reg = putty_reg("Default%20Settings", &p);
        assert!(reg.contains("[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\Default%20Settings]"));
        assert!(reg.contains("\"Colour0\"=\"18,52,86\""));
        assert!(reg.contains("\"Colour21\"=\"18,52,86\""));
    }
}
