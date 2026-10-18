use std::collections::BTreeMap;
pub struct Locale {
    zh: BTreeMap<String, String>,
    en: BTreeMap<String, String>,
}
impl Locale {
    pub fn load() -> Result<Self, toml::de::Error> {
        Ok(Self {
            zh: toml::from_str(include_str!("../locales/zh-CN.toml"))?,
            en: toml::from_str(include_str!("../locales/en.toml"))?,
        })
    }
    pub fn text<'a>(&'a self, english: bool, key: &'a str) -> &'a str {
        let catalog = if english { &self.en } else { &self.zh };
        catalog.get(key).map(String::as_str).unwrap_or(key)
    }
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn every_interface_key_has_both_languages() {
        let locale = Locale::load().unwrap();
        assert_eq!(
            locale.zh.keys().collect::<Vec<_>>(),
            locale.en.keys().collect::<Vec<_>>()
        );
        assert!(locale.zh.len() >= 90);
        assert!(locale.zh.values().all(|v| !v.is_empty()));
        assert!(locale.en.values().all(|v| !v.is_empty()));
    }
}
