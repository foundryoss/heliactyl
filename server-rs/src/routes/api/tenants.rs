use crate::json::json::rs;

pub async fn tenants() -> axum::response::Json<serde_json::Value> {
    let data = serde_json::json!({
    });
    rs(200, data)
}