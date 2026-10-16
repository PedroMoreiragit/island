//! Save/load em JSON. O mundo é regenerado pela seed; só o estado que muda é salvo.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::building::Structure;
use crate::crafting::CraftQueue;
use crate::creatures::CreatureSave;
use crate::inventory::Stack;
use crate::progression::Progression;
use crate::vitals::Vitals;
use crate::world::BagKind;

pub const SAVE_VERSION: u32 = 1;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SaveData {
    pub version: u32,
    pub seed: u64,
    pub time: f32,
    pub player_pos: [f32; 2],
    pub facing_left: bool,
    pub vitals: Vitals,
    pub inventory: Vec<Option<Stack>>,
    pub hotbar_selected: usize,
    /// Apenas nós alterados: (índice, hp, respawn_timer).
    pub nodes: Vec<(usize, u32, f32)>,
    pub bags: Vec<BagSave>,
    /// Campos novos usam `default` para saves antigos continuarem válidos.
    #[serde(default)]
    pub structures: Vec<Structure>,
    #[serde(default)]
    pub craft_queue: CraftQueue,
    /// `None` em saves anteriores à v0.4 (a fauna é gerada do zero).
    #[serde(default)]
    pub creatures: Option<Vec<CreatureSave>>,
    /// `None` em saves anteriores à v0.6 (receitas antigas liberadas, nível 1).
    #[serde(default)]
    pub progression: Option<Progression>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct BagSave {
    pub pos: [f32; 2],
    pub stacks: Vec<Stack>,
    pub kind: BagKind,
    pub ttl: f32,
}

/// Grava de forma atômica (arquivo temporário + rename) para não corromper o save.
pub fn write(path: &Path, data: &SaveData) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(data).map_err(io::Error::other)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

/// `Ok(None)` se não existe save; `Err` se existe mas é inválido ou de outra versão.
pub fn read(path: &Path) -> io::Result<Option<SaveData>> {
    let json = match fs::read_to_string(path) {
        Ok(json) => json,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let data: SaveData = serde_json::from_str(&json).map_err(io::Error::other)?;
    if data.version != SAVE_VERSION {
        return Err(io::Error::other(format!(
            "versão de save {} não suportada (esperada {SAVE_VERSION})",
            data.version
        )));
    }
    Ok(Some(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::ItemId;

    #[test]
    fn round_trip() {
        let data = SaveData {
            version: SAVE_VERSION,
            seed: 42,
            time: 12.5,
            player_pos: [100.0, 200.0],
            facing_left: true,
            vitals: Vitals::full(),
            inventory: vec![Some(Stack::new(ItemId::StonePickaxe, 1)), None],
            hotbar_selected: 1,
            nodes: vec![(3, 0, 10.0)],
            bags: vec![BagSave {
                pos: [1.0, 2.0],
                stacks: vec![Stack::new(ItemId::Berry, 4)],
                kind: BagKind::Death,
                ttl: 99.0,
            }],
            structures: vec![Structure::new(crate::building::StructureKind::StorageBox, (1, 2))],
            craft_queue: CraftQueue::default(),
            creatures: Some(vec![CreatureSave::from(
                &crate::creatures::Creature::new(crate::creatures::Species::Raptor, macroquad::math::vec2(5.0, 6.0))
                    .with_level(3),
            )]),
            progression: Some(Progression::default()),
        };
        let path = std::env::temp_dir().join(format!("island-test-{}/save.json", std::process::id()));
        write(&path, &data).unwrap();
        assert_eq!(read(&path).unwrap(), Some(data));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn missing_file_is_none() {
        assert_eq!(read(Path::new("nao/existe/save.json")).unwrap(), None);
    }

    #[test]
    fn v02_save_without_new_fields_still_loads() {
        let json = r#"{"version":1,"seed":1,"time":0.0,"player_pos":[0.0,0.0],"facing_left":false,
            "vitals":{"health":100.0,"food":100.0,"water":100.0,"stamina":100.0,"exhausted":false},
            "inventory":[],"hotbar_selected":0,"nodes":[],"bags":[]}"#;
        let data: SaveData = serde_json::from_str(json).unwrap();
        assert!(data.structures.is_empty() && data.craft_queue.jobs.is_empty());
        assert!(data.creatures.is_none());
        assert!(data.progression.is_none());
    }
}
