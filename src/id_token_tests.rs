//! Tokens here are signed with a throwaway key generated for these tests
//! (`testdata/id-token-test-key.pem`); it guards nothing.

use super::*;
use jsonwebtoken::{EncodingKey, Header, encode};

const ISSUER: &str = "https://auth.openai.com";
const KID: &str = "test-kid";

fn jwks() -> JwkSet {
    let n = include_str!("../testdata/id-token-test-key.n").trim();
    serde_json::from_value(serde_json::json!({"keys": [{
        "kty": "RSA", "use": "sig", "alg": "RS256", "kid": KID, "n": n, "e": "AQAB"
    }]}))
    .unwrap()
}

fn sign(claims: serde_json::Value, kid: &str) -> String {
    let key =
        EncodingKey::from_rsa_pem(include_bytes!("../testdata/id-token-test-key.pem")).unwrap();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());
    encode(&header, &claims, &key).unwrap()
}

fn claims(aud: &str, nonce: &str, exp_offset: i64) -> serde_json::Value {
    let now = crate::oauth::now_secs() as i64;
    serde_json::json!({
        "iss": ISSUER, "aud": aud, "sub": "user-sub", "email": "a@b.c",
        "nonce": nonce, "iat": now, "exp": now + exp_offset
    })
}

#[test]
fn valid_token_yields_the_subject() {
    let token = sign(claims("oaiapp_1", "n1", 600), KID);
    let identity = verify_with(&jwks(), ISSUER, &token, "oaiapp_1", "n1").unwrap();
    assert_eq!(identity.subject, "user-sub");
    assert_eq!(identity.email.as_deref(), Some("a@b.c"));
}

#[test]
fn wrong_audience_nonce_issuer_or_expiry_is_rejected() {
    let cases = [
        (sign(claims("oaiapp_other", "n1", 600), KID), "audience"),
        (sign(claims("oaiapp_1", "replayed", 600), KID), "nonce"),
        (sign(claims("oaiapp_1", "n1", -3600), KID), "expiry"),
        (sign(claims("oaiapp_1", "n1", 600), "unknown-kid"), "kid"),
    ];
    for (token, what) in cases {
        let err = verify_with(&jwks(), ISSUER, &token, "oaiapp_1", "n1").unwrap_err();
        assert_eq!(err.code, "invalid_id_token", "{what}");
    }
    let token = sign(claims("oaiapp_1", "n1", 600), KID);
    assert!(verify_with(&jwks(), "https://evil.example", &token, "oaiapp_1", "n1").is_err());
}

#[test]
fn tampered_payload_fails_the_signature() {
    let token = sign(claims("oaiapp_1", "n1", 600), KID);
    let mut parts: Vec<&str> = token.split('.').collect();
    let forged = sign(
        serde_json::json!({"iss": ISSUER, "aud": "oaiapp_1", "sub": "someone-else",
            "nonce": "n1", "exp": crate::oauth::now_secs() + 600}),
        KID,
    );
    let forged_payload = forged.split('.').nth(1).unwrap().to_string();
    parts[1] = &forged_payload;
    let spliced = parts.join(".");
    // Same key signed both, but the signature covers the original payload.
    let err = verify_with(&jwks(), ISSUER, &spliced, "oaiapp_1", "n1").unwrap_err();
    assert_eq!(err.code, "invalid_id_token");
}

#[test]
fn unsigned_tokens_are_rejected() {
    use base64::Engine;
    let b64 = |v: serde_json::Value| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v.to_string())
    };
    let token = format!(
        "{}.{}.",
        b64(serde_json::json!({"alg": "none", "kid": KID})),
        b64(claims("oaiapp_1", "n1", 600))
    );
    assert!(verify_with(&jwks(), ISSUER, &token, "oaiapp_1", "n1").is_err());
}
