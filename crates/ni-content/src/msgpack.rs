/// A MessagePack value, as far as `PLID` needs one.
pub enum Mp {
    Float(f64),
    Int(i64),
    Str(String),
    List(Vec<Mp>),
    Map(Vec<(String, Mp)>),
    Other,
}

pub struct Reader<'a> {
    pub b: &'a [u8],
    pub at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.at.checked_add(n).filter(|end| *end <= self.b.len()).ok_or("the preset's plug-in id is truncated")?;
        let out = &self.b[self.at..end];
        self.at = end;
        Ok(out)
    }

    /// An unsigned big-endian number of `n` bytes.
    fn number(&mut self, n: usize) -> Result<u64, String> {
        Ok(self.take(n)?.iter().fold(0u64, |v, b| (v << 8) | *b as u64))
    }

    fn text(&mut self, n: usize) -> Result<Mp, String> {
        Ok(Mp::Str(String::from_utf8_lossy(self.take(n)?).into_owned()))
    }

    fn skip(&mut self, n: usize) -> Result<Mp, String> {
        self.take(n)?;
        Ok(Mp::Other)
    }

    fn list(&mut self, n: usize, depth: u8) -> Result<Mp, String> {
        // (Not sized ahead: a truncated file can claim any length.)
        let mut out = Vec::new();
        for _ in 0..n {
            out.push(self.value(depth + 1)?);
        }
        Ok(Mp::List(out))
    }

    fn map(&mut self, n: usize, depth: u8) -> Result<Mp, String> {
        let mut out = Vec::new();
        for _ in 0..n {
            let key = match self.value(depth + 1)? {
                Mp::Str(key) => key,
                _ => String::new(),
            };
            out.push((key, self.value(depth + 1)?));
        }
        Ok(Mp::Map(out))
    }

    pub fn value(&mut self, depth: u8) -> Result<Mp, String> {
        if depth > 8 {
            return Err("the preset's plug-in id is nested too deeply".into());
        }
        let t = self.take(1)?[0];
        match t {
            0x00..=0x7f => Ok(Mp::Int(t as i64)),
            0xe0..=0xff => Ok(Mp::Int(t as i8 as i64)),
            0x80..=0x8f => self.map((t & 0x0f) as usize, depth),
            0x90..=0x9f => self.list((t & 0x0f) as usize, depth),
            0xa0..=0xbf => self.text((t & 0x1f) as usize),
            0xc0 | 0xc2 | 0xc3 => Ok(Mp::Other),
            0xc4..=0xc6 => {
                let n = self.number(1 << (t - 0xc4))? as usize;
                self.skip(n)
            }
            0xc7..=0xc9 => {
                let n = self.number(1 << (t - 0xc7))? as usize;
                self.skip(n.saturating_add(1))
            }
            0xca => Ok(Mp::Float(f32::from_bits(self.number(4)? as u32) as f64)),
            0xcb => Ok(Mp::Float(f64::from_bits(self.number(8)?))),
            0xcc..=0xcf => Ok(Mp::Int(self.number(1 << (t - 0xcc))? as i64)),
            0xd0 => Ok(Mp::Int(self.number(1)? as u8 as i8 as i64)),
            0xd1 => Ok(Mp::Int(self.number(2)? as u16 as i16 as i64)),
            0xd2 => Ok(Mp::Int(self.number(4)? as u32 as i32 as i64)),
            0xd3 => Ok(Mp::Int(self.number(8)? as i64)),
            0xd4..=0xd8 => self.skip((1usize << (t - 0xd4)) + 1),
            0xd9..=0xdb => {
                let n = self.number(1 << (t - 0xd9))? as usize;
                self.text(n)
            }
            0xdc | 0xdd => {
                let n = self.number(2 << (t - 0xdc))? as usize;
                self.list(n, depth)
            }
            0xde | 0xdf => {
                let n = self.number(2 << (t - 0xde))? as usize;
                self.map(n, depth)
            }
            0xc1 => Err("the preset's plug-in id isn't valid".into()),
        }
    }
}

