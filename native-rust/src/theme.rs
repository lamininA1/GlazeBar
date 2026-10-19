//! Theme export compatibility with the pinned Go/Aether implementation.
//! Planning is pure: callers perform transactional writes separately.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
pub const NAME: &str = "GlazeBar Aether";
pub type Palette = [String; 16];

pub fn rgb(hex: &str) -> [f64; 3] {
    let n = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    [
        ((n >> 16) & 255) as f64,
        ((n >> 8) & 255) as f64,
        (n & 255) as f64,
    ]
}
fn linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
pub fn contrast(a: &str, b: &str) -> f64 {
    let luminance = |s: &str| {
        let [r, g, b] = rgb(s).map(|v| linear(v / 255.));
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
pub fn readable(colors: &[String]) -> Result<Palette> {
    ensure!(colors.len() == 16, "expected 16 colors");
    for c in colors {
        ensure!(
            c.len() == 7
                && c.starts_with('#')
                && c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit),
            "invalid color: {c}"
        );
    }
    let mut p: Palette = std::array::from_fn(|i| colors[i].clone());
    for i in [7, 8, 15] {
        if contrast(&p[0], &p[i]) < 4.5 {
            p[i] = p[15].clone();
        }
        if contrast(&p[0], &p[i]) < 4.5 {
            p[i] = "#eeeeee".into();
            if contrast(&p[0], &p[i]) < 4.5 {
                p[i] = "#181818".into();
            }
        }
    }
    Ok(p)
}
pub fn accent(p: &Palette, light: bool) -> String {
    let [_, mut c, h] = to_oklch(&p[4]);
    if c > 0.01 {
        c = (c * 1.5).clamp(0.12, 0.20);
    }
    let channels = from_oklch([if light { 0.46 } else { 0.72 }, c, h]);
    format!("#{:02x}{:02x}{:02x}", channels[0], channels[1], channels[2])
}
pub fn to_oklch(hex: &str) -> [f64; 3] {
    let [l, a, b] = rgb_to_oklab(rgb(hex));
    let c = (a * a + b * b).sqrt();
    let mut h = b.atan2(a) * 180. / std::f64::consts::PI;
    if h < 0. {
        h += 360.;
    }
    [l, c, h]
}
pub fn rgb_to_oklab(rgb: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|v| linear(v / 255.));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2164557844 * g + 0.6952417517 * b).cbrt();
    let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
    let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
    [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, a, b]
}
pub fn to_oklab(hex: &str) -> [f64; 3] {
    rgb_to_oklab(rgb(hex))
}
pub fn from_oklch([l, c, h]: [f64; 3]) -> [u8; 3] {
    let rad = h * std::f64::consts::PI / 180.;
    let (a, b) = (c * rad.cos(), c * rad.sin());
    oklab_to_rgb([l, a, b])
}
pub fn oklab_to_rgb([l, a, b]: [f64; 3]) -> [u8; 3] {
    let l1 = l + 0.3963377774 * a + 0.2158037573 * b;
    let m1 = l - 0.1055613458 * a - 0.0638541728 * b;
    let s1 = l - 0.0894841775 * a - 1.2914855480 * b;
    let (l, m, s) = (l1 * l1 * l1, m1 * m1 * m1, s1 * s1 * s1);
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ]
    .map(|c| {
        let c = c.clamp(0., 1.);
        let c = if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * c.powf(1. / 2.4) - 0.055
        };
        (c * 255.).round() as u8
    })
}
pub fn terminal(p: &Palette) -> Value {
    let mut v = json!({"name":NAME,"background":p[0],"foreground":p[15],"cursorColor":p[6],"selectionBackground":p[8]});
    for (i, name) in [
        "black",
        "red",
        "green",
        "yellow",
        "blue",
        "purple",
        "cyan",
        "white",
        "brightBlack",
        "brightRed",
        "brightGreen",
        "brightYellow",
        "brightBlue",
        "brightPurple",
        "brightCyan",
        "brightWhite",
    ]
    .iter()
    .enumerate()
    {
        v[*name] = json!(p[i]);
    }
    v
}
pub fn code_colors(p: &Palette) -> Value {
    let mut v = json!({});
    for (name, i) in [
        ("editor.background", 0),
        ("editor.foreground", 15),
        ("editorCursor.foreground", 6),
        ("editorLineNumber.foreground", 8),
        ("editorLineNumber.activeForeground", 15),
        ("sideBar.background", 0),
        ("sideBar.foreground", 7),
        ("sideBarTitle.foreground", 15),
        ("activityBar.background", 0),
        ("activityBar.foreground", 6),
        ("statusBar.background", 0),
        ("statusBar.foreground", 7),
        ("titleBar.activeBackground", 0),
        ("titleBar.activeForeground", 15),
        ("tab.activeBackground", 0),
        ("tab.activeForeground", 15),
        ("tab.inactiveBackground", 0),
        ("tab.inactiveForeground", 7),
        ("editorGroupHeader.tabsBackground", 0),
        ("panel.background", 0),
        ("input.background", 0),
        ("input.foreground", 15),
        ("focusBorder", 6),
        ("button.background", 4),
        ("button.foreground", 0),
        ("terminal.background", 0),
        ("terminal.foreground", 15),
        ("terminal.ansiBlack", 0),
        ("terminal.ansiRed", 1),
        ("terminal.ansiGreen", 2),
        ("terminal.ansiYellow", 3),
        ("terminal.ansiBlue", 4),
        ("terminal.ansiMagenta", 5),
        ("terminal.ansiCyan", 6),
        ("terminal.ansiWhite", 7),
        ("terminal.ansiBrightBlack", 8),
        ("terminal.ansiBrightRed", 9),
        ("terminal.ansiBrightGreen", 10),
        ("terminal.ansiBrightYellow", 11),
        ("terminal.ansiBrightBlue", 12),
        ("terminal.ansiBrightMagenta", 13),
        ("terminal.ansiBrightCyan", 14),
        ("terminal.ansiBrightWhite", 15),
        ("editorError.foreground", 1),
        ("editorWarning.foreground", 3),
        ("gitDecoration.addedResourceForeground", 2),
        ("gitDecoration.deletedResourceForeground", 1),
    ] {
        v[name] = json!(p[i]);
    }
    v["editor.selectionBackground"] = json!(format!("{}66", p[8]));
    v["editor.lineHighlightBackground"] = json!(format!("{}0d", p[7]));
    v
}
pub fn code_tokens(p: &Palette) -> Value {
    json!({"comments":p[8],"strings":p[2],"keywords":p[5],"numbers":p[3],"functions":p[4],"types":p[6],"variables":p[15]})
}
pub fn textmate(p: &Palette) -> String {
    let mut s = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>name</key><string>GlazeBar Aether</string><key>settings</key><array><dict><key>settings</key><dict>".to_owned();
    for (key, i) in [
        ("background", 0),
        ("foreground", 15),
        ("caret", 6),
        ("selection", 8),
    ] {
        s += &format!("<key>{key}</key><string>{}</string>", p[i]);
    }
    s += "</dict></dict>";
    for (scope, i) in [
        ("comment", 8),
        ("string", 2),
        ("keyword, storage", 5),
        ("constant.numeric, constant.language", 3),
        ("entity.name.function, support.function", 4),
        ("entity.name.type, support.type", 6),
        ("variable", 15),
        ("markup.inserted", 2),
        ("markup.deleted, invalid", 1),
        ("markup.changed", 3),
        ("markup.heading", 4),
    ] {
        s += &format!(
            "<dict><key>scope</key><string>{scope}</string><key>settings</key><dict><key>foreground</key><string>{}</string></dict></dict>",
            p[i]
        );
    }
    s += "</array></dict></plist>\n";
    s
}
pub fn export_preview(colors: &[String], light: bool) -> Result<Value> {
    let p = readable(colors)?;
    Ok(
        json!({"palette":p,"accent":accent(&p,light),"terminal":terminal(&p),"codeColors":code_colors(&p),"codeTokens":code_tokens(&p),"textMate":textmate(&p),"glazebar":{"name":NAME,"palette":p,"light":light}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exports_match_go_reference() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../fixtures/theme-export-go.json")).unwrap();
        assert_eq!(cases.len(), 138);
        for (i, case) in cases.iter().enumerate() {
            let input: Vec<String> = serde_json::from_value(case["input"].clone()).unwrap();
            assert_eq!(
                export_preview(&input, case["light"].as_bool().unwrap()).unwrap(),
                case["expected"],
                "Go reference case {i}"
            );
        }
    }
    #[test]
    fn invalid_palette_rejected_and_foregrounds_remain_readable() {
        assert!(readable(&vec!["#000000".into(); 15]).is_err());
        assert!(readable(&vec!["#00000z".into(); 16]).is_err());
        for bg in ["#000000", "#ffffff", "#666666", "#888888"] {
            let p = readable(&vec![bg.into(); 16]).unwrap();
            for i in [7, 8, 15] {
                assert!(contrast(&p[0], &p[i]) >= 4.5);
            }
        }
    }
}
