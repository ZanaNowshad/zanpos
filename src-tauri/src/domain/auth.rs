#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct UserSummary {
    pub user_id: String,
    pub display_name: String,
    pub username: String,
    pub role_name: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SessionUser {
    pub user_id: String,
    pub display_name: String,
    pub username: String,
    pub role_id: String,
    pub role_name: String,
}
