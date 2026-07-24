use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MATERIALIZED_NPC_FACEGEN_MANIFEST: &str = ".bacup/materialized_npc_facegen.json";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FacegenAlias {
    pub source_plugin: String,
    pub source_local: u32,
    pub target_plugin: String,
    pub target_local: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct FacegenAliasManifest {
    pub aliases: Vec<FacegenAlias>,
    #[serde(default)]
    pub materializations: Vec<MaterializedNpcEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct MaterializedNpcEntry {
    pub source_actor_plugin: String,
    pub source_actor_local: u32,
    pub traits_source_plugin: String,
    pub traits_source_local: u32,
    pub clone_local: u32,
}

pub fn manifest_path(mod_path: &Path) -> PathBuf {
    mod_path.join(MATERIALIZED_NPC_FACEGEN_MANIFEST)
}
