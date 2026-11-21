//! 외부 크레이트 없는 최소 JSON 읽기(설정 파일·`klib scan` JSON 줄용). 쓰기는 하지 않는다.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s) } else { None }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let Value::Num(n) = self { Some(*n) } else { None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(b) = self { Some(*b) } else { None }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        if let Value::Arr(a) = self { Some(a) } else { None }
    }
}

pub fn parse(text: &str) -> Result<Value, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut p = Parser { s: text.as_bytes(), i: 0, depth: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        return Err(format!("JSON 끝에 남는 글자(위치 {})", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    depth: u32,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.peek() == Some(b) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("'{}' 이(가) 필요합니다(위치 {})", b as char, self.i))
        }
    }
    fn lit(&mut self, word: &str, v: Value) -> Result<Value, String> {
        if self.s[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(format!("알 수 없는 값(위치 {})", self.i))
        }
    }
    fn value(&mut self) -> Result<Value, String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err("JSON 중첩이 너무 깊습니다".into());
        }
        let v = match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Value::Str),
            Some(b't') => self.lit("true", Value::Bool(true)),
            Some(b'f') => self.lit("false", Value::Bool(false)),
            Some(b'n') => self.lit("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(format!("값이 필요합니다(위치 {})", self.i)),
        };
        self.depth -= 1;
        v
    }
    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        while self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()
            .and_then(|t| t.parse::<f64>().ok())
            .filter(|n| n.is_finite())
            .map(Value::Num)
            .ok_or_else(|| format!("숫자가 아닙니다(위치 {start})"))
    }
    fn hex4(&mut self) -> Result<u32, String> {
        let end = self.i + 4;
        let t = self.s.get(self.i..end).and_then(|b| std::str::from_utf8(b).ok()).ok_or("\\u 뒤 16진수 부족")?;
        let n = u32::from_str_radix(t, 16).map_err(|_| "\\u 뒤 16진수 오류".to_string())?;
        self.i = end;
        Ok(n)
    }
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(b) = self.peek() else { return Err("문자열이 닫히지 않았습니다".into()) };
            self.i += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let Some(e) = self.peek() else { return Err("문자열이 닫히지 않았습니다".into()) };
                    self.i += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) {
                                if self.s[self.i..].starts_with(b"\\u") {
                                    self.i += 2;
                                    let lo = self.hex4()?;
                                    if !(0xDC00..0xE000).contains(&lo) {
                                        return Err("서로게이트 쌍 오류".into());
                                    }
                                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                } else {
                                    return Err("서로게이트 쌍 오류".into());
                                }
                            } else {
                                hi
                            };
                            char::from_u32(code).ok_or("잘못된 유니코드 값")?
                        }
                        _ => return Err(format!("알 수 없는 이스케이프(위치 {})", self.i)),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                b if b < 0x20 => return Err("문자열 안의 제어 문자".into()),
                b => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| "UTF-8 이 아닌 문자열".to_string())
    }
    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(format!("배열이 닫히지 않았습니다(위치 {})", self.i)),
            }
        }
    }
    fn object(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Obj(items));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            self.expect(b':')?;
            self.ws();
            let v = self.value()?;
            items.push((k, v));
            self.ws();
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Obj(items));
                }
                _ => return Err(format!("객체가 닫히지 않았습니다(위치 {})", self.i)),
            }
        }
    }
}
