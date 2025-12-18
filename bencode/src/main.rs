use std::collections::BTreeMap;

#[derive(Debug)]
#[derive(PartialEq)]
enum DecodeError {
    DuplicateStartToken(usize),
    InvalidToken(usize, char),
    InvalidLength(usize),
    ByteStrEOF(usize),
    NoEndToken(usize),
    InvalidEndToken(usize),
    Empty(usize),
    LeadingZero(usize),
}
trait BencodeDecode {
    fn decode(&mut self, enc_str: &[u8], pos: usize) -> Result<usize, DecodeError>;
}

struct Int {
    value: i32
}
impl BencodeDecode for Int {
    fn decode(&mut self, enc_str: &[u8], pos: usize) -> Result<usize, DecodeError> {
        // All bencoded ints start have format `i<base_10_int>e`
        let mut pos: usize = pos;
        let mut started = false;
        let mut ended = false;
        let mut int_chars: Vec<u8> = Vec::new();
        let mut sign = 1;

        while pos < enc_str.len() {
            match enc_str[pos] {
                b'i' => {
                    if started {
                        return Err(DecodeError::DuplicateStartToken(pos))
                    }
                    started = true;
                    pos += 1;
                },
                b'0'..=b'9' => {
                    if int_chars.last() == Some(&b'0') {
                        return Err(DecodeError::LeadingZero(pos))
                    }
                    int_chars.push(enc_str[pos]);
                    pos += 1;
                }
                b'-' => {
                    sign *= -1;
                    pos += 1;
                }
                b'e' => {
                    if int_chars.len() == 0 {
                        return Err(DecodeError::Empty(pos))
                    }
                    ended = true;
                    pos += 1;
                    break;
                },
                _ => return Err(DecodeError::InvalidToken(pos, enc_str[pos] as char))
            }
        }

        if !ended {
            return Err(DecodeError::NoEndToken(pos))
        }

        self.value = int_chars
            .iter()
            .map(|x| x - b'0')
            .fold(0, |acc, x| acc * 10 + x as i32);
        self.value *= sign;

        Ok(pos)
    }
}

struct ByteStr {
    value: Vec<u8>
}
impl BencodeDecode for ByteStr {
    fn decode(&mut self, enc_str: &[u8], pos: usize) -> Result<usize, DecodeError> {
        // Step 1: parse the length of the byte string
        let start_pos = pos;
        let mut pos: usize = 0;
        let mut str_sz: usize = 0;
        let mut valid_len = false;

        while pos < enc_str.len() {
            match enc_str[pos] {
                b'0'..=b'9' => {
                    if enc_str[pos] == b'0' && start_pos != pos {
                        return Err(DecodeError::LeadingZero(pos))
                    }

                    let digit = enc_str[pos] - b'0';
                    str_sz = str_sz * 10 + digit as usize;
                    pos += 1;
                },
                b':' => {
                    valid_len = true;
                    pos += 1;
                    break;
                },
                _ => return Err(DecodeError::InvalidToken(pos, enc_str[pos] as char))
            }
        }

        if !valid_len {
            return Err(DecodeError::InvalidLength(pos))
        }

        // Early return for zero-length string
        if str_sz == 0 {
            self.value = vec![];
            return Ok(pos);
        }

        // Step 2: parse the byte string
        if pos + str_sz > enc_str.len() {
            return Err(DecodeError::ByteStrEOF(pos))
        }

        self.value = enc_str[pos..pos + str_sz].to_vec();
        pos += str_sz;

        Ok(pos)
    }
}

struct List {
    value: Vec<Box<dyn BencodeDecode>>
}
impl BencodeDecode for List {
    fn decode(&mut self, enc_str: &[u8], pos: usize) -> Result<usize, DecodeError> {
        // Gross
        Ok(pos)
    }
}

struct Dict {
    value: BTreeMap<String, Box<dyn BencodeDecode>>,
}
enum ScopeType {
    Root,
    List,
    Dict,
}

struct Scope {
    stype: ScopeType,
    items: Vec<Box<dyn BencodeDecode>>,
}

fn decode(enc_str: &str) -> Result<Vec<Box<dyn BencodeDecode>>, DecodeError> {
    let str_bytes = enc_str.as_bytes();
    let ret: Vec<Box<dyn BencodeDecode>> = Vec::new();
    let mut pos: usize = 0;

    // Maintain a stack for dealing with lists and dicts
    let mut stack: Vec<Scope> = Vec::new();
    stack.push(Scope { stype: ScopeType::Root, items: ret });

    while pos < str_bytes.len() {
        match str_bytes[pos] {
            b'i' => {
                // Gross, we're basically decoding by using a side effect.
                // Restructure this to avoid
                let mut ele = Int{ value: 0 };
                pos += ele.decode(&str_bytes, pos)?;
                stack.last_mut().unwrap().items.push(Box::new(ele));
            },
            b'0'..=b'9' => {
                let mut ele = ByteStr{ value: vec![] };
                pos += ele.decode(&str_bytes, pos)?;
                stack.last_mut().unwrap().items.push(Box::new(ele));
            }
            b'l' => {
                // Start a "new scope"
                stack.push(Scope { stype: ScopeType::List, items: vec![] });
                pos += 1
            }
            b'd' => {
                todo!()
            }
            b'e' => {
                // We'll only see 'e' when we're in a scope (parsing a list or dict) because the
                // 'e' that occurs in int parsing is consumed by the int decoding function
                // So if we're not in a scope, it's an error. Otherwise we simply exit the scope
                // TODO: what does this mean for dicts
                match stack.pop().unwrap() {
                    Scope { stype: ScopeType::List, items } => {
                        let ele = List{ value: items };
                        stack.last_mut().unwrap().items.push(Box::new(ele));
                    },
                    Scope { stype: ScopeType::Dict, items } => {
                        todo!()
                    }
                    Scope { stype: ScopeType::Root, items } => {
                        return Err(DecodeError::InvalidEndToken(pos))
                    }
                }

            }
            _ => return Err(DecodeError::InvalidToken(pos, str_bytes[pos] as char))
        }
    }

    // If there's still unclosed scopes, we're missing an end token somewhere
    // We want to end parsing with just the root scope
    if stack.len() > 1 {
        return Err(DecodeError::NoEndToken(pos))
    }

    Ok(stack.pop().unwrap().items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_list_ok() {
        let str = "li420ei69ee";
        let ret = decode(str).unwrap();
    }

    #[test]
    fn test_int_ok() {
        let str = "i1234567890e".as_bytes();

        let mut ele = Int{ value: 0 };
        let pos = ele.decode(&str, 0).unwrap();

        assert_eq!(pos, 12);
        assert_eq!(ele.value, 1234567890);
    }

    #[test]
    fn test_int_neg() {
        let str = "i-125e".as_bytes();

        let mut ele = Int{ value: 0 };
        let pos = ele.decode(&str, 0).unwrap();

        assert_eq!(pos, 6);
        assert_eq!(ele.value, -125);
    }

    #[test]
    fn test_int_double_neg() {
        let str = "i--69e".as_bytes();

        let mut ele = Int{ value: 0 };
        let pos = ele.decode(&str, 0).unwrap();

        assert_eq!(pos, 6);
        assert_eq!(ele.value, 69);
    }

    #[test]
    fn test_int_empty() {
        let str = "ie".as_bytes();
        let mut ele = Int{ value: 0 };

        let pos = ele.decode(&str, 0);

        assert_eq!(pos.err(), Some(DecodeError::Empty(1)));
    }

    #[test]
    fn test_int_invalid() {
        let str = "iBe".as_bytes();

        let mut ele = Int{ value: 0 };
        let pos = ele.decode(&str, 0);

        assert_eq!(pos.err(), Some(DecodeError::InvalidToken(1, 'B')));
    }

    #[test]
    fn test_int_noend() {
        let str = "i420".as_bytes();

        let mut ele = Int{ value: 0 };
        let pos = ele.decode(&str, 0);

        assert_eq!(pos.err(), Some(DecodeError::NoEndToken(4)));
    }

    #[test]
    fn test_int_duplicate_start() {
        let str = "ii420".as_bytes();

        let mut ele = Int{ value: 0 };
        let pos = ele.decode(&str, 0);

        assert_eq!(pos.err(), Some(DecodeError::DuplicateStartToken(1)));
    }

    #[test]
    fn test_bstr_ok() {
        let str = "3:hey".as_bytes();

        let mut ele = ByteStr{ value: vec![] };
        let pos = ele.decode(str, 0).unwrap();

        assert_eq!(pos, 5);
        assert_eq!(ele.value, b"hey".to_vec());
    }

    #[test]
    fn test_bstr_long() {
        let str = "44:The quick brown fox jumped over the lazy dog".as_bytes();

        let mut ele = ByteStr{ value: vec![] };
        let pos = ele.decode(str, 0).unwrap();

        assert_eq!(pos, 47);
        assert_eq!(ele.value, b"The quick brown fox jumped over the lazy dog".to_vec());
    }

    #[test]
    fn test_bstr_bytes() {
        let str = &[b'4', b':', 0xAA, 0xBB, 0xCC, 0xDD];

        let mut ele = ByteStr{ value: vec![] };
        let pos = ele.decode(str, 0).unwrap();

        assert_eq!(pos, 6);
        assert_eq!(ele.value, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn test_bstr_empty() {
        let str = "0:".as_bytes();

        let mut ele = ByteStr{ value: vec![] };
        let pos = ele.decode(str, 0).unwrap();

        assert_eq!(pos, 2);
        assert_eq!(ele.value, vec![]);
    }
}

fn main() {}