//! Explicit UI catalog. Window titles, group names and diagnostic arguments stay untouched.
use crate::preferences::Language;
use std::{collections::BTreeMap, sync::OnceLock};

type Catalog = BTreeMap<String, [String; 5]>;
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("locales/messages.json"))
            .expect("embedded locale catalog")
    })
}
pub fn text(language: Language, source: &str) -> String {
    if language == Language::English {
        return source.to_owned();
    }
    if let Some(values) = catalog().get(source) {
        return values[language.index() - 1].clone();
    }
    // Worker error messages use a stable English protocol, independently of presentation.
    if let Some(error) = source.strip_prefix("Error: ") {
        return replace(&text(language, "Error: {}"), &[text(language, error)]);
    }
    if let Some((_, result)) = catalog()
        .iter()
        .filter(|(template, _)| template.contains("{}"))
        .filter_map(|(template, values)| {
            arguments(template, source).map(|arguments| {
                (
                    template.len() - template.matches("{}").count() * 2,
                    replace(&values[language.index() - 1], &arguments),
                )
            })
        })
        .max_by_key(|(specificity, _)| *specificity)
    {
        return result;
    }
    // Unknown OS/network diagnostics remain accurate instead of being guessed.
    source.to_owned()
}
fn replace(template: &str, arguments: &[String]) -> String {
    let mut output = String::new();
    let mut parts = template.split("{}");
    output.push_str(parts.next().unwrap_or_default());
    for (argument, suffix) in arguments.iter().zip(parts) {
        output.push_str(argument);
        output.push_str(suffix);
    }
    output
}
fn arguments(template: &str, source: &str) -> Option<Vec<String>> {
    let mut parts = template.split("{}");
    let mut rest = source.strip_prefix(parts.next()?)?;
    let suffixes: Vec<_> = parts.collect();
    let mut result = vec![];
    for (index, suffix) in suffixes.iter().enumerate() {
        if index + 1 == suffixes.len() {
            result.push(rest.strip_suffix(suffix)?.to_owned());
            rest = "";
        } else {
            let end = rest.find(suffix)?;
            result.push(rest[..end].to_owned());
            rest = &rest[end + suffix.len()..];
        }
    }
    rest.is_empty().then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_translation_is_nonempty_and_preserves_message_arguments() {
        assert!(catalog().len() > 100);
        for (source, translations) in catalog() {
            for translation in translations {
                assert!(!translation.trim().is_empty(), "{source}");
                assert_eq!(
                    source.matches("{}").count(),
                    translation.matches("{}").count(),
                    "{source}: {translation}"
                );
            }
        }
    }
    #[test]
    fn translated_status_preserves_user_data_and_diagnostics() {
        assert_eq!(
            text(Language::French, "7 windows selected"),
            "7 fenêtres sélectionnées"
        );
        assert_eq!(
            text(
                Language::German,
                "Installed: 0.2.0. Check failed: timeout: example.org"
            ),
            "Installiert: 0.2.0. Prüfung fehlgeschlagen: timeout: example.org"
        );
        assert_eq!(
            text(Language::Russian, "Error: Select a saved group."),
            "Ошибка: Выберите сохранённую группу."
        );
        assert_eq!(
            text(Language::Italian, "My untouched document.txt"),
            "My untouched document.txt"
        );
    }
}
