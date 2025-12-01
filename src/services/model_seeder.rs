use crate::services::database::{DatabaseService, ModelEntity};
use crate::services::model_registry::ModelType;
use serde_json::Value;

pub async fn seed_models(db: &DatabaseService) -> anyhow::Result<()> {
    seed_openai_models(db).await?;
    seed_anthropic_models(db).await?;
    seed_gemini_models(db).await?;
    Ok(())
}

async fn seed_openai_models(db: &DatabaseService) -> anyhow::Result<()> {
    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();

    if api_key.is_empty() {
        println!("OPENAI_API_KEY not set, using static OpenAI models.");
        return seed_static_openai_models(db).await;
    }

    let client = reqwest::Client::new();
    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let parsed: Value = resp.json().await?;
            if let Some(data) = parsed.get("data").and_then(|v| v.as_array()) {
                for item in data {
                    let id = item["id"].as_str().unwrap_or_default().to_string();
                    save_openai_model(db, &id).await?;
                }
            }
        }
        _ => {
            println!("Failed to fetch OpenAI models, using static fallback.");
            seed_static_openai_models(db).await?;
        }
    }
    Ok(())
}

async fn seed_static_openai_models(db: &DatabaseService) -> anyhow::Result<()> {
    let models = vec![
        "gpt-4o",
        "gpt-4-turbo",
        "gpt-4",
        "gpt-3.5-turbo",
        "dall-e-3",
        "dall-e-2",
        "text-embedding-3-small",
        "text-embedding-3-large",
        "tts-1",
        "whisper-1",
        "o1-preview",
        "o1-mini",
    ];
    for id in models {
        save_openai_model(db, id).await?;
    }
    Ok(())
}

async fn save_openai_model(db: &DatabaseService, id: &str) -> anyhow::Result<()> {
    let model_type = if id.starts_with("dall-e") {
        ModelType::ImageGeneration
    } else if id.starts_with("text-embedding") {
        ModelType::TextEmbedding
    } else if id.starts_with("tts") {
        ModelType::AudioGeneration
    } else if id.starts_with("whisper") {
        ModelType::SpeechRecognition
    } else if id.starts_with("omni-moderation") {
        ModelType::Moderation
    } else {
        ModelType::TextGeneration
    };

    let entity = ModelEntity {
        id: id.to_string(),
        provider: "OpenAI".to_string(),
        name: id.to_string(),
        description: None,
        model_type: model_type.to_string(),
        input_token_limit: None,
        output_token_limit: None,
        capabilities: "{}".to_string(),
        is_thinking: id.starts_with("o1") || id.starts_with("o3"),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    db.save_model(&entity).await?;
    Ok(())
}

async fn seed_anthropic_models(db: &DatabaseService) -> anyhow::Result<()> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();

    if api_key.is_empty() {
        println!("ANTHROPIC_API_KEY not set, using static Anthropic models.");
        return seed_static_anthropic_models(db).await;
    }

    let client = reqwest::Client::new();
    let response = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let parsed: Value = resp.json().await?;
            if let Some(data) = parsed.get("data").and_then(|v| v.as_array()) {
                for item in data {
                    let id = item["id"].as_str().unwrap_or_default().to_string();
                    let name = item["display_name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    save_anthropic_model(db, &id, &name).await?;
                }
            }
        }
        _ => {
            println!("Failed to fetch Anthropic models, using static fallback.");
            seed_static_anthropic_models(db).await?;
        }
    }
    Ok(())
}

async fn seed_static_anthropic_models(db: &DatabaseService) -> anyhow::Result<()> {
    let models = vec![
        ("claude-3-5-sonnet-20240620", "Claude 3.5 Sonnet"),
        ("claude-3-opus-20240229", "Claude 3 Opus"),
        ("claude-3-sonnet-20240229", "Claude 3 Sonnet"),
        ("claude-3-haiku-20240307", "Claude 3 Haiku"),
    ];
    for (id, name) in models {
        save_anthropic_model(db, id, name).await?;
    }
    Ok(())
}

async fn save_anthropic_model(db: &DatabaseService, id: &str, name: &str) -> anyhow::Result<()> {
    let entity = ModelEntity {
        id: id.to_string(),
        provider: "Anthropic".to_string(),
        name: name.to_string(),
        description: None,
        model_type: ModelType::TextGeneration.to_string(),
        input_token_limit: None,
        output_token_limit: None,
        capabilities: "{}".to_string(),
        is_thinking: false,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    db.save_model(&entity).await?;
    Ok(())
}

async fn seed_gemini_models(db: &DatabaseService) -> anyhow::Result<()> {
    let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();

    if api_key.is_empty() {
        println!("GEMINI_API_KEY not set, using static Gemini models.");
        return seed_static_gemini_models(db).await;
    }

    let client = reqwest::Client::new();
    let mut next_page_token: Option<String> = None;
    let mut success = false;

    loop {
        let mut url = "https://generativelanguage.googleapis.com/v1beta/models".to_string();
        if let Some(token) = &next_page_token {
            url.push_str(&format!("?pageToken={}", token));
        }

        let response = client
            .get(&url)
            .header("x-goog-api-key", &api_key)
            .header("Content-Type", "application/json")
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                success = true;
                let parsed: Value = resp.json().await?;
                if let Some(models) = parsed.get("models").and_then(|v| v.as_array()) {
                    for item in models {
                        // Logic to parse and save Gemini model (same as before)
                        let id = item["name"].as_str().unwrap_or_default().to_string();
                        let clean_id = id.trim_start_matches("models/").to_string();
                        let name = item["displayName"].as_str().unwrap_or_default().to_string();
                        let description = item
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let input_limit = item.get("inputTokenLimit").and_then(|v| v.as_i64());
                        let output_limit = item.get("outputTokenLimit").and_then(|v| v.as_i64());
                        let methods = item
                            .get("supportedGenerationMethods")
                            .and_then(|v| v.as_array());

                        let mut model_type = ModelType::TextGeneration;
                        if let Some(methods) = methods {
                            let methods_str: Vec<&str> =
                                methods.iter().filter_map(|v| v.as_str()).collect();
                            if methods_str.contains(&"predict") {
                                model_type = ModelType::ImageGeneration;
                            } else if methods_str.contains(&"embedContent")
                                || methods_str.contains(&"embedText")
                            {
                                model_type = ModelType::TextEmbedding;
                            }
                        }

                        let is_thinking = item
                            .get("thinking")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        let entity = ModelEntity {
                            id: clean_id,
                            provider: "Gemini".to_string(),
                            name,
                            description,
                            model_type: model_type.to_string(),
                            input_token_limit: input_limit,
                            output_token_limit: output_limit,
                            capabilities: "{}".to_string(),
                            is_thinking,
                            created_at: chrono::Utc::now().to_rfc3339(),
                        };
                        db.save_model(&entity).await?;
                    }
                }

                next_page_token = parsed
                    .get("nextPageToken")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if next_page_token.is_none() {
                    break;
                }
            }
            _ => {
                println!("Failed to fetch Gemini models.");
                break;
            }
        }
    }

    if !success {
        println!("Using static Gemini models fallback.");
        seed_static_gemini_models(db).await?;
    }

    Ok(())
}

async fn seed_static_gemini_models(db: &DatabaseService) -> anyhow::Result<()> {
    let models = vec![
        (
            "gemini-1.5-pro",
            "Gemini 1.5 Pro",
            ModelType::TextGeneration,
        ),
        (
            "gemini-1.5-flash",
            "Gemini 1.5 Flash",
            ModelType::TextGeneration,
        ),
        (
            "gemini-1.0-pro",
            "Gemini 1.0 Pro",
            ModelType::TextGeneration,
        ),
        (
            "text-embedding-004",
            "Text Embedding 004",
            ModelType::TextEmbedding,
        ),
        ("aqa", "AQA", ModelType::TextGeneration),
    ];

    for (id, name, m_type) in models {
        let entity = ModelEntity {
            id: id.to_string(),
            provider: "Gemini".to_string(),
            name: name.to_string(),
            description: None,
            model_type: m_type.to_string(),
            input_token_limit: None,
            output_token_limit: None,
            capabilities: "{}".to_string(),
            is_thinking: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        db.save_model(&entity).await?;
    }
    Ok(())
}
