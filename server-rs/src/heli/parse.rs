use serde_json::Value;
use std::fs;
use uuid::Uuid;
use rand::Rng;

pub struct HeliConfig {
    data: Value,
}

impl Clone for HeliConfig {
    fn clone(&self) -> Self {
        HeliConfig {
            data: self.data.clone(),
        }
    }
}

impl HeliConfig {
    /// Parse a .heli configuration file
    pub fn parse(file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(file_path)?;
        
        // Check if we need to update the .heli file (if it has auto-generation directives)
        let needs_update = contents.contains("uuid") || 
                          contents.contains("number") || 
                          contents.contains("text") || 
                          contents.contains("auto-helia");
        
        let mut updated_contents = contents.clone();
        if needs_update {
            updated_contents = replace_auto_generation_in_heli(&contents);
            fs::write(file_path, &updated_contents)?;
        }
        
        // Parse the updated .heli content
        let data = parse_heli(&updated_contents)?;
        
        // Cache the JSON version
        fs::create_dir_all("data")?;
        fs::write("data/config.json", serde_json::to_string_pretty(&data)?)?;
        
        Ok(HeliConfig { data })
    }

    /// Get a value from the config using dot notation
    pub fn get(&self, path: &str) -> Option<Value> {
        let keys: Vec<&str> = path.split('.').collect();
        let mut current = &self.data;
        
        for key in keys {
            current = current.get(key)?;
        }
        
        Some(current.clone())
    }

    /// Get a value as a string
    pub fn get_string(&self, path: &str) -> Option<String> {
        self.get(path).and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    /// Get a value as an integer
    pub fn get_int(&self, path: &str) -> Option<i64> {
        self.get(path).and_then(|v| {
            // Try as i64 first, then try parsing string
            v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
        })
    }

    /// Get servers configuration
    pub fn get_servers(&self) -> Option<std::collections::HashMap<String, ServerConfig>> {
        let servers_obj = self.get("servers")?;
        let obj = servers_obj.as_object()?;
        
        let mut servers = std::collections::HashMap::new();
        
        for (name, config) in obj.iter() {
            let host = config.get("host")?.as_str()?.to_string();
            let port = config.get("port")?
                .as_i64()
                .or_else(|| config.get("port")?.as_str()?.parse::<i64>().ok())? as u16;
            
            servers.insert(name.clone(), ServerConfig { host, port });
        }
        
        Some(servers)
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}



/// Parse .heli format into JSON
fn parse_heli(content: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let mut json_str = content.to_string();

    // Remove control characters except newlines and tabs
    json_str = json_str.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t' || *c == '\r')
        .collect();

    // Replace = with :
    json_str = json_str.replace(" = ", ": ");

    // Add commas between object entries that are missing them
    let re_missing_commas = regex::Regex::new(r#"([\\"\}])\s*\n\s*([a-zA-Z0-9_\-]+\s*[:=])"#).unwrap();
    json_str = re_missing_commas.replace_all(&json_str, "$1,\n$2").to_string();

    // Replace {value} with "value" for string/number values, but keep nested objects as is
    // This is a more careful regex that won't match on nested objects
    let re_val = regex::Regex::new(r":\s*\{([^{}\n]+)\}").unwrap();
    json_str = re_val.replace_all(&json_str, ": \"$1\"").to_string();

    // Replace keys without quotes with quoted keys
    let re = regex::Regex::new(r#"(\s*)([a-zA-Z0-9_\-]+)\s*:"#).unwrap();
    json_str = re.replace_all(&json_str, "$1\"$2\":").to_string();

    // Remove trailing commas before closing braces
    let re_comma = regex::Regex::new(r",(\s*[}\]])").unwrap();
    json_str = re_comma.replace_all(&json_str, "$1").to_string();

    // Clean up any remaining control characters before JSON parsing
    json_str = json_str.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t' || *c == '\r' || *c == ' ')
        .collect();

    // Parse as JSON
    let data: Value = match serde_json::from_str(&json_str) {
        Ok(data) => data,
        Err(e) => {
            // For debugging - write the transformed JSON to a file
            let _ = fs::write("data/debug_transformed.json", &json_str);
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse .heli format: {} - Debug JSON written to data/debug_transformed.json", e)
            )));
        }
    };
    
    Ok(data)
}


/// Replace auto-generation directives in the .heli file itself
fn replace_auto_generation_in_heli(content: &str) -> String {
    let mut result = content.to_string();
    
    // Replace auto-helia[uuid]
    let re_auto_helia = regex::Regex::new(r"\{auto-helia\[uuid\]\}").unwrap();
    result = re_auto_helia.replace_all(&result, |_: &regex::Captures| {
        format!("{{auto-helia-{}}}", Uuid::new_v4())
    }).to_string();
    
    // Replace {uuid}
    let re_uuid = regex::Regex::new(r"\{uuid\}").unwrap();
    result = re_uuid.replace_all(&result, |_: &regex::Captures| {
        format!("{{{}}}", Uuid::new_v4())
    }).to_string();
    
    // Replace {number}
    let re_number = regex::Regex::new(r"\{number\}").unwrap();
    result = re_number.replace_all(&result, |_: &regex::Captures| {
        format!("{{{}}}", rand::random::<u32>())
    }).to_string();
    
    // Replace {text}
    let re_text = regex::Regex::new(r"\{text\}").unwrap();
    result = re_text.replace_all(&result, |_: &regex::Captures| {
        format!("{{{}}}", generate_random_text(16))
    }).to_string();
    
    result
}

/// Generate random text
fn generate_random_text(length: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    
    (0..length)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}