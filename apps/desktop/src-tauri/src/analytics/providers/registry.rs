use super::endpoint::Endpoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndpointKind {
    Fixed,
    Choice,
    User,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProviderSpec {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) kind: EndpointKind,
    pub(crate) default_endpoint: &'static str,
    pub(crate) models_path: &'static str,
    pub(crate) auth_header: Option<&'static str>,
    pub(crate) auth_prefix: &'static str,
    pub(crate) verified: bool,
}

pub(crate) const PROVIDERS: [ProviderSpec; 10] = [
    ProviderSpec {
        id: "anthropic",
        label: "Anthropic",
        kind: EndpointKind::Fixed,
        default_endpoint: "https://api.anthropic.com/v1",
        models_path: "/models",
        auth_header: Some("x-api-key"),
        auth_prefix: "",
        verified: false,
    },
    ProviderSpec {
        id: "openai",
        label: "OpenAI (Platform API key)",
        kind: EndpointKind::Fixed,
        default_endpoint: "https://api.openai.com/v1",
        models_path: "/models",
        auth_header: Some("Authorization"),
        auth_prefix: "Bearer ",
        verified: false,
    },
    ProviderSpec {
        id: "openrouter",
        label: "OpenRouter",
        kind: EndpointKind::Fixed,
        default_endpoint: "https://openrouter.ai/api/v1",
        models_path: "/models",
        auth_header: Some("Authorization"),
        auth_prefix: "Bearer ",
        verified: false,
    },
    ProviderSpec {
        id: "gemini",
        label: "Google Gemini",
        kind: EndpointKind::Fixed,
        default_endpoint: "https://generativelanguage.googleapis.com/v1beta/openai",
        models_path: "/models",
        auth_header: Some("Authorization"),
        auth_prefix: "Bearer ",
        verified: false,
    },
    ProviderSpec {
        id: "grok",
        label: "xAI Grok",
        kind: EndpointKind::Fixed,
        default_endpoint: "https://api.x.ai/v1",
        models_path: "/models",
        auth_header: Some("Authorization"),
        auth_prefix: "Bearer ",
        verified: false,
    },
    ProviderSpec {
        id: "deepseek",
        label: "DeepSeek",
        kind: EndpointKind::Fixed,
        default_endpoint: "https://api.deepseek.com/v1",
        models_path: "/models",
        auth_header: Some("Authorization"),
        auth_prefix: "Bearer ",
        verified: false,
    },
    ProviderSpec {
        id: "qwen",
        label: "Qwen (DashScope)",
        kind: EndpointKind::Choice,
        default_endpoint: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        models_path: "/models",
        auth_header: Some("Authorization"),
        auth_prefix: "Bearer ",
        verified: false,
    },
    ProviderSpec {
        id: "ollama",
        label: "Ollama (เครื่องนี้)",
        kind: EndpointKind::User,
        default_endpoint: "http://127.0.0.1:11434/v1",
        models_path: "/models",
        auth_header: Some("Authorization"),
        auth_prefix: "Bearer ",
        verified: false,
    },
    ProviderSpec {
        id: "litellm",
        label: "LiteLLM",
        kind: EndpointKind::User,
        default_endpoint: "http://127.0.0.1:4000/v1",
        models_path: "/models",
        auth_header: Some("Authorization"),
        auth_prefix: "Bearer ",
        verified: false,
    },
    ProviderSpec {
        id: "custom",
        label: "OpenAI-compatible",
        kind: EndpointKind::User,
        default_endpoint: "",
        models_path: "/models",
        auth_header: Some("Authorization"),
        auth_prefix: "Bearer ",
        verified: false,
    },
];

pub(crate) fn spec(id: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|provider| provider.id == id)
}

pub(crate) fn fixed_endpoint(id: &str, selected: Option<&str>) -> Result<Endpoint, &'static str> {
    let spec = spec(id).ok_or("provider_not_allowed")?;
    match spec.kind {
        EndpointKind::Fixed => Endpoint::parse(spec.default_endpoint),
        EndpointKind::Choice => {
            let host = selected.unwrap_or(spec.default_endpoint);
            let allowed = matches!(
                host,
                "https://dashscope-intl.aliyuncs.com/compatible-mode/v1"
                    | "https://dashscope.aliyuncs.com/compatible-mode/v1"
            );
            if !allowed {
                return Err("endpoint_invalid");
            }
            Endpoint::parse(host)
        }
        EndpointKind::User => Err("endpoint_required"),
    }
}

pub(crate) fn validate_model_id(value: &str) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > 200
        || !value.is_ascii()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("model_invalid");
    }
    Ok(())
}

pub(crate) fn known_models(provider: &str) -> &'static [&'static str] {
    match provider {
        "anthropic" => &["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"],
        _ => &[],
    }
}

pub(crate) fn parse_model_list(body: &[u8]) -> Result<Vec<String>, &'static str> {
    if body.len() > 256 * 1024 {
        return Err("model_list_too_large");
    }
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| "model_list_invalid")?;
    let rows = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.get("models").and_then(serde_json::Value::as_array))
        .ok_or("model_list_invalid")?;
    let mut models = Vec::new();
    for row in rows.iter().take(500) {
        let Some(id) = row
            .get("id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| row.get("name").and_then(serde_json::Value::as_str))
        else {
            continue;
        };
        if validate_model_id(id).is_ok() && !models.iter().any(|model| model == id) {
            models.push(id.to_owned());
        }
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_the_closed_ten_provider_set_and_every_row_starts_unverified() {
        assert_eq!(
            PROVIDERS
                .iter()
                .map(|provider| provider.id)
                .collect::<Vec<_>>(),
            [
                "anthropic",
                "openai",
                "openrouter",
                "gemini",
                "grok",
                "deepseek",
                "qwen",
                "ollama",
                "litellm",
                "custom",
            ]
        );
        assert!(PROVIDERS.iter().all(|provider| !provider.verified));
    }

    #[test]
    fn qwen_endpoint_is_a_closed_two_region_choice() {
        assert!(fixed_endpoint("qwen", None).is_ok());
        assert!(fixed_endpoint(
            "qwen",
            Some("https://dashscope.aliyuncs.com/compatible-mode/v1")
        )
        .is_ok());
        assert_eq!(
            fixed_endpoint("qwen", Some("https://evil.example/v1")),
            Err("endpoint_invalid")
        );
    }

    #[test]
    fn fetched_and_typed_model_ids_are_bounded_printable_ascii_without_whitespace() {
        for value in ["gpt-example", "vendor/model", "ollama:latest", "alias_1"] {
            assert!(validate_model_id(value).is_ok());
        }
        for value in ["", "two words", "bad\nmodel", "a".repeat(201).as_str(), "é"] {
            assert!(validate_model_id(value).is_err());
        }
    }

    #[test]
    fn anthopic_has_only_the_closed_three_model_list_and_openai_stays_unlisted() {
        assert_eq!(
            known_models("anthropic"),
            ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"]
        );
        assert!(known_models("openai").is_empty());
    }

    #[test]
    fn unverified_providers_do_not_publish_assumed_known_models() {
        for provider in [
            "openai", "openrouter", "gemini", "grok", "deepseek", "qwen", "ollama", "litellm",
            "custom",
        ] {
            assert!(known_models(provider).is_empty(), "{provider}");
        }
    }

    #[test]
    fn model_list_parser_is_bounded_deduplicated_and_skips_invalid_entries() {
        let rows = serde_json::json!({"data":[{"id":"model-a"},{"name":"vendor/model"},{"id":"bad model"},{"id":"model-a"}]});
        assert_eq!(
            parse_model_list(&serde_json::to_vec(&rows).unwrap()).unwrap(),
            ["model-a", "vendor/model"]
        );
        assert_eq!(
            parse_model_list(&vec![b' '; 256 * 1024 + 1]),
            Err("model_list_too_large")
        );
        assert_eq!(parse_model_list(b"[]"), Err("model_list_invalid"));
    }
}
