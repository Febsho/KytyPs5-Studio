pub(super) fn steam_vdf_string(out: &mut Vec<u8>, key: &str, value: &str) -> Result<(), String> {
    if key.as_bytes().contains(&0) || value.as_bytes().contains(&0) {
        return Err("Steam shortcut contains a string with an embedded NUL byte".into());
    }
    out.push(1);
    out.extend_from_slice(key.as_bytes());
    out.push(0);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    Ok(())
}

pub(super) fn steam_vdf_int(out: &mut Vec<u8>, key: &str, value: u32) {
    out.push(2);
    out.extend_from_slice(key.as_bytes());
    out.push(0);
    out.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn steam_vdf_key(bytes: &[u8], mut pos: usize) -> Result<(String, usize), String> {
    let start = pos;
    while pos < bytes.len() && bytes[pos] != 0 {
        pos += 1;
    }
    if pos >= bytes.len() {
        return Err("Steam shortcuts file is malformed".into());
    }
    Ok((
        String::from_utf8_lossy(&bytes[start..pos]).into_owned(),
        pos + 1,
    ))
}

pub(super) const STEAM_VDF_MAX_NESTING: usize = 64;

pub(super) fn steam_vdf_skip(bytes: &[u8], kind: u8, pos: usize) -> Result<usize, String> {
    steam_vdf_skip_nested(bytes, kind, pos, 0)
}

pub(super) fn steam_vdf_skip_nested(
    bytes: &[u8],
    kind: u8,
    mut pos: usize,
    depth: usize,
) -> Result<usize, String> {
    match kind {
        0 => loop {
            if depth >= STEAM_VDF_MAX_NESTING {
                return Err("Steam shortcuts file exceeds the supported nesting depth".into());
            }
            let next = *bytes.get(pos).ok_or("Steam shortcuts file is malformed")?;
            pos += 1;
            if next == 8 {
                return Ok(pos);
            }
            let (_, value_pos) = steam_vdf_key(bytes, pos)?;
            pos = steam_vdf_skip_nested(bytes, next, value_pos, depth + 1)?;
        },
        1 | 5 => {
            if kind == 1 {
                while pos < bytes.len() && bytes[pos] != 0 {
                    pos += 1;
                }
                if pos >= bytes.len() {
                    return Err("Steam shortcuts file is malformed".into());
                }
                Ok(pos + 1)
            } else {
                while pos + 1 < bytes.len() && (bytes[pos] != 0 || bytes[pos + 1] != 0) {
                    pos += 2;
                }
                if pos + 1 >= bytes.len() {
                    return Err("Steam shortcuts file is malformed".into());
                }
                Ok(pos + 2)
            }
        }
        2 | 3 | 4 | 6 => pos
            .checked_add(4)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "Steam shortcuts file is malformed".into()),
        7 => pos
            .checked_add(8)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "Steam shortcuts file is malformed".into()),
        _ => Err("Steam shortcuts file contains an unsupported value".into()),
    }
}

pub(super) fn steam_root_end(bytes: &[u8]) -> Result<(usize, u32), String> {
    if bytes.get(..11) != Some(b"\0shortcuts\0") || bytes.get(11) != Some(&0) {
        return Err("Steam shortcuts.vdf has an unexpected format".into());
    }
    let mut pos = 12;
    let mut max_index = None;
    loop {
        let kind = *bytes.get(pos).ok_or("Steam shortcuts.vdf is incomplete")?;
        if kind == 8 {
            return Ok((pos, max_index.unwrap_or(0)));
        }
        // Steam writes shortcut entries as an index key followed directly by
        // the entry fields (without the usual object type byte).
        let (key, mut cursor) = steam_vdf_key(bytes, pos)?;
        if let Ok(index) = key.parse::<u32>() {
            max_index = Some(max_index.map_or(index, |previous: u32| previous.max(index)));
        }
        loop {
            let field_kind = *bytes
                .get(cursor)
                .ok_or("Steam shortcut entry is incomplete")?;
            cursor += 1;
            if field_kind == 8 {
                pos = cursor;
                break;
            }
            let (_, value_pos) = steam_vdf_key(bytes, cursor)?;
            cursor = steam_vdf_skip(bytes, field_kind, value_pos)?;
        }
    }
}

pub(super) fn steam_next_entry_index(root_end: usize, max_index: u32) -> Result<u32, String> {
    if max_index == 0 && root_end == 12 {
        Ok(0)
    } else {
        max_index
            .checked_add(1)
            .ok_or_else(|| "Steam shortcuts.vdf has no available entry index".into())
    }
}

pub(super) fn steam_vdf_string_value(
    bytes: &[u8],
    mut pos: usize,
) -> Result<(String, usize), String> {
    let start = pos;
    while pos < bytes.len() && bytes[pos] != 0 {
        pos += 1;
    }
    if pos >= bytes.len() {
        return Err("Steam shortcuts file is malformed".into());
    }
    let value = std::str::from_utf8(&bytes[start..pos])
        .map_err(|_| "Steam shortcuts file contains invalid UTF-8")?
        .to_string();
    Ok((value, pos + 1))
}

pub(super) fn steam_upsert_shortcut(
    bytes: &mut Vec<u8>,
    app_name: &str,
    replacement: &[u8],
) -> Result<(), String> {
    let (root_end, _) = steam_root_end(bytes)?;
    let mut pos = 12;
    let mut kept = Vec::with_capacity(bytes.len() + replacement.len());
    kept.extend_from_slice(&bytes[..12]);
    let mut inserted = false;
    while pos < root_end {
        let start = pos;
        let (key, value_pos) = steam_vdf_key(bytes, pos)?;
        let mut cursor = value_pos;
        let mut found_name = None;
        loop {
            let field_kind = *bytes
                .get(cursor)
                .ok_or("Steam shortcut entry is incomplete")?;
            if field_kind == 8 {
                cursor += 1;
                break;
            }
            cursor += 1;
            let (field, field_value_pos) = steam_vdf_key(bytes, cursor)?;
            if field_kind == 1 && field == "AppName" {
                let (value, next) = steam_vdf_string_value(bytes, field_value_pos)?;
                found_name = Some(value);
                cursor = next;
            } else {
                cursor = steam_vdf_skip(bytes, field_kind, field_value_pos)?;
            }
        }
        let entry_matches = found_name.as_deref() == Some(app_name);
        if entry_matches {
            if !inserted {
                kept.extend_from_slice(key.as_bytes());
                kept.push(0);
                kept.extend_from_slice(&replacement[3..]);
                inserted = true;
            }
        } else {
            kept.extend_from_slice(&bytes[start..cursor]);
        }
        pos = cursor;
    }
    if !inserted {
        let (_, max_index) = steam_root_end(bytes)?;
        let index = steam_next_entry_index(root_end, max_index)?;
        kept.extend_from_slice(index.to_string().as_bytes());
        kept.push(0);
        kept.extend_from_slice(&replacement[3..]);
    }
    kept.extend_from_slice(&bytes[root_end..]);
    *bytes = kept;
    Ok(())
}

pub(super) fn steam_crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
