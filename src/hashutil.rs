use std::fmt::Write;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};

pub use md5::{Context as Md5Context, Digest as Md5Digest};

use sha2::Digest as ShaDigest;
pub use sha2::{Sha224, Sha256, Sha384, Sha512};

pub use xxhash_rust::xxh3::Xxh3Default; // <3

use memmap3::Mmap;


#[derive(Debug, Clone)]
pub enum HashAlgorithm {
    Xxh3,
    Blake3,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Md5,
}

impl HashAlgorithm {
    pub fn digest_size(&self) -> usize {
        match self {
            Self::Xxh3 => 16,
            Self::Blake3 => 32,
            Self::Sha224 => 28,
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
            Self::Md5 => 16,
        }
    }
}

impl From<&String> for HashAlgorithm {
    fn from(s: &String) -> Self {
        match s.to_lowercase().as_str() {
            "xxh3" => Self::Xxh3,
            "blake3" => Self::Blake3,
            "sha224" => Self::Sha224,
            "sha256" => Self::Sha256,
            "sha384" => Self::Sha384,
            "sha512" => Self::Sha512,
            "md5" => Self::Md5,
            _ => panic!("Invalid hash algorithm! '{}'", s),
        }
    }
}

macro_rules! hash_file {
    ($algo:expr, $path:expr) => {
        match $algo {
            HashAlgorithm::Xxh3 => hash_file::<Xxh3Default>($path),
            HashAlgorithm::Blake3 => hash_file_blake3($path),
            HashAlgorithm::Sha224 => hash_file::<Sha224>($path),
            HashAlgorithm::Sha256 => hash_file::<Sha256>($path),
            HashAlgorithm::Sha384 => hash_file::<Sha384>($path),
            HashAlgorithm::Sha512 => hash_file::<Sha512>($path),
            HashAlgorithm::Md5 => hash_file::<Md5Context>($path),
        }
    };
}

/// Special-case file hashing for BLAKE3 so we can use update_mmap_rayon when appropriate.
pub fn hash_file_blake3(path: &String) -> std::io::Result<String> {
    // Open and get size as you already do
    let mut file = File::open(path)?;
    let _ = file.seek(SeekFrom::End(0));
    let file_size = file.stream_position().unwrap_or(0);
    let _ = file.seek(SeekFrom::Start(0));

    // Threshold to switch to multithreaded update; tune for your CPU/dataset.
    // The blake3 crate notes update_rayon tends to be slower under ~128 KiB.
    const RAYON_THRESHOLD: u64 = 128 * 1024; // 128 KiB

    let mut hasher = blake3::Hasher::new();

    if file_size >= RAYON_THRESHOLD {
        // update_mmap_rayon requires `mmap` + `rayon` features on the crate.
        // It will memory-map and use a rayon-based internal strategy.
        // It returns a Result<&mut Hasher, std::io::Error>
        hasher.update_mmap_rayon(path)?;
    } else {
        // small files: map where helpful; otherwise single-thread update
        let mmap = unsafe { Mmap::map(&file)? };
        // let mut hh = Xxh3Default::create();
        // hh.update(input);
        hasher.update(&mmap);
    }

    Ok(hexlify(hasher.finalize().as_bytes().to_vec()))
}

pub fn hash_file<H: Hasher>(path: &String) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = H::create();

    let _ = file.seek(SeekFrom::End(0));
    let file_size = file.stream_position().ok().unwrap();
    let _ = file.seek(SeekFrom::Start(0));

    if file_size > (1024*1024)*20 {
        let mmap = unsafe { Mmap::map(&file)? };
        hasher.update(&mmap);
    } 

    // Read in 128kb chunks
    else {
        let mut reader = BufReader::new(file);
        let mut buffer = vec![0; 128*1024];

        while let Ok(bytes_read) = reader.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }
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
    digest.iter().fold(String::new(), |mut acc, b| {
        write!(acc, "{:02x}", b).unwrap();
        acc
    })
}

pub fn get_random_bytes(count: usize) -> Vec<u8> {
    let file = File::open("/dev/urandom").unwrap();
    let mut reader = BufReader::new(file);
    let mut buffer = vec![0; count];
    let _ = reader.read_exact(&mut buffer);
    buffer
}
