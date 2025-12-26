use crate::models::tenant::{Resources, UsedResources};
use crate::heli::parse::HeliConfig;

pub fn sum_used_resources(servers: &[(i64, i64, i64)]) -> UsedResources {
    let mut memory_mb = 0;
    let mut disk_mb = 0;
    let mut cpu_percent = 0;

    for (mem, disk, cpu) in servers {
        memory_mb += mem;
        disk_mb += disk;
        cpu_percent += cpu;
    }

    UsedResources {
        memory_mb,
        disk_mb,
        cpu_percent,
        servers: servers.len(),
    }
}

pub fn remaining_resources(
    package: &Resources,
    extra: &Resources,
    used: &UsedResources,
) -> Resources {
    Resources {
        memory_mb: (package.memory_mb + extra.memory_mb - used.memory_mb).max(0),
        disk_mb: (package.disk_mb + extra.disk_mb - used.disk_mb).max(0),
        cpu_percent: (package.cpu_percent + extra.cpu_percent - used.cpu_percent).max(0),
        server_slots: (package.server_slots + extra.server_slots - used.servers as i64).max(0),
    }
}

pub fn get_package_resources(package_id: &str) -> Resources {
    // Try to load from config.heli
    if let Ok(heli_config) = HeliConfig::parse("./config.heli") {
        // Try to get package-specific config first (e.g., packages.free, packages.basic)
        let package_path = format!("packages.{}", package_id);
        
        let memory_mb = heli_config.get_int(&format!("{}.memory_mb", package_path))
            .or_else(|| heli_config.get_int("packages.default.memory_mb"))
            .unwrap_or(2048);
        
        let disk_mb = heli_config.get_int(&format!("{}.disk_mb", package_path))
            .or_else(|| heli_config.get_int("packages.default.disk_mb"))
            .unwrap_or(10240);
        
        let cpu_percent = heli_config.get_int(&format!("{}.cpu_percent", package_path))
            .or_else(|| heli_config.get_int("packages.default.cpu_percent"))
            .unwrap_or(100);
        
        let server_slots = heli_config.get_int(&format!("{}.server_slots", package_path))
            .or_else(|| heli_config.get_int("packages.default.server_slots"))
            .unwrap_or(2);
        
        return Resources {
            memory_mb,
            disk_mb,
            cpu_percent,
            server_slots,
        };
    }
    
    // Fallback to hardcoded defaults if config can't be loaded
    match package_id {
        "free" => Resources {
            memory_mb: 2048,
            disk_mb: 10240,
            cpu_percent: 100,
            server_slots: 2,
        },
        "basic" => Resources {
            memory_mb: 4096,
            disk_mb: 20480,
            cpu_percent: 200,
            server_slots: 5,
        },
        "premium" => Resources {
            memory_mb: 8192,
            disk_mb: 51200,
            cpu_percent: 400,
            server_slots: 10,
        },
        _ => Resources {
            memory_mb: 2048,
            disk_mb: 10240,
            cpu_percent: 100,
            server_slots: 2,
        },
    }
}
