use ref_feder_core::key::{
    ActorKeyPair, KeyError, create_sha256_digest_header, sign_draft_cavage, verify_draft_cavage,
};

const PRIVATE_KEY_PEM: &str = include_str!("fixtures/rsa-private-key.pem");
const PUBLIC_KEY_PEM: &str = include_str!("fixtures/rsa-public-key.pem");
const OTHER_PUBLIC_KEY_PEM: &str = include_str!("fixtures/rsa-other-public-key.pem");

fn actor_key_pair() -> ActorKeyPair {
    ActorKeyPair::from_pem(PRIVATE_KEY_PEM.to_string(), PUBLIC_KEY_PEM.to_string())
        .expect("valid actor key pair")
}

#[test]
fn rejects_mismatched_persisted_keys() {
    let result = ActorKeyPair::from_pem(
        PRIVATE_KEY_PEM.to_string(),
        OTHER_PUBLIC_KEY_PEM.to_string(),
    );

    assert!(matches!(result, Err(KeyError::MismatchedKeyPair)));
}

#[test]
fn redacts_private_key_from_debug_output() {
    let pair = actor_key_pair();
    let debug = format!("{pair:?}");

    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains(pair.private_key_pem()));
}

#[test]
fn creates_known_sha256_digest() {
    assert_eq!(
        create_sha256_digest_header(b"Hello, world!"),
        "SHA-256=MV9b23bQeMQ7isAGTkoBZGErH853yGk0W/yUx1iU7dM="
    );
}

#[test]
fn signs_and_verifies_draft_cavage_request() {
    let pair = actor_key_pair();
    let headers = [
        ("date", "Tue, 05 Mar 2024 07:49:44 GMT"),
        (
            "digest",
            "SHA-256=MV9b23bQeMQ7isAGTkoBZGErH853yGk0W/yUx1iU7dM=",
        ),
        ("host", "example.com"),
    ];
    let signature_header =
        sign_draft_cavage(&pair, "https://example.com/key", "POST", "/inbox", &headers)
            .expect("sign request");
    let signature = signature_header
        .rsplit_once("signature=\"")
        .and_then(|(_, signature)| signature.strip_suffix('"'))
        .expect("signature parameter");

    verify_draft_cavage(pair.public_key_pem(), "POST", "/inbox", &headers, signature)
        .expect("verify request");
    assert!(
        verify_draft_cavage(
            pair.public_key_pem(),
            "POST",
            "/other-inbox",
            &headers,
            signature,
        )
        .is_err()
    );
}
