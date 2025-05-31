use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;

#[derive(Deserialize, Debug, Clone)]
pub struct FeatureFlags {
    flags: HashMap<String, bool>,
}

impl FeatureFlags {
    pub fn new(flags: HashMap<String, bool>) -> Self {
        FeatureFlags { flags }
    }

    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let flags: HashMap<String, bool> = serde_json::from_str(&contents)?;
        Ok(FeatureFlags::new(flags))
    }

    pub fn is_feature_enabled(&self, feature_name: &str) -> bool {
        self.flags.get(feature_name).copied().unwrap_or(false)
    }
}

pub fn load_feature_flags(path: &str) -> Result<Arc<FeatureFlags>, Box<dyn std::error::Error>> {
    let feature_flags = FeatureFlags::load_from_file(path)?;
    Ok(Arc::new(feature_flags))
}
