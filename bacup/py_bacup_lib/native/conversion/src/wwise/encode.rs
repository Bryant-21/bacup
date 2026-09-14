//! Low-level Wwise-toolchain primitives: FNV-1 32-bit hashing (bank/media/event
//! IDs), the WWED GUID codec, event-name sanitization, and the
//! `{in}`/`{out}` command-template + subprocess plumbing used by `build_bank`.

use std::path::Path;
use std::process::Command;

/// FNV-1 (not FNV-1a) 32-bit hash of the lowercased input, used by Wwise for
/// bank ShortName -> bank ID and event name -> event ShortID.
pub fn fnv1_32(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5; // 2166136261
    for b in s.to_lowercase().as_bytes() {
        h = h.wrapping_mul(0x0100_0193); // 16777619
        h ^= *b as u32;
    }
    h
}

/// `FO4SF_` + the source path uppercased, every non-alphanumeric byte mapped
/// to `_`. The fixed prefix also satisfies the engine's `[A-Za-z0-9_]`,
/// no-leading-digit event-name rule.
pub fn sanitize_event_name(source_rel_path: &str) -> String {
    let sanitized: String = source_rel_path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("FO4SF_{sanitized}")
}

/// A deterministic, well-formed Wwise object GUID string (no braces) derived
/// from `seed`. Used to author `.wwu` object IDs; not the WWED byte codec.
pub fn wwise_object_guid(seed: &str) -> String {
    let hash = blake3::hash(seed.as_bytes());
    let bytes = hash.as_bytes();
    let hex: String = bytes[..16].iter().map(|b| format!("{b:02X}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// WWED stores a Wwise object GUID as two little-endian u64s (hi‖lo), not a
/// Microsoft-ordered GUID. Parses the string form into those bytes, both for
/// authoring `.wwu` object IDs and for decoding the generated bank JSON's
/// `IncludedEvents[].GUID`.
pub fn guid_string_to_bytes(s: &str) -> Result<[u8; 16], String> {
    let cleaned: String = s
        .chars()
        .filter(|c| *c != '{' && *c != '}' && *c != '-')
        .collect();
    if cleaned.len() != 32 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("invalid Wwise object GUID string: {s}"));
    }
    let hi = u64::from_str_radix(&cleaned[0..16], 16).map_err(|e| e.to_string())?;
    let lo = u64::from_str_radix(&cleaned[16..32], 16).map_err(|e| e.to_string())?;
    let mut out = [0u8; 16];
    out[0..8].copy_from_slice(&hi.to_le_bytes());
    out[8..16].copy_from_slice(&lo.to_le_bytes());
    Ok(out)
}

/// Substitute the `{in}`/`{out}` placeholders in a command template.
pub fn substitute_template(cmd: &[String], in_path: &Path, out_path: &Path) -> Vec<String> {
    let in_str = in_path.to_string_lossy();
    let out_str = out_path.to_string_lossy();
    cmd.iter()
        .map(|arg| arg.replace("{in}", &in_str).replace("{out}", &out_str))
        .collect()
}

fn is_batch_script(exe: &str) -> bool {
    let lower = exe.to_lowercase();
    lower.ends_with(".cmd") || lower.ends_with(".bat")
}

/// Fail loudly (never panic) if a command's executable does not exist on
/// disk, naming the missing tool in the error.
pub fn ensure_tool_exists(exe: &str) -> Result<(), String> {
    if Path::new(exe).exists() {
        Ok(())
    } else {
        Err(format!("missing tool: {exe}"))
    }
}

/// Run a fully-built argv, probing `argv[0]` first. `.cmd`/`.bat` stubs are
/// routed through `cmd /C` since Windows `CreateProcess` cannot launch batch
/// files directly.
pub fn run_command(argv: &[String]) -> Result<(), String> {
    let exe = argv.first().ok_or_else(|| "empty command".to_string())?;
    ensure_tool_exists(exe)?;
    let mut cmd = if is_batch_script(exe) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(exe);
        c
    } else {
        Command::new(exe)
    };
    cmd.args(&argv[1..]);
    let output = cmd
        .output()
        .map_err(|e| format!("failed to run {exe}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{exe} exited with {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1_32_pinned_value() {
        assert_eq!(fnv1_32("fallout4_sf"), 3768386522);
    }

    #[test]
    fn fnv1_32_is_case_insensitive() {
        assert_eq!(fnv1_32("Fallout4_SF"), fnv1_32("fallout4_sf"));
    }

    #[test]
    fn sanitize_event_name_maps_non_alnum_to_underscore_and_uppercases() {
        assert_eq!(
            sanitize_event_name("music/Track 01.xwm"),
            "FO4SF_MUSIC_TRACK_01_XWM"
        );
    }

    #[test]
    fn sanitize_event_name_never_leads_with_digit_due_to_fixed_prefix() {
        let name = sanitize_event_name("123beep.wav");
        assert!(name.starts_with("FO4SF_"));
        assert!(!name.chars().next().unwrap().is_ascii_digit());
    }

    #[test]
    fn guid_roundtrip_matches_r6_worked_example() {
        // WwiseEvent_AMB_RL083_MovingRailHook Start decodes to this GUID.
        let bytes: [u8; 16] = [42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 143];
        // Round-trip through the string form must reproduce the same bytes.
        let hi = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let lo = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let s = format!("{hi:016X}{lo:016X}");
        let guid = format!(
            "{}-{}-{}-{}-{}",
            &s[0..8],
            &s[8..12],
            &s[12..16],
            &s[16..20],
            &s[20..32]
        );
        assert_eq!(guid_string_to_bytes(&guid).unwrap(), bytes);
    }

    #[test]
    fn guid_string_to_bytes_accepts_braces_and_dashes() {
        let a = guid_string_to_bytes("E563D9FB-C52A-492A-8FF7-6623CB5CDC31").unwrap();
        let b = guid_string_to_bytes("{E563D9FB-C52A-492A-8FF7-6623CB5CDC31}").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn guid_string_to_bytes_rejects_malformed_input() {
        assert!(guid_string_to_bytes("not-a-guid").is_err());
    }

    #[test]
    fn substitute_template_replaces_both_placeholders() {
        let cmd = vec![
            "tool.exe".to_string(),
            "{in}".to_string(),
            "{out}".to_string(),
        ];
        let out = substitute_template(&cmd, Path::new("a.xwm"), Path::new("b.wav"));
        assert_eq!(out, vec!["tool.exe", "a.xwm", "b.wav"]);
    }

    #[test]
    fn ensure_tool_exists_names_missing_tool() {
        let err = ensure_tool_exists("Z:\\does\\not\\exist\\tool.exe").unwrap_err();
        assert!(err.contains("Z:\\does\\not\\exist\\tool.exe"), "{err}");
    }
}
