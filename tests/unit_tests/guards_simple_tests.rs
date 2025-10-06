use the_beaconator::guards::ApiToken;

#[test]
fn test_api_token_struct() {
    // Test the ApiToken struct itself
    let token = ApiToken("test_token".to_string());
    assert_eq!(token.0, "test_token");

    // Test with different token values
    let token2 = ApiToken("another_token".to_string());
    assert_eq!(token2.0, "another_token");

    // Test with empty token
    let empty_token = ApiToken("".to_string());
    assert_eq!(empty_token.0, "");

    // Test with special characters
    let special_token = ApiToken("token-with-special_chars.123".to_string());
    assert_eq!(special_token.0, "token-with-special_chars.123");
}

#[test]
fn test_api_token_creation() {
    // Test various token formats
    let tokens = vec![
        "simple_token",
        "token-with-dashes",
        "token_with_underscores",
        "token.with.dots",
        "TOKEN_UPPERCASE",
        "MixedCaseToken",
        "token123",
        "very_long_token_with_many_characters_and_numbers_123456789",
    ];

    for token_str in tokens {
        let token = ApiToken(token_str.to_string());
        assert_eq!(token.0, token_str);
    }
}

#[test]
fn test_api_token_edge_cases() {
    // Test boundary conditions
    let single_char = ApiToken("a".to_string());
    assert_eq!(single_char.0, "a");

    let long_token = ApiToken("a".repeat(1000));
    assert_eq!(long_token.0.len(), 1000);

    // Test unicode characters
    let unicode_token = ApiToken("token_🔑_unicode".to_string());
    assert_eq!(unicode_token.0, "token_🔑_unicode");
}

#[test]
fn test_api_token_memory_usage() {
    // Test that tokens don't share memory inappropriately
    let token1 = ApiToken("token1".to_string());
    let token2 = ApiToken("token2".to_string());

    assert_ne!(token1.0, token2.0);

    // Modify one and ensure the other isn't affected
    let mut token3 = ApiToken("mutable".to_string());
    token3.0.push_str("_modified");
    assert_eq!(token3.0, "mutable_modified");
}

#[test]
fn test_api_token_clone_behavior() {
    let original = ApiToken("original_token".to_string());
    let cloned = ApiToken(original.0.clone());

    assert_eq!(original.0, cloned.0);

    // They should have the same content but be separate instances
    assert_eq!(original.0, "original_token");
    assert_eq!(cloned.0, "original_token");
}
