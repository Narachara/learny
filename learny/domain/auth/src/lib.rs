use serde::{Deserialize, Serialize};

/// A registered user.
/// `password_hash` stores an Argon2 PHC string — never the raw password.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id:            String,
    pub username:      String,
    pub password_hash: String,
}

/// Credentials submitted on the login form.
#[derive(Clone, Debug, Deserialize)]
pub struct LoginCredentials {
    pub username: String,
    pub password: String,
}
