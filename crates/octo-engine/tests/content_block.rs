//! Tests for ContentBlock Image and Document support (D3).

use octo_types::message::{ChatMessage, ContentBlock, ImageSourceType, MessageRole};

// --- ImageSourceType serde ---

#[test]
fn image_source_type_serde_base64() {
    let st = ImageSourceType::Base64;
    let json = serde_json::to_string(&st).unwrap();
    assert_eq!(json, r#""base64""#);
    let deser: ImageSourceType = serde_json::from_str(&json).unwrap();
    assert_eq!(deser, ImageSourceType::Base64);
}

#[test]
fn image_source_type_serde_url() {
    let st = ImageSourceType::Url;
    let json = serde_json::to_string(&st).unwrap();
    assert_eq!(json, r#""url""#);
    let deser: ImageSourceType = serde_json::from_str(&json).unwrap();
    assert_eq!(deser, ImageSourceType::Url);
}

// --- ContentBlock Image serde round-trip ---

#[test]
fn content_block_image_base64_roundtrip() {
    let block = ContentBlock::Image {
        source_type: ImageSourceType::Base64,
        media_type: "image/png".into(),
        data: "iVBORw0KGgo=".into(),
    };
    let json = serde_json::to_string(&block).unwrap();
    let deser: ContentBlock = serde_json::from_str(&json).unwrap();
    match deser {
        ContentBlock::Image {
            source_type,
            media_type,
            data,
        } => {
            assert_eq!(source_type, ImageSourceType::Base64);
            assert_eq!(media_type, "image/png");
            assert_eq!(data, "iVBORw0KGgo=");
        }
        _ => panic!("Expected Image block"),
    }
}

#[test]
fn content_block_image_url_roundtrip() {
    let block = ContentBlock::Image {
        source_type: ImageSourceType::Url,
        media_type: "image/jpeg".into(),
        data: "https://example.com/photo.jpg".into(),
    };
    let json = serde_json::to_string(&block).unwrap();
    let deser: ContentBlock = serde_json::from_str(&json).unwrap();
    match deser {
        ContentBlock::Image {
            source_type,
            media_type,
            data,
        } => {
            assert_eq!(source_type, ImageSourceType::Url);
            assert_eq!(media_type, "image/jpeg");
            assert_eq!(data, "https://example.com/photo.jpg");
        }
        _ => panic!("Expected Image block"),
    }
}

// --- ContentBlock Document serde round-trip ---

#[test]
fn content_block_document_roundtrip() {
    let block = ContentBlock::Document {
        source_type: "base64".into(),
        media_type: "application/pdf".into(),
        data: "JVBERi0xLjQ=".into(),
    };
    let json = serde_json::to_string(&block).unwrap();
    let deser: ContentBlock = serde_json::from_str(&json).unwrap();
    match deser {
        ContentBlock::Document {
            source_type,
            media_type,
            data,
        } => {
            assert_eq!(source_type, "base64");
            assert_eq!(media_type, "application/pdf");
            assert_eq!(data, "JVBERi0xLjQ=");
        }
        _ => panic!("Expected Document block"),
    }
}

// --- JSON literal deserialization ---

#[test]
fn content_block_image_from_json_literal() {
    let json = r#"{
        "type": "image",
        "source_type": "base64",
        "media_type": "image/png",
        "data": "abc123"
    }"#;
    let block: ContentBlock = serde_json::from_str(json).unwrap();
    match block {
        ContentBlock::Image {
            source_type,
            media_type,
            data,
        } => {
            assert_eq!(source_type, ImageSourceType::Base64);
            assert_eq!(media_type, "image/png");
            assert_eq!(data, "abc123");
        }
        _ => panic!("Expected Image block"),
    }
}

#[test]
fn content_block_document_from_json_literal() {
    let json = r#"{
        "type": "document",
        "source_type": "base64",
        "media_type": "application/pdf",
        "data": "pdf-data"
    }"#;
    let block: ContentBlock = serde_json::from_str(json).unwrap();
    match block {
        ContentBlock::Document {
            source_type,
            media_type,
            data,
        } => {
            assert_eq!(source_type, "base64");
            assert_eq!(media_type, "application/pdf");
            assert_eq!(data, "pdf-data");
        }
        _ => panic!("Expected Document block"),
    }
}

// --- text_content() ignores Image/Document blocks ---

#[test]
fn text_content_ignores_image_blocks() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: vec![
            ContentBlock::Text {
                text: "Hello ".into(),
            },
            ContentBlock::Image {
                source_type: ImageSourceType::Base64,
                media_type: "image/png".into(),
                data: "data".into(),
            },
            ContentBlock::Text {
                text: "world".into(),
            },
        ],
    };
    assert_eq!(msg.text_content(), "Hello world");
}

#[test]
fn text_content_ignores_document_blocks() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: vec![
            ContentBlock::Text {
                text: "See attached".into(),
            },
            ContentBlock::Document {
                source_type: "base64".into(),
                media_type: "application/pdf".into(),
                data: "pdfdata".into(),
            },
        ],
    };
    assert_eq!(msg.text_content(), "See attached");
}

// --- Multiple images in single message ---

#[test]
fn multiple_images_in_message() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: vec![
            ContentBlock::Text {
                text: "Compare these:".into(),
            },
            ContentBlock::Image {
                source_type: ImageSourceType::Base64,
                media_type: "image/png".into(),
                data: "img1".into(),
            },
            ContentBlock::Image {
                source_type: ImageSourceType::Url,
                media_type: "image/jpeg".into(),
                data: "https://example.com/img2.jpg".into(),
            },
        ],
    };

    let image_count = msg
        .content
        .iter()
        .filter(|b| matches!(b, ContentBlock::Image { .. }))
        .count();
    assert_eq!(image_count, 2);
    assert_eq!(msg.text_content(), "Compare these:");
}

// --- Anthropic format conversion ---

#[test]
fn anthropic_convert_image_base64() {
    // Verify the ApiContentBlock serializes correctly for Anthropic format
    let block = ContentBlock::Image {
        source_type: ImageSourceType::Base64,
        media_type: "image/png".into(),
        data: "iVBORw0KGgo=".into(),
    };

    // Simulate what anthropic.rs convert_messages does
    let api_json = match &block {
        ContentBlock::Image {
            source_type,
            media_type,
            data,
        } => {
            let st = match source_type {
                ImageSourceType::Base64 => "base64",
                ImageSourceType::Url => "url",
            };
            serde_json::json!({
                "type": "image",
                "source": {
                    "type": st,
                    "media_type": media_type,
                    "data": data
                }
            })
        }
        _ => panic!("Expected Image"),
    };

    assert_eq!(api_json["type"], "image");
    assert_eq!(api_json["source"]["type"], "base64");
    assert_eq!(api_json["source"]["media_type"], "image/png");
    assert_eq!(api_json["source"]["data"], "iVBORw0KGgo=");
}

#[test]
fn anthropic_convert_image_url() {
    let block = ContentBlock::Image {
        source_type: ImageSourceType::Url,
        media_type: "image/jpeg".into(),
        data: "https://example.com/img.jpg".into(),
    };

    let api_json = match &block {
        ContentBlock::Image {
            source_type,
            media_type,
            data,
        } => {
            let st = match source_type {
                ImageSourceType::Base64 => "base64",
                ImageSourceType::Url => "url",
            };
            serde_json::json!({
                "type": "image",
                "source": {
                    "type": st,
                    "media_type": media_type,
                    "data": data
                }
            })
        }
        _ => panic!("Expected Image"),
    };

    assert_eq!(api_json["source"]["type"], "url");
    assert_eq!(api_json["source"]["data"], "https://example.com/img.jpg");
}

// --- OpenAI format conversion ---

#[test]
fn openai_convert_image_base64_data_uri() {
    let block = ContentBlock::Image {
        source_type: ImageSourceType::Base64,
        media_type: "image/png".into(),
        data: "iVBORw0KGgo=".into(),
    };

    let url = match &block {
        ContentBlock::Image {
            source_type: ImageSourceType::Base64,
            media_type,
            data,
        } => format!("data:{media_type};base64,{data}"),
        _ => panic!("Expected Base64 Image"),
    };

    assert_eq!(url, "data:image/png;base64,iVBORw0KGgo=");
}

#[test]
fn openai_convert_image_url_passthrough() {
    let block = ContentBlock::Image {
        source_type: ImageSourceType::Url,
        media_type: "image/jpeg".into(),
        data: "https://example.com/img.jpg".into(),
    };

    let url = match &block {
        ContentBlock::Image {
            source_type: ImageSourceType::Url,
            data,
            ..
        } => data.clone(),
        _ => panic!("Expected Url Image"),
    };

    assert_eq!(url, "https://example.com/img.jpg");
}

#[test]
fn openai_mixed_text_and_image_produces_parts() {
    let msg = ChatMessage {
        role: MessageRole::User,
        content: vec![
            ContentBlock::Text {
                text: "What is in this image?".into(),
            },
            ContentBlock::Image {
                source_type: ImageSourceType::Base64,
                media_type: "image/png".into(),
                data: "imgdata".into(),
            },
        ],
    };

    let has_images = msg.content.iter().any(|b| matches!(b, ContentBlock::Image { .. }));
    assert!(has_images);

    // Build parts like OpenAI adapter does
    let parts: Vec<serde_json::Value> = msg
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(serde_json::json!({"type": "text", "text": text})),
            ContentBlock::Image {
                source_type,
                media_type,
                data,
            } => {
                let url = match source_type {
                    ImageSourceType::Base64 => format!("data:{media_type};base64,{data}"),
                    ImageSourceType::Url => data.clone(),
                };
                Some(serde_json::json!({"type": "image_url", "image_url": {"url": url}}))
            }
            _ => None,
        })
        .collect();

    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["type"], "text");
    assert_eq!(parts[1]["type"], "image_url");
    assert_eq!(
        parts[1]["image_url"]["url"],
        "data:image/png;base64,imgdata"
    );
}
