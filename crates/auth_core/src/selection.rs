use std::{error::Error, fmt};

use crate::{PresetConfigOverride, StoredApiPreset};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiPresetLookup<'a> {
    Auto(&'a str),
    Id(&'a str),
    Name(&'a str),
    Model(&'a str),
}

pub trait ApiPresetSelectionEntry {
    fn api_preset_id(&self) -> &str;
    fn api_preset_name(&self) -> &str;
    fn api_preset_model(&self) -> Option<&str>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiPresetSelectionError {
    message: String,
}

impl ApiPresetSelectionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ApiPresetSelectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ApiPresetSelectionError {}

pub fn model_from_config_overrides(overrides: &[PresetConfigOverride]) -> Option<&str> {
    overrides
        .iter()
        .rev()
        .find(|item| {
            item.key
                .as_deref()
                .is_some_and(|key| key.trim().eq_ignore_ascii_case("model"))
        })
        .and_then(|item| item.value.as_deref())
        .map(str::trim)
        .filter(|model| !model.is_empty())
}

pub fn api_preset_model(preset: &StoredApiPreset) -> Option<&str> {
    model_from_config_overrides(&preset.config_overrides)
}

impl ApiPresetSelectionEntry for StoredApiPreset {
    fn api_preset_id(&self) -> &str {
        &self.id
    }

    fn api_preset_name(&self) -> &str {
        &self.name
    }

    fn api_preset_model(&self) -> Option<&str> {
        api_preset_model(self)
    }
}

pub fn select_api_preset_index<T: ApiPresetSelectionEntry>(
    presets: &[T],
    lookup: ApiPresetLookup<'_>,
) -> Result<usize, ApiPresetSelectionError> {
    match lookup {
        ApiPresetLookup::Auto(selector) => {
            let selector = normalized_selector(selector)?;
            if let Some(index) = find_id(presets, selector) {
                return Ok(index);
            }
            match find_unique_name(presets, selector, NameMatchCase::Sensitive)? {
                Some(index) => Ok(index),
                None => {
                    find_model(presets, selector).ok_or_else(|| not_found("selector", selector))
                }
            }
        }
        ApiPresetLookup::Id(id) => {
            let id = normalized_selector(id)?;
            find_id(presets, id).ok_or_else(|| not_found("id", id))
        }
        ApiPresetLookup::Name(name) => {
            let name = normalized_selector(name)?;
            find_unique_name(presets, name, NameMatchCase::Insensitive)?
                .ok_or_else(|| not_found("name", name))
        }
        ApiPresetLookup::Model(model) => {
            let model = normalized_selector(model)?;
            find_model(presets, model).ok_or_else(|| not_found("model", model))
        }
    }
}

fn normalized_selector(selector: &str) -> Result<&str, ApiPresetSelectionError> {
    let selector = selector.trim();
    if selector.is_empty() {
        return Err(ApiPresetSelectionError::new("API preset selector cannot be empty"));
    }
    Ok(selector)
}

fn find_id<T: ApiPresetSelectionEntry>(presets: &[T], id: &str) -> Option<usize> {
    presets
        .iter()
        .position(|preset| preset.api_preset_id() == id)
}

fn find_unique_name<T: ApiPresetSelectionEntry>(
    presets: &[T],
    name: &str,
    match_case: NameMatchCase,
) -> Result<Option<usize>, ApiPresetSelectionError> {
    let matches = presets
        .iter()
        .enumerate()
        .filter(|(_, preset)| match match_case {
            NameMatchCase::Sensitive => preset.api_preset_name().trim() == name,
            NameMatchCase::Insensitive => {
                preset.api_preset_name().trim().eq_ignore_ascii_case(name)
            }
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [index] => Ok(Some(*index)),
        _ => Err(ApiPresetSelectionError::new(format!(
            "multiple API presets have the exact name `{name}`; use an id"
        ))),
    }
}

#[derive(Clone, Copy)]
enum NameMatchCase {
    Sensitive,
    Insensitive,
}

fn find_model<T: ApiPresetSelectionEntry>(presets: &[T], model: &str) -> Option<usize> {
    presets.iter().position(|preset| {
        preset
            .api_preset_model()
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(model))
    })
}

fn not_found(kind: &str, selector: &str) -> ApiPresetSelectionError {
    ApiPresetSelectionError::new(format!("API preset {kind} `{selector}` was not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Entry {
        id: &'static str,
        name: &'static str,
        model: Option<&'static str>,
    }

    impl ApiPresetSelectionEntry for Entry {
        fn api_preset_id(&self) -> &str {
            self.id
        }

        fn api_preset_name(&self) -> &str {
            self.name
        }

        fn api_preset_model(&self) -> Option<&str> {
            self.model
        }
    }

    fn entries() -> Vec<Entry> {
        vec![
            Entry {
                id: "api-1",
                name: "primary",
                model: Some("gpt-5.6-sol"),
            },
            Entry {
                id: "api-2",
                name: "backup",
                model: Some("gpt-5.6-sol"),
            },
            Entry {
                id: "api-3",
                name: "gpt-5.6-sol",
                model: Some("other-model"),
            },
        ]
    }

    #[test]
    fn auto_lookup_prefers_id_then_unique_name_then_first_model() {
        let entries = entries();
        assert_eq!(select_api_preset_index(&entries, ApiPresetLookup::Auto("api-2")).unwrap(), 1);
        assert_eq!(
            select_api_preset_index(&entries, ApiPresetLookup::Auto("gpt-5.6-sol")).unwrap(),
            2
        );
        assert_eq!(
            select_api_preset_index(&entries, ApiPresetLookup::Auto("GPT-5.6-SOL")).unwrap(),
            0
        );
    }

    #[test]
    fn explicit_model_lookup_uses_first_saved_match() {
        assert_eq!(
            select_api_preset_index(&entries(), ApiPresetLookup::Model("gpt-5.6-sol")).unwrap(),
            0
        );
    }

    #[test]
    fn duplicate_exact_names_are_rejected_before_model_fallback() {
        let entries = vec![
            Entry {
                id: "api-1",
                name: "same",
                model: Some("same"),
            },
            Entry {
                id: "api-2",
                name: "same",
                model: Some("same"),
            },
        ];
        assert!(select_api_preset_index(&entries, ApiPresetLookup::Auto("same")).is_err());
    }

    #[test]
    fn explicit_name_lookup_is_case_insensitive_and_never_falls_back_to_model() {
        let entries = entries();
        assert_eq!(select_api_preset_index(&entries, ApiPresetLookup::Name("PRIMARY")).unwrap(), 0);
        assert!(select_api_preset_index(&entries, ApiPresetLookup::Name("OTHER-MODEL")).is_err());
    }
}
