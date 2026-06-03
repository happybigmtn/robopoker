use serde::Deserialize;
use serde::Serialize;

#[derive(Deserialize, Serialize)]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Deserialize, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
}
