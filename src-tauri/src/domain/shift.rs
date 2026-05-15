#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Shift {
    pub shift_id: String,
    pub branch_id: String,
    pub device_id: String,
    pub cashier_user_id: String,
    pub cashier_name: String,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub opening_cash_minor: i64,
    pub status: String,
}
