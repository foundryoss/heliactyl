use crate::json::json::rs;



pub async fn root() -> axum::response::Json<serde_json::Value> {
    let data = serde_json::json!({
        "endpoints": {
            "/": "GET - Root endpoint",
            "/api": "GET - API endpoint (to be implemented)",
            "/status": "GET - Status endpoint (to be implemented)"
        }
    });
    rs(200, data)
}