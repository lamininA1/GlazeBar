use crate::wallhaven::SearchOptions;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub last_search: SearchOptions,
    pub favorites: Vec<SearchOptions>,
    pub saved_wallpapers: Vec<crate::wallhaven::Wallpaper>,
}
impl Preferences {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("theme-state/wallhaven.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        ensure!(
            std::fs::metadata(&path)?.len() <= 4 << 20,
            "Wallhaven 설정 파일이 너무 큽니다"
        );
        let mut value: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        value.last_search.validate()?;
        let mut unique = Vec::new();
        for mut entry in value.favorites {
            entry.query = entry.query.trim().into();
            entry.seed = None;
            entry.validate()?;
            if !entry.query.is_empty() && !unique.iter().any(|v: &SearchOptions| same(v, &entry)) {
                unique.push(entry)
            }
        }
        value.favorites = unique;
        let mut saved = Vec::new();
        for row in value.saved_wallpapers {
            if crate::wallhaven::valid_id(&row.id)
                && row.purity == "sfw"
                && !saved
                    .iter()
                    .any(|v: &crate::wallhaven::Wallpaper| v.id == row.id)
            {
                saved.push(row)
            }
        }
        value.saved_wallpapers = saved;
        Ok(value)
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        let data = serde_json::to_vec_pretty(self)?;
        ensure!(
            data.len() <= 4 << 20,
            "Wallhaven 보관 설정이 너무 큽니다. 사용하지 않는 즐겨찾기를 정리해주세요"
        );
        crate::theme_transaction::atomic_write(&root.join("theme-state/wallhaven.json"), &data)
    }
    pub fn toggle(&mut self, mut options: SearchOptions) -> Result<bool> {
        options.query = options.query.trim().into();
        options.seed = None;
        options.validate()?;
        ensure!(
            !options.query.is_empty(),
            "태그나 검색어를 입력한 뒤 즐겨찾기에 저장해주세요"
        );
        if let Some(i) = self.favorites.iter().position(|v| same(v, &options)) {
            self.favorites.remove(i);
            Ok(false)
        } else {
            self.favorites.push(options);
            Ok(true)
        }
    }
}
pub fn same(a: &SearchOptions, b: &SearchOptions) -> bool {
    a.query.trim().to_lowercase() == b.query.trim().to_lowercase() && a.filters == b.filters
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn favorite_roundtrip_dedup_and_filter_preservation() {
        let root = tempfile::tempdir().unwrap();
        let mut prefs = Preferences::load(root.path()).unwrap();
        let mut options = SearchOptions {
            query: " +landscape -people ".into(),
            ..Default::default()
        };
        options.filters.minimum = 2;
        assert!(prefs.toggle(options.clone()).unwrap());
        prefs.last_search = options.clone();
        prefs.saved_wallpapers.push(crate::wallhaven::Wallpaper {
            id: "abc123".into(),
            purity: "sfw".into(),
            ..Default::default()
        });
        prefs.save(root.path()).unwrap();
        let mut loaded = Preferences::load(root.path()).unwrap();
        assert_eq!(loaded.favorites.len(), 1);
        assert_eq!(loaded.saved_wallpapers[0].id, "abc123");
        assert_eq!(loaded.favorites[0].query, "+landscape -people");
        assert_eq!(loaded.favorites[0].filters.minimum, 2);
        options.query = "+LANDSCAPE -PEOPLE".into();
        assert!(!loaded.toggle(options).unwrap());
        assert!(loaded.favorites.is_empty());
        assert!(loaded.toggle(Default::default()).is_err());
        std::fs::write(root.path().join("theme-state/wallhaven.json"), b"broken").unwrap();
        assert!(Preferences::load(root.path()).is_err());
    }
}
