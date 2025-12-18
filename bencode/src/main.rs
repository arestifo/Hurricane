use std::collections::BTreeMap;

#[derive(PartialEq, Debug)]
enum DecodeError {
    DuplicateStartToken(usize),
    InvalidToken(usize, char),
    InvalidLength(usize),
    ByteStrEOF(usize),
    NoEndToken(usize),
    NoStartToken(usize),
    InvalidEndToken(usize),
    InvalidDict(usize),
    Empty(usize),
    LeadingZero(usize),
}

#[derive(PartialEq, Debug)]
enum BencodeValue {
    Int(i32),
    ByteStr(Vec<u8>),
    List(Vec<BencodeValue>),
    Dict(BTreeMap<Vec<u8>, BencodeValue>),
}

fn decode_int(enc_str: &[u8], start_pos: usize) -> Result<(i32, usize), DecodeError> {
    // All bencoded ints start have format `i<base_10_int>e`
    let mut pos: usize = start_pos;
    let mut started = false;
    let mut ended = false;
    let mut value: i32 = 0;
    let mut sign = 1;

    while pos < enc_str.len() {
        match enc_str[pos] {
            b'i' => {
                if started {
                    return Err(DecodeError::DuplicateStartToken(pos));
                }
                started = true;
                pos += 1;
            }
            b'0'..=b'9' => {
                if pos - start_pos > 2 && value == 0 {
                    return Err(DecodeError::LeadingZero(pos));
                }
                value = value * 10 + (enc_str[pos] - b'0') as i32;
                pos += 1;
            }
            b'-' => {
                // Can't have more than one sign (e.g. double negative)
                // Check if we've already "flipped" the sign
                if sign == -1 {
                    return Err(DecodeError::InvalidToken(pos, enc_str[pos] as char));
                }
                sign = -1;
                pos += 1;
            }
            b'e' => {
                if pos - start_pos <= 1 {
                    return Err(DecodeError::Empty(pos));
                }
                ended = true;
                pos += 1;
                break;
            }
            _ => return Err(DecodeError::InvalidToken(pos, enc_str[pos] as char)),
        }
    }

    if !ended {
        return Err(DecodeError::NoEndToken(pos));
    }

    Ok((value * sign, pos - start_pos))
}

fn decode_bytestr(enc_str: &[u8], start_pos: usize) -> Result<(Vec<u8>, usize), DecodeError> {
    // Step 1: parse the length of the byte string
    let mut pos: usize = start_pos;
    let mut str_sz: usize = 0;
    let mut valid_len = false;

    while pos < enc_str.len() {
        match enc_str[pos] {
            b'0'..=b'9' => {
                if enc_str[pos] == b'0' && str_sz == 0 && start_pos != pos {
                    return Err(DecodeError::LeadingZero(pos));
                }

                let digit = enc_str[pos] - b'0';
                str_sz = str_sz * 10 + digit as usize;
                pos += 1;
            }
            b':' => {
                valid_len = true;
                pos += 1;
                break;
            }
            _ => return Err(DecodeError::InvalidToken(pos, enc_str[pos] as char)),
        }
    }

    if !valid_len {
        return Err(DecodeError::InvalidLength(pos));
    }

    // Early return for zero-length string
    if str_sz == 0 {
        return Ok((vec![], pos - start_pos));
    }

    // Step 2: parse the byte string
    if pos + str_sz > enc_str.len() {
        return Err(DecodeError::ByteStrEOF(pos));
    }

    let ret = enc_str[pos..pos + str_sz].to_vec();
    pos += str_sz;

    Ok((ret, pos - start_pos))
}

enum ScopeType {
    Root,
    List,
    Dict,
}

struct Scope {
    stype: ScopeType,
    items: Vec<BencodeValue>,
}

fn decode(buf: &[u8]) -> Result<Vec<BencodeValue>, DecodeError> {
    let ret: Vec<BencodeValue> = Vec::new();
    let mut pos: usize = 0;

    // Maintain a stack for dealing with lists and dicts
    let mut stack: Vec<Scope> = Vec::new();
    stack.push(Scope {
        stype: ScopeType::Root,
        items: ret,
    });

    while pos < buf.len() {
        match buf[pos] {
            b'i' => {
                let (item, item_len) = decode_int(buf, pos)?;
                stack
                    .last_mut()
                    .unwrap()
                    .items
                    .push(BencodeValue::Int(item));
                pos += item_len;
            }
            b'0'..=b'9' => {
                let (item, item_len) = decode_bytestr(buf, pos)?;
                stack
                    .last_mut()
                    .unwrap()
                    .items
                    .push(BencodeValue::ByteStr(item));
                pos += item_len;
            }
            b'l' => {
                // Start a "new scope"
                stack.push(Scope {
                    stype: ScopeType::List,
                    items: vec![],
                });
                pos += 1;
            }
            b'd' => {
                stack.push(Scope {
                    stype: ScopeType::Dict,
                    items: vec![],
                });
                pos += 1;
            }
            b'e' => {
                // We'll only see 'e' when we're in a scope (parsing a list or dict) because the
                // 'e' that occurs in int parsing is consumed by the int decoding function
                // So if we're not in a scope, it's an error. Otherwise we simply exit the scope
                // TODO: what does this mean for dicts
                match stack.pop().unwrap() {
                    Scope {
                        stype: ScopeType::List,
                        items,
                    } => {
                        stack
                            .last_mut()
                            .unwrap()
                            .items
                            .push(BencodeValue::List(items));
                    }
                    Scope {
                        stype: ScopeType::Dict,
                        items,
                    } => {
                        if items.len() % 2 != 0 {
                            // TODO: change this to MissingKey and MissingValue errors
                            return Err(DecodeError::InvalidDict(pos));
                        }

                        // Iterate over pairs and create a BTreeMap from them
                        let mut dict_item: BTreeMap<Vec<u8>, BencodeValue> = BTreeMap::new();
                        let mut iter = items.into_iter();
                        while let (Some(key_item), Some(val_item)) = (iter.next(), iter.next()) {
                            if let BencodeValue::ByteStr(key) = key_item {
                                dict_item.insert(key, val_item);
                            }
                        }

                        // TODO: check for lexicographic order after map is created
                        stack
                            .last_mut()
                            .unwrap()
                            .items
                            .push(BencodeValue::Dict(dict_item))
                    }
                    Scope {
                        stype: ScopeType::Root,
                        items: _,
                    } => return Err(DecodeError::InvalidEndToken(pos)),
                }
                pos += 1;
            }
            _ => return Err(DecodeError::InvalidToken(pos, buf[pos] as char)),
        }
    }

    // If there's still unclosed scopes, we're missing an end token somewhere
    // We want to end parsing with just the root scope
    if stack.len() > 1 {
        return Err(DecodeError::NoEndToken(pos));
    }

    Ok(stack.pop().unwrap().items)
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_real_torrent_file() {
        use std::fs;

        let torrent_bytes = fs::read("tests/fixtures/sample.torrent").unwrap();

        let result = decode(&torrent_bytes).expect("Failed to decode torrent file");

        assert_eq!(result, vec![])
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_dict_ok() {
        let str = "d3:heyi69ee";
        let ret = decode(str.as_bytes()).unwrap();

        assert_eq!(
            ret,
            vec![BencodeValue::Dict(BTreeMap::from([(
                b"hey".to_vec(),
                BencodeValue::Int(69)
            ),]))]
        )
    }

    #[test]
    fn test_dict_nested() {
        let str = "d3:heyd3:food3:bari420eeee";
        let ret = decode(str.as_bytes()).unwrap();

        assert_eq!(
            ret,
            vec![BencodeValue::Dict(BTreeMap::from([(
                b"hey".to_vec(),
                BencodeValue::Dict(BTreeMap::from([(
                    b"foo".to_vec(),
                    BencodeValue::Dict(BTreeMap::from(
                        [(b"bar".to_vec(), BencodeValue::Int(420)),]
                    ))
                ),]))
            ),]))]
        )
    }

    #[test]
    fn test_complex_nested() {
        // The big kahuna!
        // Represents:
        // {
        //   "user": [
        //     {"name": "John", "age": 30, "scores": [100, [95, 88]]}
        //   ],
        //   "meta": {"admin": "", "1": "y", "active": "1"}
        // }
        let str = "d4:userld4:name4:John3:agei30e6:scoresli100eli95ei88eeeee4:metad5:admin0:1:11:y6:active1:1ee";
        let ret = decode(str.as_bytes()).unwrap();

        assert_eq!(
            ret,
            vec![BencodeValue::Dict(BTreeMap::from([
                (
                    b"user".to_vec(),
                    BencodeValue::List(vec![BencodeValue::Dict(BTreeMap::from([
                        (b"name".to_vec(), BencodeValue::ByteStr(b"John".to_vec())),
                        (b"age".to_vec(), BencodeValue::Int(30)),
                        (
                            b"scores".to_vec(),
                            BencodeValue::List(vec![
                                BencodeValue::Int(100),
                                BencodeValue::List(vec![
                                    BencodeValue::Int(95),
                                    BencodeValue::Int(88),
                                ]),
                            ])
                        ),
                    ])),])
                ),
                (
                    b"meta".to_vec(),
                    BencodeValue::Dict(BTreeMap::from([
                        (b"admin".to_vec(), BencodeValue::ByteStr(b"".to_vec())),
                        (b"1".to_vec(), BencodeValue::ByteStr(b"y".to_vec())),
                        (b"active".to_vec(), BencodeValue::ByteStr(b"1".to_vec())),
                    ]))
                ),
            ]))]
        );
    }

    #[test]
    fn test_list_ok() {
        let str = "li420ei69ee";
        let ret = decode(str.as_bytes()).unwrap();

        assert_eq!(
            ret,
            vec![BencodeValue::List(vec![
                BencodeValue::Int(420),
                BencodeValue::Int(69),
            ])]
        )
    }
    #[test]
    fn test_list_empty() {
        let str = "le";
        let ret = decode(str.as_bytes()).unwrap();

        assert_eq!(ret, vec![BencodeValue::List(vec![]),])
    }

    #[test]
    fn test_list_nested() {
        let str = "llli420eeee";
        let ret = decode(str.as_bytes()).unwrap();

        assert_eq!(
            ret,
            vec![BencodeValue::List(vec![BencodeValue::List(vec![
                BencodeValue::List(vec![BencodeValue::Int(420),])
            ])])]
        )
    }

    #[test]
    fn test_list_complex() {
        let str = "lli420el3:heyeel5:Helloee";
        let ret = decode(str.as_bytes()).unwrap();

        assert_eq!(
            ret,
            vec![BencodeValue::List(vec![
                BencodeValue::List(vec![
                    BencodeValue::Int(420),
                    BencodeValue::List(vec![BencodeValue::ByteStr("hey".as_bytes().to_vec())])
                ]),
                BencodeValue::List(vec![BencodeValue::ByteStr("Hello".as_bytes().to_vec())])
            ])]
        )
    }

    #[test]
    fn test_int_ok() {
        let str = "i1234567890e";
        let (item, pos) = decode_int(&str.as_bytes(), 0).unwrap();

        assert_eq!(pos, 12);
        assert_eq!(item, 1234567890);
    }

    #[test]
    fn test_int_neg() {
        let str = "i-125e";
        let (item, pos) = decode_int(&str.as_bytes(), 0).unwrap();

        assert_eq!(pos, 6);
        assert_eq!(item, -125);
    }

    #[test]
    fn test_int_double_neg() {
        let str = "i--69e";
        let result = decode_int(&str.as_bytes(), 0);

        assert_eq!(result.err(), Some(DecodeError::InvalidToken(2, '-')))
    }

    #[test]
    fn test_int_empty() {
        let str = "ie";
        let result = decode_int(&str.as_bytes(), 0);

        assert_eq!(result.err(), Some(DecodeError::Empty(1)));
    }

    #[test]
    fn test_int_invalid() {
        let str = "iBe";
        let result = decode_int(&str.as_bytes(), 0);

        assert_eq!(result.err(), Some(DecodeError::InvalidToken(1, 'B')));
    }

    #[test]
    fn test_int_noend() {
        let str = "i420";
        let result = decode_int(&str.as_bytes(), 0);

        assert_eq!(result.err(), Some(DecodeError::NoEndToken(4)));
    }

    #[test]
    fn test_int_duplicate_start() {
        let str = "ii420";
        let result = decode_int(&str.as_bytes(), 0);

        assert_eq!(result.err(), Some(DecodeError::DuplicateStartToken(1)));
    }

    #[test]
    fn test_bstr_ok() {
        let str = "3:hey";
        let (item, pos) = decode_bytestr(str.as_bytes(), 0).unwrap();

        assert_eq!(pos, 5);
        assert_eq!(item, b"hey");
    }

    #[test]
    fn test_bstr_long() {
        let str = "44:The quick brown fox jumped over the lazy dog";
        let (item, pos) = decode_bytestr(str.as_bytes(), 0).unwrap();

        assert_eq!(pos, 47);
        assert_eq!(item, b"The quick brown fox jumped over the lazy dog");
    }

    #[test]
    fn test_bstr_bytes() {
        let str = &[b'4', b':', 0xAA, 0xBB, 0xCC, 0xDD];
        let (item, pos) = decode_bytestr(str, 0).unwrap();

        assert_eq!(pos, 6);
        assert_eq!(item, &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn test_bstr_empty() {
        let str = "0:";
        let (item, pos) = decode_bytestr(str.as_bytes(), 0).unwrap();

        assert_eq!(pos, 2);
        assert_eq!(item, b"");
    }

    #[test]
    fn test_bstr_backtoback() {
        let str = "5:admin0:3:hey10:abcdefghij";
        let result = decode(str.as_bytes()).unwrap();

        assert_eq!(
            result,
            vec![
                BencodeValue::ByteStr("admin".as_bytes().to_vec()),
                BencodeValue::ByteStr("".as_bytes().to_vec()),
                BencodeValue::ByteStr("hey".as_bytes().to_vec()),
                BencodeValue::ByteStr("abcdefghij".as_bytes().to_vec()),
            ]
        );
    }
}

fn main() {}
