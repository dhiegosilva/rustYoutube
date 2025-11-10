use std::collections::HashMap;
use once_cell::sync::Lazy;
use serde_yaml::Value;
use std::fs;

static TRANSLATIONS: Lazy<HashMap<String, String>> = Lazy::new(|| {
    load_translations()
});

fn load_translations() -> HashMap<String, String> {
    let mut translations = HashMap::new();
    
    // Detect OS language
    let lang = detect_language();
    let locale_file = format!("locales/{}.yml", lang);
    
    // Try to load the detected language, fallback to English
    if let Ok(content) = fs::read_to_string(&locale_file) {
        if let Ok(yaml) = serde_yaml::from_str::<Value>(&content) {
            if let Some(lang_map) = yaml.as_mapping() {
                if let Some(translations_map) = lang_map.get(&Value::String(lang.clone())) {
                    if let Some(trans_map) = translations_map.as_mapping() {
                        for (key, value) in trans_map {
                            if let (Some(k), Some(v)) = (key.as_str(), value.as_str()) {
                                translations.insert(k.to_string(), v.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    
    // If no translations loaded, try English as fallback
    if translations.is_empty() {
        if let Ok(content) = fs::read_to_string("locales/en.yml") {
            if let Ok(yaml) = serde_yaml::from_str::<Value>(&content) {
                if let Some(lang_map) = yaml.as_mapping() {
                    if let Some(translations_map) = lang_map.get(&Value::String("en".to_string())) {
                        if let Some(trans_map) = translations_map.as_mapping() {
                            for (key, value) in trans_map {
                                if let (Some(k), Some(v)) = (key.as_str(), value.as_str()) {
                                    translations.insert(k.to_string(), v.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    translations
}

fn detect_language() -> String {
    // Try to get OS language
    #[cfg(windows)]
    {
        use std::env;
        if let Ok(lang) = env::var("LANG") {
            if lang.starts_with("pt_BR") || lang.starts_with("pt-BR") {
                return "pt-br".to_string();
            }
            if lang.starts_with("es_ES") || lang.starts_with("es-ES") {
                return "es".to_string();
            }
            if lang.starts_with("fr_FR") || lang.starts_with("fr-FR") {
                return "fr".to_string();
            }
            if lang.starts_with("pt") {
                return "pt-br".to_string();
            }
            if lang.starts_with("es") {
                return "es".to_string();
            }
            if lang.starts_with("fr") {
                return "fr".to_string();
            }
            if lang.starts_with("de") {
                return "de".to_string();
            }
        }
        // Try Windows locale
        if let Ok(locale) = env::var("LOCALE") {
            if locale.starts_with("pt_BR") || locale.starts_with("pt-BR") {
                return "pt-br".to_string();
            }
            if locale.starts_with("es_ES") || locale.starts_with("es-ES") {
                return "es".to_string();
            }
            if locale.starts_with("fr_FR") || locale.starts_with("fr-FR") {
                return "fr".to_string();
            }
            if locale.starts_with("pt") {
                return "pt-br".to_string();
            }
            if locale.starts_with("es") {
                return "es".to_string();
            }
            if locale.starts_with("fr") {
                return "fr".to_string();
            }
            if locale.starts_with("de") {
                return "de".to_string();
            }
        }
    }
    
    #[cfg(not(windows))]
    {
        use std::env;
        if let Ok(lang) = env::var("LANG") {
            if lang.starts_with("pt_BR") || lang.starts_with("pt-BR") {
                return "pt-br".to_string();
            }
            if lang.starts_with("es_ES") || lang.starts_with("es-ES") {
                return "es".to_string();
            }
            if lang.starts_with("fr_FR") || lang.starts_with("fr-FR") {
                return "fr".to_string();
            }
            if lang.starts_with("pt") {
                return "pt-br".to_string();
            }
            if lang.starts_with("es") {
                return "es".to_string();
            }
            if lang.starts_with("fr") {
                return "fr".to_string();
            }
            if lang.starts_with("de") {
                return "de".to_string();
            }
        }
    }
    
    "en".to_string() // Default to English
}

pub fn t(key: &str) -> String {
    TRANSLATIONS.get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

pub fn t_with_args(key: &str, args: &[(&str, &str)]) -> String {
    let mut result = t(key);
    
    // Replace placeholders
    for (k, v) in args {
        result = result.replace(&format!("%{{{}}}", k), v);
    }
    result
}
