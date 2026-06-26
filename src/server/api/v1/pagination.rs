//! Helpers for opaque pagination tokens.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Encodes a pagination cursor into an opaque token.
pub fn encode_token<T: Serialize>(cursor: &T) -> Result<String, serde_json::Error> {
    let json = serde_json::to_vec(cursor)?;
    Ok(URL_SAFE_NO_PAD.encode(json))
}

/// Decodes a pagination cursor from an opaque token.
pub fn decode_token<T: DeserializeOwned>(token: &str) -> Result<T, String> {
    let decoded = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| format!("invalid `next_token`: `{token}`"))?;

    serde_json::from_slice(&decoded).map_err(|_| format!("invalid `next_token`: `{token}`"))
}
