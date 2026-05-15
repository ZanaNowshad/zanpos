#[derive(Debug, serde::Serialize)]
pub struct TodaySummary {
    pub business_date: String,
    pub transaction_count: i64,
    pub gross_total_minor: i64,
    pub discount_total_minor: i64,
    pub tax_total_minor: i64,
    pub net_total_minor: i64,
    pub cash_total_minor: i64,
    pub card_total_minor: i64,
    pub refund_count: i64,
    pub refund_total_minor: i64,
}
