#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
/// What the PIN screen may know before anyone has authenticated.
///
/// Deliberately without `user_id`. The list has to exist — a shared till shows
/// who is on shift so they can pick themselves — and the names and roles on it
/// are already visible to anyone standing at the screen. The account id was
/// different: it was the one field that turned "read the list" into "act as the
/// owner", back when a payload id established the caller. Login needs the
/// username, not the id, so nothing is lost by withholding it.
pub struct UserSummary {
    pub display_name: String,
    pub username: String,
    pub role_name: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SessionUser {
    pub user_id: String,
    pub branch_id: String,
    pub display_name: String,
    pub username: String,
    pub role_id: String,
    pub role_name: String,
    pub session_token: String,
    pub session_expires_at: String,
}
