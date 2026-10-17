use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct SourceSubgraph {
    pub role: String,
    pub perspective: String,
    pub actor_keywords: Vec<String>,
    pub target_keywords: Vec<String>,
    pub graph: String,
    pub animation_paths: Vec<String>,
}

fn list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn safe_relative(value: &str) -> bool {
    let normalized = value.replace('\\', "/");
    !normalized.is_empty()
        && !normalized.contains(':')
        && Path::new(&normalized)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub(super) fn parse_generated_subgraphs(text: &str) -> Result<Vec<SourceSubgraph>, String> {
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    if lines.next()
        != Some(
            "Role\tPerspective\tActor Keyword(s)\tTarget Keyword(s)\tBehavior Path\tAnimation Path(s)",
        )
    {
        return Err("Starfield generated subgraph table has an unknown header".into());
    }
    let mut rows = Vec::new();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 6 || !matches!(fields[1], "1st" | "3rd") || !safe_relative(fields[4]) {
            return Err(format!(
                "Starfield generated subgraph row {} is invalid",
                index + 2
            ));
        }
        let animation_paths = list(fields[5]);
        if animation_paths.is_empty() || animation_paths.iter().any(|path| !safe_relative(path)) {
            return Err(format!(
                "Starfield generated subgraph row {} has invalid animation paths",
                index + 2
            ));
        }
        rows.push(SourceSubgraph {
            role: fields[0].into(),
            perspective: fields[1].into(),
            actor_keywords: list(fields[2]),
            target_keywords: list(fields[3]),
            graph: fields[4].into(),
            animation_paths,
        });
    }
    Ok(rows)
}

fn subset(required: &[String], available: &[String]) -> bool {
    required.iter().all(|keyword| {
        available
            .iter()
            .any(|actual| actual.eq_ignore_ascii_case(keyword))
    })
}

pub(super) fn select_weapon_subgraph<'a>(
    rows: &'a [SourceSubgraph],
    perspective: &str,
    actor_keywords: &[String],
    target_keywords: &[String],
) -> Result<&'a SourceSubgraph, String> {
    let candidates: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.role == "Weapon"
                && row.perspective == perspective
                && subset(&row.actor_keywords, actor_keywords)
                && subset(&row.target_keywords, target_keywords)
        })
        .collect();
    let mut maximal = Vec::new();
    for candidate in &candidates {
        let dominated = candidates.iter().any(|other| {
            subset(&candidate.actor_keywords, &other.actor_keywords)
                && subset(&candidate.target_keywords, &other.target_keywords)
                && (candidate.actor_keywords.len() < other.actor_keywords.len()
                    || candidate.target_keywords.len() < other.target_keywords.len())
        });
        if !dominated && !maximal.contains(candidate) {
            maximal.push(*candidate);
        }
    }
    match maximal.as_slice() {
        [selected] => Ok(selected),
        [] => Err(format!(
            "Starfield has no generated Weapon {perspective} subgraph for the source keywords"
        )),
        _ => Err(format!(
            "Starfield has {} incomparable generated Weapon {perspective} subgraphs",
            maximal.len()
        )),
    }
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ResolvedSourceClip {
    pub tag: String,
    pub source_af: String,
    pub source_afx: String,
    pub directory_index: usize,
}

pub(super) fn resolve_source_clip(
    row: &SourceSubgraph,
    tag: &str,
    meshes: &Path,
) -> Result<ResolvedSourceClip, String> {
    if tag.is_empty()
        || !tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("Starfield source clip tag is not a filename stem".into());
    }
    for (directory_index, directory) in row.animation_paths.iter().enumerate() {
        let stem = format!("{}/{}", directory.replace('\\', "/"), tag);
        let source_af = format!("{stem}.af");
        if meshes.join(&source_af).is_file() {
            let source_afx = format!("{stem}.afx");
            if !meshes.join(&source_afx).is_file() {
                return Err(format!(
                    "Starfield source clip {source_af} has no AFX contract"
                ));
            }
            return Ok(ResolvedSourceClip {
                tag: tag.into(),
                source_af,
                source_afx,
                directory_index,
            });
        }
    }
    Err(format!(
        "Starfield source tag {tag} is absent from its generated animation paths"
    ))
}

#[derive(Debug, Serialize)]
pub(super) struct SourceCacheTagMap {
    pub version: u8,
    pub graph: String,
    pub tags: BTreeMap<String, String>,
    pub remaining_offset_bytes: usize,
}

// CK 1.15.222:4CF1C70 reads V4+ graph and tag pairs before offset data; 4D0B4D0 reads u8-sized strings.
pub(super) fn parse_cache_tag_map(bytes: &[u8]) -> Result<SourceCacheTagMap, String> {
    let line_end = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("Starfield clip cache has no version line")?;
    let version = match &bytes[..line_end] {
        b"V4" | b"V4\r" => 4,
        b"V5" | b"V5\r" => 5,
        _ => return Err("Starfield clip cache version is unsupported".into()),
    };
    let mut offset = line_end + 1;
    fn string(bytes: &[u8], offset: &mut usize) -> Result<String, String> {
        let length = *bytes
            .get(*offset)
            .ok_or("Starfield clip cache string is truncated")? as usize;
        *offset += 1;
        if length == 0 {
            return Ok(String::new());
        }
        let value = bytes
            .get(*offset..*offset + length)
            .ok_or("Starfield clip cache string is truncated")?;
        *offset += length;
        let value = value
            .strip_suffix(&[0])
            .ok_or("Starfield clip cache string is not terminated")?;
        if value.contains(&0) {
            return Err("Starfield clip cache string has embedded NUL".into());
        }
        String::from_utf8(value.to_vec())
            .map_err(|_| "Starfield clip cache string is not UTF-8".into())
    }
    let graph = string(bytes, &mut offset)?;
    if !safe_relative(&graph) {
        return Err("Starfield clip cache graph path is invalid".into());
    }
    let count = u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("Starfield clip cache count is truncated")?
            .try_into()
            .unwrap(),
    );
    offset += 4;
    if count > 10_000 {
        return Err("Starfield clip cache tag count exceeds source contract".into());
    }
    let mut tags = BTreeMap::new();
    for _ in 0..count {
        let tag = string(bytes, &mut offset)?;
        let path = string(bytes, &mut offset)?;
        if tag.is_empty()
            || !safe_relative(&path)
            || tags.insert(tag.to_ascii_lowercase(), path).is_some()
        {
            return Err("Starfield clip cache tag is empty, duplicated or invalid".into());
        }
    }
    Ok(SourceCacheTagMap {
        version,
        graph,
        tags,
        remaining_offset_bytes: bytes.len() - offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEADER: &str = "Role\tPerspective\tActor Keyword(s)\tTarget Keyword(s)\tBehavior Path\tAnimation Path(s)\n";

    #[test]
    fn source_keywords_select_specific_rows_without_order_tiebreaks() {
        let text = format!(
            "{HEADER}Weapon\t1st\t\tPistol\tBase.agx\tbase\nWeapon\t1st\t\tPistol, Stinger\tPistol.agx\tstinger, base\nWeapon\t1st\tPlayer\tPistol\tPlayer.agx\tplayer, base\n"
        );
        let rows = parse_generated_subgraphs(&text).unwrap();
        let target = vec!["Pistol".into(), "Stinger".into()];
        assert_eq!(
            select_weapon_subgraph(&rows, "1st", &[], &target)
                .unwrap()
                .graph,
            "Pistol.agx"
        );
        assert!(select_weapon_subgraph(&rows, "1st", &["Player".into()], &target).is_err());
        assert!(select_weapon_subgraph(&rows, "3rd", &[], &target).is_err());
        assert!(parse_generated_subgraphs(&text.replace("stinger, base", "../stinger")).is_err());
    }

    #[test]
    fn cache_prefix_preserves_resolved_paths_and_rejects_truncation() {
        fn string(bytes: &mut Vec<u8>, value: &str) {
            bytes.push((value.len() + 1) as u8);
            bytes.extend_from_slice(value.as_bytes());
            bytes.push(0);
        }
        let mut bytes = b"V5\n".to_vec();
        string(&mut bytes, "Graphs/Pistol.agx");
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        string(&mut bytes, "FireSingle");
        string(&mut bytes, "Actors/Human/FireSingle");
        let map = parse_cache_tag_map(&bytes).unwrap();
        assert_eq!(map.tags["firesingle"], "Actors/Human/FireSingle");
        for end in [0, 2, 4, bytes.len() - 1] {
            assert!(parse_cache_tag_map(&bytes[..end]).is_err());
        }
        bytes[1] = b'3';
        assert!(parse_cache_tag_map(&bytes).is_err());
    }

    #[test]
    #[ignore = "requires extracted Starfield source generated HumanRace table and clips"]
    fn real_eon_generated_subgraphs_resolve_motion_and_sync_clips() {
        let meshes = PathBuf::from(std::env::var("B21_STARFIELD_MESHES").expect("B21_STARFIELD_MESHES"));
        let rows = parse_generated_subgraphs(
            &std::fs::read_to_string(
                meshes.join("AnimTextData/GeneratedSubgraphData/HumanRace.txt"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(rows.len(), 1607);
        let target = vec!["AnimGripPistol".into(), "AnimStinger".into()];
        let cache_root = meshes.join("AnimTextData/AnimationOffsets");
        let caches: Vec<_> =
            std::fs::read_to_string(cache_root.join("PersistantSubgraphInfoAndOffsetData.txt"))
                .unwrap()
                .lines()
                .map(|name| {
                    (
                        name.to_owned(),
                        parse_cache_tag_map(&std::fs::read(cache_root.join(name)).unwrap())
                            .unwrap(),
                    )
                })
                .collect();
        let mut receipt = Vec::new();
        for (perspective, actor) in [
            ("1st", vec![]),
            ("3rd", vec!["AnimArchetypePlayer".into()]),
            ("3rd", vec![]),
        ] {
            let row = select_weapon_subgraph(&rows, perspective, &actor, &target).unwrap();
            let mut clips = Vec::new();
            let tags = if perspective == "1st" || !actor.is_empty() {
                vec!["FireSingle", "Reload", "Gun_FireSingle"]
            } else {
                vec!["FireSingle", "Reload"]
            };
            for tag in tags {
                let clip = resolve_source_clip(row, tag, meshes).unwrap();
                let af = std::fs::read(meshes.join(&clip.source_af)).unwrap();
                let bones = u16::from_le_bytes(af[42..44].try_into().unwrap());
                let frames = u16::from_le_bytes(af[44..46].try_into().unwrap());
                if tag == "Gun_FireSingle" {
                    assert_eq!(bones, 0);
                    assert!(frames > 0);
                } else {
                    assert!(bones > 0 && frames > 1);
                }
                clips.push(serde_json::json!({"clip":clip,"bones":bones,"last_frame":frames}));
            }
            let matching_caches: Vec<_> = caches
                .iter()
                .filter(|(_, cache)| {
                    cache.graph.eq_ignore_ascii_case(&row.graph)
                        && clips.iter().all(|item| {
                            let tag = item["clip"]["tag"].as_str().unwrap().to_ascii_lowercase();
                            let path = item["clip"]["source_af"]
                                .as_str()
                                .unwrap()
                                .trim_end_matches(".af");
                            cache.tags.get(&tag).is_some_and(|cached| {
                                cached.replace('\\', "/").eq_ignore_ascii_case(path)
                            })
                        })
                })
                .map(|(name, _)| name)
                .collect();
            assert!(
                !matching_caches.is_empty(),
                "generated source resolution disagrees with every shipped clip cache for {perspective} {actor:?}"
            );
            receipt.push(serde_json::json!({"perspective":perspective,"actor_keywords":actor,"subgraph":row,"clips":clips,"matching_shipped_clip_caches":matching_caches}));
        }
        println!("{}", serde_json::to_string(&receipt).unwrap());
        std::fs::write(
            std::env::temp_dir().join("b21_sf_eon_source_subgraphs_receipt.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }
}
