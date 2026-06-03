use super::*;
use rbp_core::ID;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    pub sub: uuid::Uuid,
    pub sid: uuid::Uuid,
    pub usr: String,
    pub iat: i64,
    pub exp: i64,
}

impl Claims {
    pub fn new(user: ID<Member>, session: ID<Session>, username: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_secs() as i64;
        Self {
            sub: user.inner(),
            sid: session.inner(),
            usr: username,
            iat: now,
            exp: now + Crypto::duration().as_secs() as i64,
        }
    }
    pub fn expired(&self) -> bool {
        self.exp
            < std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_secs() as i64
    }
    pub fn user(&self) -> ID<Member> {
        ID::from(self.sub)
    }
    pub fn session(&self) -> ID<Session> {
        ID::from(self.sid)
    }
    pub fn username(&self) -> &str {
        &self.usr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_return_what_was_constructed() {
        let user = ID::<Member>::default();
        let sid = ID::<Session>::default();
        let claims = Claims::new(user, sid, "alice".to_string());
        assert_eq!(claims.user(), user);
        assert_eq!(claims.session(), sid);
        assert_eq!(claims.username(), "alice");
    }

    #[test]
    fn fresh_claims_are_not_expired() {
        let claims = Claims::new(ID::default(), ID::default(), "alice".to_string());
        assert!(!claims.expired());
    }

    #[test]
    fn claims_with_past_exp_are_expired() {
        let mut claims = Claims::new(ID::default(), ID::default(), "alice".to_string());
        claims.exp = 0;
        claims.iat = 0;
        assert!(claims.expired());
    }

    #[test]
    fn exp_is_far_enough_in_future() {
        // Sanity: the duration we configure gives us 15 minutes.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let claims = Claims::new(ID::default(), ID::default(), "alice".to_string());
        let skew = claims.exp - now;
        assert!(skew >= 14 * 60, "exp too close to now: {skew}");
        assert!(skew <= 16 * 60, "exp too far from now: {skew}");
    }
}
