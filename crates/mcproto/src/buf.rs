//! Reading and writing protocol primitives.

use crate::error::{Error, Result};

/// Longest string the 1.8 protocol allows (in UTF-16 units; we count chars).
pub const MAX_STRING: usize = 32767;

/// Reads protocol primitives from the front of a byte slice, advancing it.
pub trait Read<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]>;
    fn rest(&mut self) -> &'a [u8];

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn i8(&mut self) -> Result<i8> {
        Ok(self.u8()? as i8)
    }
    fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_be_bytes(
            self.take(8)?.try_into().expect("8 bytes"),
        ))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.i32()? as u32))
    }
    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.i64()? as u64))
    }
    fn uuid(&mut self) -> Result<u128> {
        Ok(u128::from_be_bytes(
            self.take(16)?.try_into().expect("16 bytes"),
        ))
    }
    fn varint(&mut self) -> Result<i32> {
        let mut value: u32 = 0;
        for i in 0..5 {
            let byte = self.u8()?;
            value |= u32::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 == 0 {
                return Ok(value as i32);
            }
        }
        Err(Error::VarIntTooLong)
    }
    /// A non-negative VarInt used as a length or count.
    fn length(&mut self) -> Result<usize> {
        let n = self.varint()?;
        usize::try_from(n).map_err(|_| Error::Malformed(format!("negative length {n}")))
    }
    fn string(&mut self) -> Result<String> {
        self.string_max(MAX_STRING)
    }
    fn string_max(&mut self, max: usize) -> Result<String> {
        let len = self.length()?;
        if len > max * 4 {
            return Err(Error::StringTooLong { len, max });
        }
        let s = std::str::from_utf8(self.take(len)?).map_err(|_| Error::InvalidString)?;
        Ok(s.to_string())
    }
    fn byte_array(&mut self) -> Result<Vec<u8>> {
        let len = self.length()?;
        Ok(self.take(len)?.to_vec())
    }
}

impl<'a> Read<'a> for &'a [u8] {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.len() < n {
            return Err(Error::Truncated);
        }
        let (head, tail) = self.split_at(n);
        *self = tail;
        Ok(head)
    }

    fn rest(&mut self) -> &'a [u8] {
        std::mem::take(self)
    }
}

/// Writes protocol primitives to the end of a byte vector.
pub trait Write {
    fn put(&mut self, bytes: &[u8]);

    fn u8(&mut self, v: u8) {
        self.put(&[v]);
    }
    fn i8(&mut self, v: i8) {
        self.put(&[v as u8]);
    }
    fn bool(&mut self, v: bool) {
        self.put(&[u8::from(v)]);
    }
    fn u16(&mut self, v: u16) {
        self.put(&v.to_be_bytes());
    }
    fn i16(&mut self, v: i16) {
        self.put(&v.to_be_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.put(&v.to_be_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.put(&v.to_be_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.put(&v.to_bits().to_be_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.put(&v.to_bits().to_be_bytes());
    }
    fn uuid(&mut self, v: u128) {
        self.put(&v.to_be_bytes());
    }
    fn varint(&mut self, v: i32) {
        let mut v = v as u32;
        loop {
            if v & !0x7f == 0 {
                self.put(&[v as u8]);
                return;
            }
            self.put(&[(v & 0x7f) as u8 | 0x80]);
            v >>= 7;
        }
    }
    fn length(&mut self, n: usize) {
        self.varint(n as i32);
    }
    fn string(&mut self, s: &str) {
        self.length(s.len());
        self.put(s.as_bytes());
    }
    fn byte_array(&mut self, bytes: &[u8]) {
        self.length(bytes.len());
        self.put(bytes);
    }
}

impl Write for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

pub fn varint_len(v: i32) -> usize {
    let v = v as u32;
    match v {
        0..=0x7f => 1,
        0x80..=0x3fff => 2,
        0x4000..=0x1f_ffff => 3,
        0x20_0000..=0x0fff_ffff => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_round_trip_and_known_encodings() {
        let cases: &[(i32, &[u8])] = &[
            (0, &[0x00]),
            (1, &[0x01]),
            (127, &[0x7f]),
            (128, &[0x80, 0x01]),
            (255, &[0xff, 0x01]),
            (25565, &[0xdd, 0xc7, 0x01]),
            (2097151, &[0xff, 0xff, 0x7f]),
            (i32::MAX, &[0xff, 0xff, 0xff, 0xff, 0x07]),
            (-1, &[0xff, 0xff, 0xff, 0xff, 0x0f]),
            (i32::MIN, &[0x80, 0x80, 0x80, 0x80, 0x08]),
        ];
        for (value, bytes) in cases {
            let mut out = Vec::new();
            Write::varint(&mut out, *value);
            assert_eq!(&out, bytes, "encoding {value}");
            assert_eq!(varint_len(*value), bytes.len());
            let mut slice: &[u8] = bytes;
            assert_eq!(Read::varint(&mut slice).unwrap(), *value);
            assert!(slice.is_empty());
        }
    }

    #[test]
    fn varint_rejects_six_bytes() {
        let mut slice: &[u8] = &[0x80, 0x80, 0x80, 0x80, 0x80, 0x01];
        assert!(matches!(
            Read::varint(&mut slice),
            Err(Error::VarIntTooLong)
        ));
    }

    #[test]
    fn strings_and_truncation() {
        let mut out = Vec::new();
        Write::string(&mut out, "§aHi ✫");
        let mut slice: &[u8] = &out;
        assert_eq!(Read::string(&mut slice).unwrap(), "§aHi ✫");
        let mut short: &[u8] = &out[..out.len() - 1];
        assert!(matches!(Read::string(&mut short), Err(Error::Truncated)));
    }
}
