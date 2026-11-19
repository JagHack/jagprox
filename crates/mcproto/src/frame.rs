//! Packet framing over a byte stream: VarInt length prefixes, zlib compression and
//! AES-CFB8 encryption. A [`Frame`] keeps the bytes exactly as they arrived (after
//! decryption), so a packet the proxy doesn't care about is forwarded without being
//! decompressed or re-encoded.

use std::borrow::Cow;

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::buf::{Read, Write};
use crate::crypto::Cfb8;
use crate::error::{Error, Result};

/// Largest frame a 3-byte VarInt length allows (the 1.8 protocol limit).
pub const MAX_FRAME: usize = 2_097_151;
/// Largest uncompressed packet accepted (vanilla's limit).
pub const MAX_PACKET: usize = 2_097_152 * 4;

/// One packet as it came off the wire (decrypted, possibly still compressed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    body: Vec<u8>,
    /// Whether compression was on for this connection when the frame was read.
    compression: bool,
}

impl Frame {
    /// A frame holding an uncompressed packet (id + data).
    pub fn from_payload(payload: Vec<u8>) -> Self {
        Self {
            body: payload,
            compression: false,
        }
    }

    /// The packet id, inflating at most a few bytes of a compressed packet.
    pub fn id(&self) -> Result<i32> {
        if !self.compression {
            return (&self.body[..]).varint();
        }
        let mut body = &self.body[..];
        let data_len = body.length()?;
        if data_len == 0 {
            return body.varint();
        }
        let mut head = [0u8; 5];
        let mut inflater = Decompress::new(true);
        let _ = inflater
            .decompress(body, &mut head, FlushDecompress::Sync)
            .map_err(|e| Error::Zlib(e.to_string()))?;
        let produced = inflater.total_out() as usize;
        (&head[..produced]).varint()
    }

    /// The packet id and data, decompressed.
    pub fn payload(&self) -> Result<Cow<'_, [u8]>> {
        if !self.compression {
            return Ok(Cow::Borrowed(&self.body));
        }
        let mut body = &self.body[..];
        let data_len = body.length()?;
        if data_len == 0 {
            return Ok(Cow::Borrowed(body));
        }
        if data_len > MAX_PACKET {
            return Err(Error::FrameTooLarge(data_len));
        }
        let mut out = vec![0u8; data_len];
        let mut inflater = Decompress::new(true);
        let status = inflater
            .decompress(body, &mut out, FlushDecompress::Finish)
            .map_err(|e| Error::Zlib(e.to_string()))?;
        if inflater.total_out() as usize != data_len || status != Status::StreamEnd {
            return Err(Error::Zlib(format!(
                "expected {data_len} bytes, inflated {} ({status:?})",
                inflater.total_out()
            )));
        }
        Ok(Cow::Owned(out))
    }
}

/// Reads frames from a stream, decrypting and splitting them.
pub struct FrameReader<R> {
    inner: R,
    buf: Vec<u8>,
    start: usize,
    cipher: Option<Cfb8>,
    compression: bool,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buf: Vec::with_capacity(16 * 1024),
            start: 0,
            cipher: None,
            compression: false,
        }
    }

    /// Everything read from now on is encrypted (including bytes already buffered).
    pub fn enable_encryption(&mut self, shared_secret: &[u8; 16]) {
        let mut cipher = Cfb8::new(shared_secret);
        cipher.decrypt(&mut self.buf[self.start..]);
        self.cipher = Some(cipher);
    }

    /// Frames read from now on carry a compression header.
    pub fn enable_compression(&mut self) {
        self.compression = true;
    }

    pub fn compression(&self) -> bool {
        self.compression
    }

    fn try_parse(&mut self) -> Result<Option<Frame>> {
        let mut available = &self.buf[self.start..];
        let before = available.len();
        let len = match available.length() {
            Ok(len) => len,
            Err(Error::Truncated) => return Ok(None),
            Err(e) => return Err(e),
        };
        if len > MAX_FRAME {
            return Err(Error::FrameTooLarge(len));
        }
        if available.len() < len {
            return Ok(None);
        }
        let header = before - available.len();
        let body = available[..len].to_vec();
        self.start += header + len;
        Ok(Some(Frame {
            body,
            compression: self.compression,
        }))
    }

    pub async fn read_frame(&mut self) -> Result<Frame> {
        loop {
            if let Some(frame) = self.try_parse()? {
                return Ok(frame);
            }
            if self.start > 0 {
                self.buf.drain(..self.start);
                self.start = 0;
            }
            let old = self.buf.len();
            self.buf.resize(old + 16 * 1024, 0);
            let n = self.inner.read(&mut self.buf[old..]).await?;
            self.buf.truncate(old + n);
            if n == 0 {
                return Err(Error::Closed);
            }
            if let Some(cipher) = &mut self.cipher {
                cipher.decrypt(&mut self.buf[old..]);
            }
        }
    }

    pub fn into_inner(self) -> R {
        self.inner
    }
}

/// Writes frames to a stream. Frames are queued and sent together on [`Self::flush`].
pub struct FrameWriter<W> {
    inner: W,
    cipher: Option<Cfb8>,
    /// Compression threshold, once enabled.
    threshold: Option<usize>,
    out: Vec<u8>,
    compressor: Compress,
}

impl<W: AsyncWrite + Unpin> FrameWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            cipher: None,
            threshold: None,
            out: Vec::with_capacity(16 * 1024),
            compressor: Compress::new(Compression::default(), true),
        }
    }

    pub fn enable_encryption(&mut self, shared_secret: &[u8; 16]) {
        self.cipher = Some(Cfb8::new(shared_secret));
    }

    /// Packets of at least `threshold` bytes are compressed from now on.
    pub fn enable_compression(&mut self, threshold: usize) {
        self.threshold = Some(threshold);
    }

    pub fn compression(&self) -> bool {
        self.threshold.is_some()
    }

    /// Queues a packet (id + data).
    pub fn queue_packet(&mut self, payload: &[u8]) -> Result<()> {
        let Some(threshold) = self.threshold else {
            self.out.length(payload.len());
            self.out.put(payload);
            return Ok(());
        };
        if payload.len() < threshold {
            self.out.length(payload.len() + 1);
            self.out.varint(0);
            self.out.put(payload);
            return Ok(());
        }
        let compressed = self.compress(payload)?;
        let data_len = payload.len() as i32;
        self.out
            .length(crate::buf::varint_len(data_len) + compressed.len());
        self.out.varint(data_len);
        self.out.put(&compressed);
        Ok(())
    }

    fn compress(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        self.compressor.reset();
        let mut out = Vec::with_capacity(payload.len() / 2 + 64);
        loop {
            let consumed = self.compressor.total_in() as usize;
            let status = self
                .compressor
                .compress_vec(&payload[consumed..], &mut out, FlushCompress::Finish)
                .map_err(|e| Error::Zlib(e.to_string()))?;
            if status == Status::StreamEnd {
                return Ok(out);
            }
            out.reserve(out.capacity().max(64));
        }
    }

    /// Queues a frame read from the other side. When both sides use compression the
    /// way it was read, the bytes are forwarded untouched.
    pub fn queue_frame(&mut self, frame: &Frame) -> Result<()> {
        if frame.compression == self.compression() {
            self.out.length(frame.body.len());
            self.out.put(&frame.body);
            Ok(())
        } else {
            let payload = frame.payload()?;
            self.queue_packet(&payload)
        }
    }

    pub fn has_pending(&self) -> bool {
        !self.out.is_empty()
    }

    /// Encrypts and sends everything queued.
    pub async fn flush(&mut self) -> Result<()> {
        if self.out.is_empty() {
            return Ok(());
        }
        if let Some(cipher) = &mut self.cipher {
            cipher.encrypt(&mut self.out);
        }
        self.inner.write_all(&self.out).await?;
        self.inner.flush().await?;
        self.out.clear();
        Ok(())
    }

    /// Queues and immediately sends one packet.
    pub async fn send(&mut self, payload: &[u8]) -> Result<()> {
        self.queue_packet(payload)?;
        self.flush().await
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(id: i32, data: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.varint(id);
        p.put(data);
        p
    }

    async fn round_trip(
        threshold: Option<usize>,
        encrypt: bool,
        packets: &[Vec<u8>],
    ) -> Vec<Frame> {
        let (a, b) = tokio::io::duplex(1 << 20);
        let mut writer = FrameWriter::new(a);
        let mut reader = FrameReader::new(b);
        let secret = [3u8; 16];
        if encrypt {
            writer.enable_encryption(&secret);
            reader.enable_encryption(&secret);
        }
        if let Some(t) = threshold {
            writer.enable_compression(t);
            reader.enable_compression();
        }
        for p in packets {
            writer.queue_packet(p).unwrap();
        }
        writer.flush().await.unwrap();
        let mut frames = Vec::new();
        for _ in packets {
            frames.push(reader.read_frame().await.unwrap());
        }
        frames
    }

    #[tokio::test]
    async fn packets_survive_every_combination() {
        let big: Vec<u8> = (0..5000u32).map(|i| (i % 7) as u8).collect();
        let packets = vec![
            packet(0x02, b"hello"),
            packet(0x21, &big),
            packet(0x00, &[]),
            packet(0x3f, &big[..300]),
        ];
        for threshold in [None, Some(256), Some(0)] {
            for encrypt in [false, true] {
                let frames = round_trip(threshold, encrypt, &packets).await;
                for (frame, original) in frames.iter().zip(&packets) {
                    assert_eq!(
                        frame.payload().unwrap().as_ref(),
                        &original[..],
                        "{threshold:?} {encrypt}"
                    );
                    assert_eq!(frame.id().unwrap(), (&original[..]).varint().unwrap());
                }
            }
        }
    }

    #[tokio::test]
    async fn compressed_frames_forward_byte_for_byte() {
        let big: Vec<u8> = (0..5000u32).map(|i| (i % 7) as u8).collect();
        let original = packet(0x21, &big);
        let frames = round_trip(Some(256), true, std::slice::from_ref(&original)).await;
        let (a, b) = tokio::io::duplex(1 << 20);
        let mut writer = FrameWriter::new(a);
        writer.enable_compression(256);
        writer.queue_frame(&frames[0]).unwrap();
        writer.flush().await.unwrap();
        let mut reader = FrameReader::new(b);
        reader.enable_compression();
        let forwarded = reader.read_frame().await.unwrap();
        assert_eq!(forwarded, frames[0], "raw bytes passed through");
        assert_eq!(forwarded.payload().unwrap().as_ref(), &original[..]);
    }

    #[tokio::test]
    async fn forwarding_between_different_compression_settings_re_encodes() {
        let original = packet(0x02, &[9u8; 600]);
        let frames = round_trip(Some(256), false, std::slice::from_ref(&original)).await;
        let (a, b) = tokio::io::duplex(1 << 20);
        let mut writer = FrameWriter::new(a);
        writer.queue_frame(&frames[0]).unwrap();
        writer.flush().await.unwrap();
        let frame = FrameReader::new(b).read_frame().await.unwrap();
        assert_eq!(frame.payload().unwrap().as_ref(), &original[..]);
    }

    #[tokio::test]
    async fn closed_stream_is_reported() {
        let (a, b) = tokio::io::duplex(64);
        drop(a);
        assert!(matches!(
            FrameReader::new(b).read_frame().await,
            Err(Error::Closed)
        ));
    }
}
