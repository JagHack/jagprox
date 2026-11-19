//! Login encryption: AES-128-CFB8 stream cipher, the RSA key exchange and
//! Minecraft's signed-SHA1 "server hash".

use aes::Aes128;
use aes::cipher::{BlockEncrypt, KeyInit};
use rand_core::{OsRng, RngCore};
use rsa::pkcs8::{DecodePublicKey, EncodePublicKey};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use sha1::{Digest, Sha1};

use crate::error::{Error, Result};

/// AES-128 in CFB-8 mode, as Minecraft uses it: key and IV are both the shared
/// secret, and one cipher state runs for the whole connection in each direction.
pub struct Cfb8 {
    aes: Aes128,
    register: [u8; 16],
}

impl Cfb8 {
    pub fn new(shared_secret: &[u8; 16]) -> Self {
        Self {
            aes: Aes128::new(shared_secret.into()),
            register: *shared_secret,
        }
    }

    fn keystream_byte(&self) -> u8 {
        let mut block = self.register.into();
        self.aes.encrypt_block(&mut block);
        block[0]
    }

    fn shift_in(&mut self, byte: u8) {
        self.register.copy_within(1.., 0);
        self.register[15] = byte;
    }

    pub fn encrypt(&mut self, data: &mut [u8]) {
        for byte in data {
            *byte ^= self.keystream_byte();
            self.shift_in(*byte);
        }
    }

    pub fn decrypt(&mut self, data: &mut [u8]) {
        for byte in data {
            let cipher = *byte;
            *byte ^= self.keystream_byte();
            self.shift_in(cipher);
        }
    }
}

/// Minecraft's server hash: SHA-1 of server id + shared secret + public key, printed
/// as a signed (two's complement) big-endian hex number without leading zeros.
pub fn server_hash(server_id: &str, shared_secret: &[u8], public_key_der: &[u8]) -> String {
    let mut sha = Sha1::new();
    sha.update(server_id.as_bytes());
    sha.update(shared_secret);
    sha.update(public_key_der);
    signed_hex(&sha.finalize())
}

fn signed_hex(digest: &[u8]) -> String {
    let mut bytes = digest.to_vec();
    let negative = bytes[0] & 0x80 != 0;
    if negative {
        // Two's complement negate.
        let mut carry = true;
        for b in bytes.iter_mut().rev() {
            *b = !*b;
            if carry {
                let (v, overflow) = b.overflowing_add(1);
                *b = v;
                carry = overflow;
            }
        }
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let trimmed = hex.trim_start_matches('0');
    let trimmed = if trimmed.is_empty() { "0" } else { trimmed };
    if negative {
        format!("-{trimmed}")
    } else {
        trimmed.to_string()
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

/// The proxy's own key pair for players logging in to it.
pub struct ServerKey {
    private: RsaPrivateKey,
    public_der: Vec<u8>,
}

impl ServerKey {
    /// A fresh 1024-bit key, as vanilla servers use.
    pub fn generate() -> Result<Self> {
        let private =
            RsaPrivateKey::new(&mut OsRng, 1024).map_err(|e| Error::Crypto(e.to_string()))?;
        let public_der = RsaPublicKey::from(&private)
            .to_public_key_der()
            .map_err(|e| Error::Crypto(e.to_string()))?
            .as_bytes()
            .to_vec();
        Ok(Self {
            private,
            public_der,
        })
    }

    /// The public key in X.509 SubjectPublicKeyInfo DER form, as sent on the wire.
    pub fn public_der(&self) -> &[u8] {
        &self.public_der
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.private
            .decrypt(Pkcs1v15Encrypt, data)
            .map_err(|e| Error::Crypto(e.to_string()))
    }
}

/// Encrypts `data` with a server's DER public key (the upstream login).
pub fn encrypt_with_public_der(public_key_der: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let key = RsaPublicKey::from_public_key_der(public_key_der)
        .map_err(|e| Error::Crypto(e.to_string()))?;
    key.encrypt(&mut OsRng, Pkcs1v15Encrypt, data)
        .map_err(|e| Error::Crypto(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha_hex(name: &str) -> String {
        signed_hex(&Sha1::digest(name.as_bytes()))
    }

    #[test]
    fn known_minecraft_digests() {
        // Reference values from wiki.vg.
        assert_eq!(sha_hex("Notch"), "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48");
        assert_eq!(sha_hex("jeb_"), "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1");
        assert_eq!(sha_hex("simon"), "88e16a1019277b15d58faf0541e11910eb756f6");
    }

    #[test]
    fn cfb8_matches_openssl() {
        // Node: createCipheriv("aes-128-cfb8", key, key) with key[i] = i*17, plain[i] = i*7+3.
        let key: [u8; 16] = std::array::from_fn(|i| (i * 17 % 256) as u8);
        let mut data: Vec<u8> = (0..64).map(|i| ((i * 7 + 3) % 256) as u8).collect();
        Cfb8::new(&key).encrypt(&mut data);
        let hex: String = data.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "618316e65f45e37f5de1547bd1d39c995ab55291b6abd2622d91b327cea53a17\
             815ef8bc7d6e8a9d2030d2dfed2d5dc610df8bf4e2bc51e902f02ee59ac72017"
        );
    }

    #[test]
    fn cfb8_round_trips_across_chunk_boundaries() {
        let key = [7u8; 16];
        let plain: Vec<u8> = (0..1000u32).map(|i| (i * 31 % 251) as u8).collect();
        let mut enc = Cfb8::new(&key);
        let mut data = plain.clone();
        let (a, b) = data.split_at_mut(333);
        enc.encrypt(a);
        enc.encrypt(b);
        let mut dec = Cfb8::new(&key);
        let (a, b) = data.split_at_mut(10);
        dec.decrypt(a);
        dec.decrypt(b);
        assert_eq!(data, plain);
    }

    #[test]
    fn rsa_key_exchange_round_trips() {
        let key = ServerKey::generate().unwrap();
        let secret: [u8; 16] = random_bytes();
        let encrypted = encrypt_with_public_der(key.public_der(), &secret).unwrap();
        assert_eq!(key.decrypt(&encrypted).unwrap(), secret);
    }
}
