use crate::theme::{self, Palette};

pub struct Scheme {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub dark: [&'static str; 16],
    pub light: [&'static str; 16],
}

pub const SCHEMES: [Scheme; 6] = [
    Scheme {
        id: "catppuccin",
        name: "Catppuccin",
        summary: "부드러운 파스텔 톤 · Mocha / Latte",
        dark: [
            "#1e1e2e", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5",
            "#bac2de", "#9399b2", "#f7aec2", "#c2ecbf", "#fcd682", "#aeccfc", "#cba6f7",
            "#b1eae1", "#cdd6f4",
        ],
        light: [
            "#eff1f5", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299",
            "#5c5f77", "#6c6f85", "#e64553", "#40a02b", "#fe640b", "#209fb5", "#8839ef",
            "#04a5e5", "#4c4f69",
        ],
    },
    Scheme {
        id: "everforest",
        name: "Everforest",
        summary: "편안한 숲 색감 · Dark / Light",
        dark: [
            "#2d353b", "#e67e80", "#a7c080", "#dbbc7f", "#7fbbb3", "#d699b6", "#83c092",
            "#d3c6aa", "#9da9a0", "#e67e80", "#a7c080", "#dbbc7f", "#7fbbb3", "#d699b6",
            "#83c092", "#d3c6aa",
        ],
        light: [
            "#fdf6e3", "#f85552", "#8da101", "#dfa000", "#3a94c5", "#df69ba", "#35a77c",
            "#5c6a72", "#65737c", "#f85552", "#8da101", "#dfa000", "#3a94c5", "#df69ba",
            "#35a77c", "#4b565c",
        ],
    },
    Scheme {
        id: "rose-pine",
        name: "Rosé Pine",
        summary: "우아한 소나무와 장미빛 · Main / Dawn",
        dark: [
            "#191724", "#eb6f92", "#31748f", "#f6c177", "#9ccfd8", "#c4a7e7", "#ebbcba",
            "#e0def4", "#908caa", "#eb6f92", "#31748f", "#f6c177", "#9ccfd8", "#c4a7e7",
            "#ebbcba", "#e0def4",
        ],
        light: [
            "#faf4ed", "#b4637a", "#286983", "#ea9d34", "#56949f", "#907aa9", "#d7827e",
            "#575279", "#6e6a86", "#b4637a", "#286983", "#ea9d34", "#56949f", "#907aa9",
            "#d7827e", "#575279",
        ],
    },
    Scheme {
        id: "monokai",
        name: "Monokai",
        summary: "선명한 클래식 코딩 테마 · Pro / Light",
        dark: [
            "#272822", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff", "#a1efe4",
            "#f8f8f2", "#9a9b94", "#fd5ff0", "#b6e354", "#feed6c", "#8cedff", "#9e6ffe",
            "#899ca1", "#f9f8f5",
        ],
        light: [
            "#fafafa", "#d81b60", "#558b2f", "#c67100", "#0277bd", "#6a1b9a", "#00838f",
            "#49483e", "#686658", "#e91e63", "#689f38", "#ef6c00", "#0288d1", "#8e24aa",
            "#0097a7", "#272822",
        ],
    },
    Scheme {
        id: "dracula",
        name: "Dracula",
        summary: "깊은 보라색과 네온 강조 · Dracula / Alucard",
        dark: [
            "#282a36", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd",
            "#f8f8f2", "#92a2d4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df",
            "#a4ffff", "#ffffff",
        ],
        light: [
            "#fffbeb", "#cb3a2a", "#14710a", "#846e15", "#644ac9", "#a3144d", "#036a96",
            "#3a3746", "#646072", "#d74c3d", "#1f8a14", "#9e841b", "#785ee0", "#be2160",
            "#0a7fb0", "#1f1f1f",
        ],
    },
    Scheme {
        id: "tokyo-night",
        name: "Tokyo Night",
        summary: "도쿄의 밤하늘 색감 · Night / Day",
        dark: [
            "#1a1b26", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff",
            "#a9b1d6", "#8992b8", "#ff899d", "#b9f27c", "#ff9e64", "#8db0ff", "#c0caf5",
            "#b4f9f8", "#c0caf5",
        ],
        light: [
            "#e1e2e7", "#f52a65", "#587539", "#8c6c3e", "#2e7de9", "#9854f1", "#007197",
            "#4c505e", "#5a6072", "#d91b52", "#48632b", "#7a5b2e", "#1b69d4", "#7e39d8",
            "#005f80", "#2e3440",
        ],
    },
];

pub fn normalize_id(name: &str) -> Option<&'static str> {
    let lower = name.trim().to_ascii_lowercase();
    match lower.as_str() {
        "catppuccin"
        | "catppucin"
        | "catppuccin-mocha"
        | "catppuccin-latte"
        | "catppuccin-frappe"
        | "catppuccin-macchiato" => Some("catppuccin"),
        "everforest" | "everforest-dark" | "everforest-light" | "everforest_dark"
        | "everforest_light" => Some("everforest"),
        "rose-pine" | "rose_pine" | "rosepine" | "rose-pine-dawn" | "rose-pine-moon" => {
            Some("rose-pine")
        }
        "monokai" | "monakai" | "monokai-pro" | "monokai-light" => Some("monokai"),
        "dracula" | "alucard" => Some("dracula"),
        "tokyo-night" | "tokyonight" | "tokyo_night" | "tokyo-night-day" => Some("tokyo-night"),
        _ => None,
    }
}

pub fn find(name: &str) -> Option<&'static Scheme> {
    let id = normalize_id(name)?;
    SCHEMES.iter().find(|s| s.id == id)
}

pub fn palette(name: &str, light: bool) -> Option<Palette> {
    let scheme = find(name)?;
    let raw = if light { &scheme.light } else { &scheme.dark };
    let vec: Vec<String> = raw.iter().map(|s| (*s).to_owned()).collect();
    theme::readable(&vec).ok()
}

pub fn all_palettes(light: bool) -> [Palette; 6] {
    std::array::from_fn(|i| {
        let raw = if light {
            &SCHEMES[i].light
        } else {
            &SCHEMES[i].dark
        };
        let vec: Vec<String> = raw.iter().map(|s| (*s).to_owned()).collect();
        theme::readable(&vec).expect("valid preset palette")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_are_valid_and_readable_in_dark_and_light() {
        for scheme in &SCHEMES {
            for light in [false, true] {
                let raw = if light { &scheme.light } else { &scheme.dark };
                let colors: Vec<String> = raw.iter().map(|s| (*s).to_owned()).collect();
                let p = theme::readable(&colors).unwrap();
                for i in [7, 8, 15] {
                    assert!(
                        theme::contrast(&p[0], &p[i]) >= 4.5,
                        "preset {} (light={}) contrast at slot {}",
                        scheme.id,
                        light,
                        i
                    );
                }
            }
        }
    }

    #[test]
    fn aliases_resolve_to_canonical_scheme() {
        assert!(palette("catppucin", false).is_some());
        assert!(palette("catppuccin", true).is_some());
        assert!(palette("everforest", false).is_some());
        assert!(palette("rose-pine", true).is_some());
        assert!(palette("monakai", false).is_some());
        assert!(palette("monokai", true).is_some());
    }
}
