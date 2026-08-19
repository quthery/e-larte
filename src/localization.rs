use std::{collections::HashMap, sync::LazyLock};

static ENGLISH: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| load(include_str!("../locales/en.json")));
static RUSSIAN: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| load(include_str!("../locales/ru.json")));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Language {
    English,
    Russian,
}

impl Language {
    pub(crate) fn system() -> Self {
        sys_locale::get_locale()
            .as_deref()
            .map(Self::from_locale)
            .unwrap_or(Self::English)
    }

    fn from_locale(locale: &str) -> Self {
        let language = locale.split(['-', '_']).next().unwrap_or_default();
        if language.eq_ignore_ascii_case("ru") {
            Self::Russian
        } else {
            Self::English
        }
    }

    pub(crate) fn text(self, key: &str) -> &'static str {
        let translations = match self {
            Self::English => &*ENGLISH,
            Self::Russian => &*RUSSIAN,
        };

        translations
            .get(key)
            .or_else(|| ENGLISH.get(key))
            .map(String::as_str)
            .unwrap_or("Missing translation")
    }
}

fn load(source: &str) -> HashMap<String, String> {
    serde_json::from_str(source).expect("embedded localization file must contain valid JSON")
}

#[cfg(test)]
mod tests {
    use super::Language;

    #[test]
    fn selects_russian_for_russian_locale() {
        assert_eq!(Language::from_locale("ru_RU"), Language::Russian);
        assert_eq!(Language::from_locale("ru-RU"), Language::Russian);
    }

    #[test]
    fn defaults_unknown_locales_to_english() {
        assert_eq!(Language::from_locale("en_US"), Language::English);
        assert_eq!(Language::from_locale("de-DE"), Language::English);
    }

    #[test]
    fn both_locales_have_the_same_keys() {
        assert_eq!(super::ENGLISH.len(), super::RUSSIAN.len());
        assert!(
            super::ENGLISH
                .keys()
                .all(|key| super::RUSSIAN.contains_key(key))
        );
    }
}
