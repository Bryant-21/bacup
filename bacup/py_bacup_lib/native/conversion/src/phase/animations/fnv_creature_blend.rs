#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SourceBlendFrames {
    pub blend_in: u8,
    pub blend_out: u8,
}

pub(crate) fn source_blend_frames<'a>(keys: impl IntoIterator<Item = &'a str>) -> Result<SourceBlendFrames, String> {
    let (mut blend, mut blend_in, mut blend_out) = (0, 0, 0);
    for line in keys.into_iter().flat_map(|key| key.split(['\r', '\n'])) {
        for (prefix, destination) in [("Blend:", &mut blend), ("BlendIn:", &mut blend_in), ("BlendOut:", &mut blend_out)] {
            if let Some(start) = line.get(..prefix.len())
                && start.eq_ignore_ascii_case(prefix)
            {
                *destination = source_atoi_byte(&line[prefix.len()..])?;
                break;
            }
        }
    }
    Ok(SourceBlendFrames { blend_in: blend.max(blend_in), blend_out: blend.max(blend_out) })
}

fn source_atoi_byte(value: &str) -> Result<u8, String> {
    let text = value.trim_start_matches(|character: char| character.is_ascii_whitespace());
    let sign_length = usize::from(text.starts_with('+') || text.starts_with('-'));
    let digit_count = text[sign_length..].bytes().take_while(u8::is_ascii_digit).count();
    if digit_count == 0 { return Ok(0); }
    text[..sign_length + digit_count].parse::<i32>().map(|value| value as u8)
        .map_err(|_| format!("source animation blend key has overflowing integer {value:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_blend_keys_preserve_native_case_prefix_and_byte_semantics() {
        assert_eq!(source_blend_frames(["Blend:3/", "bLeNdIn: 1", "BlendOut:8\r\n"]).unwrap(), SourceBlendFrames { blend_in: 3, blend_out: 8 });
        assert_eq!(source_blend_frames(["Blend:6\r\nBlendIn:20\nBlendOut:2", "Blend:1"]).unwrap(), SourceBlendFrames { blend_in: 20, blend_out: 2 });
        assert_eq!(source_blend_frames(["BlendI: 9", "Blendn:1", "Blend :15", " Blend:30"]).unwrap(), SourceBlendFrames::default());
        assert_eq!(source_blend_frames(["Blend:0", "BlendIn:-1", "BlendOut:257"]).unwrap(), SourceBlendFrames { blend_in: 255, blend_out: 1 });
        assert_eq!(source_blend_frames(["Blend:15", "Blend:invalid"]).unwrap(), SourceBlendFrames::default());
        assert!(source_blend_frames(["Blend:999999999999"]).is_err());
    }
}
