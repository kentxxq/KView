pub(crate) fn detect_line_ending(content: &str) -> &'static str {
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut cr = 0usize;
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                crlf += 1;
                i += 2;
                continue;
            } else {
                cr += 1;
            }
        } else if bytes[i] == b'\n' {
            lf += 1;
        }
        i += 1;
    }

    if crlf >= lf && crlf >= cr && crlf > 0 {
        "crlf"
    } else if cr > lf && cr > 0 {
        "cr"
    } else {
        "lf"
    }
}

pub(crate) enum DecodeTextError {
    BinaryContent,
    UnsupportedEncoding,
}

fn complete_utf8_prefix_len(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let mut lead = bytes.len() - 1;
    while lead > 0 && bytes[lead] & 0b1100_0000 == 0b1000_0000 {
        lead -= 1;
    }
    let expected = match bytes[lead] {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 1,
    };
    if bytes.len() - lead < expected {
        lead
    } else {
        bytes.len()
    }
}

pub(crate) fn decode_text(
    bytes: &[u8],
    chunked: bool,
) -> Result<(String, &'static str, bool, usize), DecodeTextError> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let payload = &bytes[3..];
        let consumed = if chunked {
            complete_utf8_prefix_len(payload)
        } else {
            payload.len()
        };
        let content = if chunked {
            String::from_utf8_lossy(&payload[..consumed]).into_owned()
        } else {
            String::from_utf8(payload.to_vec()).map_err(|_| DecodeTextError::UnsupportedEncoding)?
        };
        return Ok((content, "utf-8-bom", true, 3 + consumed));
    }

    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let little_endian = bytes.starts_with(&[0xFF, 0xFE]);
        let mut payload = &bytes[2..];
        if payload.len() % 2 != 0 {
            payload = &payload[..payload.len() - 1];
        }
        let units: Vec<u16> = payload
            .chunks_exact(2)
            .map(|pair| utf16_unit(pair, little_endian))
            .collect();
        let complete_units = if chunked && units.last().is_some_and(|unit| (0xD800..=0xDBFF).contains(unit)) {
            &units[..units.len() - 1]
        } else {
            &units
        };
        let content = if chunked {
            String::from_utf16_lossy(complete_units)
        } else {
            String::from_utf16(complete_units).map_err(|_| DecodeTextError::UnsupportedEncoding)?
        };
        return Ok((
            content,
            if little_endian { "utf-16-le" } else { "utf-16-be" },
            true,
            2 + complete_units.len() * 2,
        ));
    }

    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return Err(DecodeTextError::BinaryContent);
    }

    let consumed = if chunked {
        complete_utf8_prefix_len(bytes)
    } else {
        bytes.len()
    };
    let content = if chunked {
        String::from_utf8_lossy(&bytes[..consumed]).into_owned()
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeTextError::UnsupportedEncoding)?
    };
    Ok((content, "utf-8", false, consumed))
}

pub(crate) fn utf16_unit(pair: &[u8], little_endian: bool) -> u16 {
    if little_endian {
        u16::from_le_bytes([pair[0], pair[1]])
    } else {
        u16::from_be_bytes([pair[0], pair[1]])
    }
}

pub(crate) fn encode_text(content: &str, encoding: &str, bom: bool) -> Result<Vec<u8>, String> {
    match encoding {
        "utf-8" | "utf-8-bom" => {
            let mut bytes = Vec::with_capacity(content.len() + 3);
            if bom || encoding == "utf-8-bom" {
                bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            }
            bytes.extend_from_slice(content.as_bytes());
            Ok(bytes)
        }
        "utf-16-le" | "utf-16-be" => {
            let little_endian = encoding == "utf-16-le";
            let mut bytes = Vec::with_capacity(content.len() * 2 + 2);
            if bom {
                bytes.extend_from_slice(if little_endian { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
            }
            for unit in content.encode_utf16() {
                let encoded = if little_endian {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                };
                bytes.extend_from_slice(&encoded);
            }
            Ok(bytes)
        }
        _ => Err("不支持的文本编码".into()),
    }
}
