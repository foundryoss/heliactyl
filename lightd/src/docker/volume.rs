use bollard::{
    volume::{CreateVolumeOptions, ListVolumesOptions, RemoveVolumeOptions},
    Docker,
};
use tracing::info;

use crate::models::{CreateVolumeRequest, VolumeInfo};

/// Volume management operations
pub struct VolumeManager<'a> {
    client: &'a Docker,
}

impl<'a> VolumeManager<'a> {
    pub fn new(client: &'a Docker) -> Self {
        Self { client }
    }

    /// Create a new volume
    pub async fn create(&self, req: CreateVolumeRequest) -> anyhow::Result<String> {
        let options = CreateVolumeOptions {
            name: req.name.clone(),
            driver: req.driver.unwrap_or_else(|| "local".to_string()),
            driver_opts: req.driver_opts.unwrap_or_default(),
            labels: req.labels.unwrap_or_default(),
        };

        let volume = self.client.create_volume(options).await?;
        info!("Created volume: {}", req.name);
        Ok(volume.name)
    }

    /// List all volumes
    pub async fn list(&self) -> anyhow::Result<Vec<VolumeInfo>> {
        let options = ListVolumesOptions::<String> {
            ..Default::default()
        };

        let response = self.client.list_volumes(Some(options)).await?;
        
        let mut result = Vec::new();
        if let Some(volumes) = response.volumes {
            for volume in volumes {
                result.push(VolumeInfo {
                    name: volume.name,
                    driver: volume.driver,
                    mountpoint: volume.mountpoint,
                    created_at: volume.created_at.unwrap_or_default(),
                    labels: volume.labels,
                });
            }
        }

        Ok(result)
    }

    /// Remove a volume
    pub async fn remove(&self, name: &str) -> anyhow::Result<()> {
        let options = RemoveVolumeOptions { force: true };
        self.client.remove_volume(name, Some(options)).await?;
        info!("Removed volume: {}", name);
        Ok(())
    }
}