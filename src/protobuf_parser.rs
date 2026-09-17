/// High-performance Protobuf Varint and Field Parser for Antigravity gen_metadata

#[derive(Debug, Default, Clone, Copy)]
pub struct TokenCounts {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[inline]
fn decode_varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut val: u64 = 0;
    let mut shift: u32 = 0;
    while *pos < data.len() {
        let b = data[*pos];
        *pos += 1;
        val |= ((b & 0x7f) as u64) << shift;
        shift += 7;
        if (b & 0x80) == 0 {
            return Some(val);
        }
        if shift > 64 {
            return None;
        }
    }
    None
}

/// Parses input and output tokens from a gen_metadata BLOB
pub fn parse_tokens_from_gen_metadata(data: &[u8]) -> TokenCounts {
    let mut tokens = TokenCounts::default();
    let mut pos = 0;

    while pos < data.len() {
        let key = match decode_varint(data, &mut pos) {
            Some(k) => k,
            None => break,
        };
        let field_num = key >> 3;
        let wire_type = key & 7;

        match wire_type {
            0 => {
                // Varint
                if decode_varint(data, &mut pos).is_none() {
                    break;
                }
            }
            2 => {
                // Length-delimited
                let len = match decode_varint(data, &mut pos) {
                    Some(l) => l as usize,
                    None => break,
                };
                if pos + len > data.len() {
                    break;
                }
                let sub = &data[pos..pos + len];
                pos += len;

                // Field 1 is the main generation container
                if field_num == 1 {
                    parse_sub_container(sub, &mut tokens);
                }
            }
            1 => {
                // 64-bit
                pos += 8;
            }
            5 => {
                // 32-bit
                pos += 4;
            }
            _ => break,
        }
    }

    tokens
}

fn parse_sub_container(data: &[u8], tokens: &mut TokenCounts) {
    let mut spos = 0;
    while spos < data.len() {
        let skey = match decode_varint(data, &mut spos) {
            Some(k) => k,
            None => break,
        };
        let sfn = skey >> 3;
        let sw = skey & 7;

        match sw {
            0 => {
                if decode_varint(data, &mut spos).is_none() {
                    break;
                }
            }
            2 => {
                let slen = match decode_varint(data, &mut spos) {
                    Some(l) => l as usize,
                    None => break,
                };
                if spos + slen > data.len() {
                    break;
                }
                let ssub = &data[spos..spos + slen];
                spos += slen;

                // Field 4 contains the token counts (Tag 4 in container)
                if sfn == 4 {
                    parse_token_message(ssub, tokens);
                }
            }
            1 => spos += 8,
            5 => spos += 4,
            _ => break,
        }
    }
}

fn parse_token_message(data: &[u8], tokens: &mut TokenCounts) {
    let mut mpos = 0;
    while mpos < data.len() {
        let mkey = match decode_varint(data, &mut mpos) {
            Some(k) => k,
            None => break,
        };
        let mfn = mkey >> 3;
        let mw = mkey & 7;

        match mw {
            0 => {
                let mval = match decode_varint(data, &mut mpos) {
                    Some(v) => v,
                    None => break,
                };
                if mfn == 2 {
                    tokens.input_tokens = mval;
                } else if mfn == 3 {
                    tokens.output_tokens = mval;
                }
            }
            2 => {
                let mlen = match decode_varint(data, &mut mpos) {
                    Some(l) => l as usize,
                    None => break,
                };
                mpos += mlen;
            }
            1 => mpos += 8,
            5 => mpos += 4,
            _ => break,
        }
    }
}
