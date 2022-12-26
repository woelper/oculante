use anyhow::{anyhow, Result};
use notan::prelude::KeyCode;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::DisplayFromStr;
use std::fs::File;
use std::collections::{HashSet, HashMap};
use crate::shortcuts::*;




// pub fn import_keybindings(bindings: &Shortcuts) -> HashMap<InputEvent, HashSet<u32>>{
//     bindings.clone()
//     .into_iter()
//     .map(|(evt, keys)| (evt, keys.into_iter().map(|k| k as u32).collect::<HashSet<u32>>()))
//     .collect()   
// }

// pub fn export_keybindings(bindings: &HashMap<InputEvent, HashSet<u32>>) -> Shortcuts{
//     bindings.clone()
//     .into_iter()
//     .map(|(evt, keys)| (evt, keys.into_iter().map(|k| k.into()).collect::<HashSet<KeyCode>>()))
//     .collect()   
// }


// #[serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct PersistentSettings {
    pub accent_color: [u8; 3],
    pub vsync: bool,
    pub shortcuts: Shortcuts,
}

impl Default for PersistentSettings {
    fn default() -> Self {
        PersistentSettings {
            accent_color: [255, 0, 75],
            vsync: true,
            shortcuts: Shortcuts::default_keys(),
        }
    }
}

impl PersistentSettings {
    pub fn load() -> Result<Self> {
        let local_dir = dirs::data_local_dir().ok_or(anyhow!("Can't getlocal dir"))?;
        let f = File::open(local_dir.join(".oculante"))?;
        Ok(serde_json::from_reader::<_, PersistentSettings>(f)?)
    }

    pub fn save(&self) -> Result<()> {
        let local_dir = dirs::data_local_dir().ok_or(anyhow!("Can't getlocal dir"))?;
        let f = File::create(local_dir.join(".oculante"))?;
        Ok(serde_json::to_writer_pretty(f, self)?)
    }
}
