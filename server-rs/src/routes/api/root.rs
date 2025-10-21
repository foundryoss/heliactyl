use crate::json::json::rs;

pub async fn root() -> axum::response::Json<serde_json::Value> {
    let data = serde_json::json!({
        "endpoints": {
            "/": "GET - Root endpoint",
            "/api": "GET - API endpoint",
            "/status": "GET - Status endpoint",
            "auth": {
                "/login": "POST - User login",
                "/register": "POST - User registration"
            }

        }
    });
    rs(200, data)
}