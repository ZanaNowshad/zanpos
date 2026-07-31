use super::*;

#[test]
fn detects_supported_image_bytes_instead_of_trusting_extensions() {
    assert_eq!(
        detect_image_media_type(b"\x89PNG\r\n\x1a\nrest"),
        Some("image/png")
    );
    assert_eq!(
        detect_image_media_type(b"\xff\xd8\xff\xe0rest"),
        Some("image/jpeg")
    );
    assert_eq!(detect_image_media_type(b"not-an-image"), None);
}

#[test]
fn publisher_requires_an_origin_without_a_path_prefix() {
    assert!(HttpPublisher::new("https://shop.example.com/store", "1234567890abcdef").is_err());
    assert!(HttpPublisher::new("https://shop.example.com", "1234567890abcdef").is_ok());
}
