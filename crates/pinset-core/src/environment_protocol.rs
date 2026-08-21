use std::collections::BTreeMap;

const MAGIC: &[u8] = b"PINSET_ENV_V1\0";
const MAX_ITEMS: usize = 4096;
const MAX_FIELD: usize = 1024 * 1024;

pub fn encode_environment(values: &BTreeMap<String, String>) -> Result<Vec<u8>, String> {
    if values.len() > MAX_ITEMS {
        return Err("environment protocol contains too many variables".to_owned());
    }
    let mut output = Vec::with_capacity(MAGIC.len() + values.len() * 32);
    output.extend_from_slice(MAGIC);
    write_u32(&mut output, values.len())?;
    for (name, value) in values {
        write_field(&mut output, name.as_bytes())?;
        write_field(&mut output, value.as_bytes())?;
    }
    Ok(output)
}

pub fn decode_environment(input: &[u8]) -> Result<BTreeMap<String, String>, String> {
    if !input.starts_with(MAGIC) {
        return Err("environment protocol header is invalid".to_owned());
    }
    let mut cursor = MAGIC.len();
    let count = read_u32(input, &mut cursor)?;
    if count > MAX_ITEMS {
        return Err("environment protocol contains too many variables".to_owned());
    }
    let mut values = BTreeMap::new();
    for _ in 0..count {
        let name = String::from_utf8(read_field(input, &mut cursor)?.to_vec())
            .map_err(|_| "environment protocol name is not UTF-8".to_owned())?;
        let value = String::from_utf8(read_field(input, &mut cursor)?.to_vec())
            .map_err(|_| "environment protocol value is not UTF-8".to_owned())?;
        if values.insert(name, value).is_some() {
            return Err("environment protocol contains a duplicate variable".to_owned());
        }
    }
    if cursor != input.len() {
        return Err("environment protocol has trailing data".to_owned());
    }
    Ok(values)
}

fn write_field(output: &mut Vec<u8>, field: &[u8]) -> Result<(), String> {
    if field.len() > MAX_FIELD {
        return Err("environment protocol field is too large".to_owned());
    }
    write_u32(output, field.len())?;
    output.extend_from_slice(field);
    Ok(())
}

fn write_u32(output: &mut Vec<u8>, value: usize) -> Result<(), String> {
    let value =
        u32::try_from(value).map_err(|_| "environment protocol length is too large".to_owned())?;
    output.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn read_u32(input: &[u8], cursor: &mut usize) -> Result<usize, String> {
    let end = cursor
        .checked_add(4)
        .ok_or_else(|| "environment protocol length overflow".to_owned())?;
    let bytes: [u8; 4] = input
        .get(*cursor..end)
        .ok_or_else(|| "environment protocol is truncated".to_owned())?
        .try_into()
        .map_err(|_| "environment protocol length is invalid".to_owned())?;
    *cursor = end;
    Ok(u32::from_le_bytes(bytes) as usize)
}

fn read_field<'a>(input: &'a [u8], cursor: &mut usize) -> Result<&'a [u8], String> {
    let length = read_u32(input, cursor)?;
    if length > MAX_FIELD {
        return Err("environment protocol field is too large".to_owned());
    }
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| "environment protocol length overflow".to_owned())?;
    let field = input
        .get(*cursor..end)
        .ok_or_else(|| "environment protocol is truncated".to_owned())?;
    *cursor = end;
    Ok(field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_without_delimiter_ambiguity() {
        let values = BTreeMap::from([
            ("EMPTY".to_owned(), String::new()),
            ("MULTILINE".to_owned(), "one\ntwo=three".to_owned()),
        ]);
        assert_eq!(
            decode_environment(&encode_environment(&values).unwrap()).unwrap(),
            values
        );
    }

    #[test]
    fn rejects_truncation_and_trailing_bytes() {
        let values = BTreeMap::from([("A".to_owned(), "B".to_owned())]);
        let mut encoded = encode_environment(&values).unwrap();
        assert!(decode_environment(&encoded[..encoded.len() - 1]).is_err());
        encoded.push(0);
        assert!(decode_environment(&encoded).is_err());
    }
}
