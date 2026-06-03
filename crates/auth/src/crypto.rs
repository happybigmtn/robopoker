use super::*;

/// Minimum acceptable JWT secret length in bytes.
///
/// 32 bytes (256 bits) matches the output size of SHA-256 and is the
/// smallest secret length recommended for HS256 by RFC 7518 §3.2 — anything
/// shorter is treated as a misconfiguration rather than silently accepted.
pub const MIN_SECRET_LEN: usize = 32;

const ACCESS_TOKEN_DURATION: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Errors that can occur when constructing a [`Crypto`] service.
#[derive(Debug)]
pub enum SecretError {
    /// The `JWT_SECRET` environment variable was not set.
    Missing,
    /// The secret was present but empty or too short to safely sign JWTs.
    TooShort { len: usize, min: usize },
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => write!(
                f,
                "JWT_SECRET environment variable is not set; refusing to start"
            ),
            Self::TooShort { len, min } => write!(
                f,
                "JWT_SECRET must be at least {min} bytes (got {len}); refusing to start with \
                 a weak signing key"
            ),
        }
    }
}

impl std::error::Error for SecretError {}

pub struct Crypto {
    encoding: jsonwebtoken::EncodingKey,
    decoding: jsonwebtoken::DecodingKey,
}

impl std::fmt::Debug for Crypto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't expose secret material via Debug — `EncodingKey`/`DecodingKey`
        // are intentionally not `Debug` themselves to prevent accidental
        // logging, so we mirror that here.
        f.debug_struct("Crypto").finish_non_exhaustive()
    }
}

impl Crypto {
    /// Build a [`Crypto`] service from a secret byte slice.
    ///
    /// Returns [`SecretError::TooShort`] when the secret is shorter than
    /// [`MIN_SECRET_LEN`]. Callers must surface that error to the operator
    /// — there is no longer a silent empty-string fallback.
    pub fn new(secret: &[u8]) -> Result<Self, SecretError> {
        if secret.len() < MIN_SECRET_LEN {
            return Err(SecretError::TooShort {
                len: secret.len(),
                min: MIN_SECRET_LEN,
            });
        }
        Ok(Self {
            encoding: jsonwebtoken::EncodingKey::from_secret(secret),
            decoding: jsonwebtoken::DecodingKey::from_secret(secret),
        })
    }

    /// Build a [`Crypto`] service from the `JWT_SECRET` environment variable.
    ///
    /// A missing variable, an empty value, or a value shorter than
    /// [`MIN_SECRET_LEN`] is a fatal configuration error: the previous
    /// behavior silently fell back to the empty string, which is exactly
    /// the production token-forgery risk `STW-004` exists to close.
    pub fn from_env() -> Result<Self, SecretError> {
        let raw = std::env::var("JWT_SECRET").map_err(|_| SecretError::Missing)?;
        // Reject empty strings explicitly — `String::default()` was the
        // previous fallback path and must stay closed.
        if raw.is_empty() {
            return Err(SecretError::TooShort {
                len: 0,
                min: MIN_SECRET_LEN,
            });
        }
        Self::new(raw.as_bytes())
    }

    pub fn encode(&self, claims: &Claims) -> Result<String, jsonwebtoken::errors::Error> {
        jsonwebtoken::encode(&jsonwebtoken::Header::default(), claims, &self.encoding)
    }
    pub fn decode(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        jsonwebtoken::decode::<Claims>(token, &self.decoding, &jsonwebtoken::Validation::default())
            .map(|data| data.claims)
    }
    /// SHA-256 of the raw JWT string. The middleware uses this to verify
    /// that a presented token matches the row stored at session creation,
    /// closing the "any valid JWT for this user passes" hole that existed
    /// when the hash was computed from the user id alone.
    pub fn hash(token: &str) -> Vec<u8> {
        use sha2::Digest;
        sha2::Sha256::digest(token.as_bytes()).to_vec()
    }
    pub const fn duration() -> std::time::Duration {
        ACCESS_TOKEN_DURATION
    }
}
