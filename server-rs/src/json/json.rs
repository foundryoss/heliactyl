use axum::response::Json;
use serde_json::{json, Value};
use std::{collections::HashMap};

use crate::env;

// Helper function to return a structured response
// Now we can use a hash map and life is nicer
pub fn key_value<S: Into<String>>(status: u16, data: HashMap<S, S>) -> Json<Value> {
    let data_json: Value = data
        .into_iter()
        .map(|(key, value)| (key.into(), Value::String(value.into())))
        .collect();

    Json(json!({
        "status": status,
        "data": data_json
    }))
}


pub fn rs(status: u16, data: Value) -> Json<Value> {
    Json(json!({
        "status": status,
        "metadata": {
            "server": "heliactyl-rs",
            "version": "0.1.0",
            "status": {
                "uptime_seconds": env::uptime::UptimeTracker::current()
            }
        },
        "data": data
    }))
}