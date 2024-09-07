use std::fs::{self, File};
use std::io::{BufReader, Read, Write};

use sha2::digest::Update;
use xxhash_rust::const_xxh3::xxh3_64 as const_xxh3;
use xxhash_rust::xxh3::{self, xxh3_128, xxh3_64, Xxh3, Xxh3Default};

use sha2::{self, digest, Digest as ShaDigest};

pub use md5::{self, Context as Md5Context, Digest as Md5Digest};
pub use sha2::{Sha224, Sha256, Sha384, Sha512};

use anyhow as ah;

#[derive(Debug, Clone)]
pub enum HashAlgorithm {
    Xxh3,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Md5,
}

macro_rules! hash_file {
    ($algo:expr, $path:expr) => {
        match $algo {
            HashAlgorithm::Xxh3   => hash_file::<Xxh3Default>($path),
            HashAlgorithm::Sha224 => hash_file::<Sha224>($path),
            HashAlgorithm::Sha256 => hash_file::<Sha256>($path),
            HashAlgorithm::Sha384 => hash_file::<Sha384>($path),
            HashAlgorithm::Sha512 => hash_file::<Sha512>($path),
            HashAlgorithm::Md5    => hash_file::<Md5Context>($path),
        }
    };
}

pub fn hash_file<H: Hasher>(path: &String) -> ah::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = vec![0; 4096];

    let mut hasher = H::create();

    while let Ok(bytes_read) = reader.read(&mut buffer) {
        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hexlify(hasher.finalize()))
}

pub trait Hasher {
    fn update(&mut self, data: &[u8]);
    fn finalize(self) -> Vec<u8>;
    fn create() -> Self;
}

impl Hasher for Xxh3Default {
    fn update(&mut self, data: &[u8]) {
        self.update(data);
    }

    fn finalize(self) -> Vec<u8> {
        Xxh3Default::digest128(&self).to_ne_bytes().to_vec()
    }

    fn create() -> Self {
        Xxh3Default::default()
    }
}

impl Hasher for Sha224 {
    fn update(&mut self, data: &[u8]) {
        ShaDigest::update(self, data);
    }

    fn finalize(self) -> Vec<u8> {
        ShaDigest::finalize(self).to_vec()
    }

    fn create() -> Self {
        Sha224::default()
    }
}

impl Hasher for Sha256 {
    fn update(&mut self, data: &[u8]) {
        ShaDigest::update(self, data);
    }

    fn finalize(self) -> Vec<u8> {
        ShaDigest::finalize(self).to_vec()
    }

    fn create() -> Self {
        Sha256::default()
    }
}

impl Hasher for Sha384 {
    fn update(&mut self, data: &[u8]) {
        ShaDigest::update(self, data);
    }

    fn finalize(self) -> Vec<u8> {
        ShaDigest::finalize(self).to_vec()
    }

    fn create() -> Self {
        Sha384::default()
    }
}

impl Hasher for Sha512 {
    fn update(&mut self, data: &[u8]) {
        ShaDigest::update(self, data);
    }

    fn finalize(self) -> Vec<u8> {
        ShaDigest::finalize(self).to_vec()
    }

    fn create() -> Self {
        Sha512::default()
    }
}

impl Hasher for Md5Context {
    fn update(&mut self, data: &[u8]) {
        Md5Context::consume(self, data)
    }

    fn finalize(self) -> Vec<u8> {
        Md5Context::compute(self).0.to_vec()
    }

    fn create() -> Self {
        Md5Context::new()
    }
}


pub fn hexlify(digest: Vec<u8>) -> String {
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn get_random_bytes(count: usize) -> Vec<u8> {
    let file = File::open("/dev/urandom").unwrap();
    let mut reader = BufReader::new(file);
    let mut buffer = vec![0; count];
    reader.read_exact(&mut buffer);
    buffer
}
