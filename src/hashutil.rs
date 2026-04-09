use std::default;
use std::fmt::Write;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::ops::Deref;
use std::time::Duration;

pub use md5::{Context as Md5Context, Digest as Md5Digest};

use rayon::iter::ParallelIterator;
use rayon::slice::{ParallelSlice, ParallelSliceMut};
use sha2::Digest as ShaDigest;
pub use sha2::{Sha224, Sha256, Sha384, Sha512};

pub use xxhash_rust::xxh3::Xxh3Default; // <3

#[cfg(feature = "gxhash")]
pub use gxhash::GxHasher;

#[cfg(feature = "rapidhash")]
use rapidhash::v3;

use memmap3::Mmap;
// use memmap2::Mmap;

#[derive(Debug, Clone)]
pub enum HashAlgorithm {
    Xxh3,
    #[cfg(feature = "gxhash")]
    GxHash,
    #[cfg(feature = "rapidhash")]
    RapidHash,
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
            #[cfg(feature = "gxhash")]
            Self::GxHash => 16,
            #[cfg(feature = "rapidhash")]
            Self::RapidHash => 16,
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
            #[cfg(feature = "gxhash")]
            "gxhash" => Self::GxHash,
            #[cfg(feature = "rapidhash")]
            "rapidhash" => Self::RapidHash,
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
            #[cfg(feature = "gxhash")]
            HashAlgorithm::GxHash => hash_file::<GxHasher>($path),
            #[cfg(feature = "rapidhash")]
            HashAlgorithm::RapidHash => hash_file_rapidhash($path),
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

    let mmap = unsafe { Mmap::map(&file)? };
    hasher.update(&mmap);

    // if // file_size > 4096 { // (1024 * 1024) * 1 {
    // }
    // // Read in 128kb chunks
    // else {
    //     let mut reader = BufReader::new(file);
    //     let mut buffer = vec![0; 128 * 1024];
    //
    //     while let Ok(bytes_read) = reader.read(&mut buffer) {
    //         if bytes_read == 0 {
    //             break;
    //         }
    //
    //         hasher.update(&buffer[..bytes_read]);
    //     }
    // }

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

#[cfg(feature = "gxhash")]
const GXHASH_FIXED_SEED: i64 = 0xdead_beef_c0de_cafe; // pick a constant seed

// implement your crate's Hasher trait for gxhash::GxHasher
#[cfg(feature = "gxhash")]
impl Hasher for GxHasher {
    fn update(&mut self, data: &[u8]) {
        // GxHasher exposes `write(&[u8])`
        use std::hash::Hasher as StdHasher;
        self.write(data);
    }

    fn finalize(self) -> Vec<u8> {
        // use finish_u128 (128-bit output), normalize to BE bytes to be explicit
        let v: u128 = self.finish_u128();
        v.to_be_bytes().to_vec()
    }

    fn create() -> Self {
        // create deterministically using a fixed seed
        // You can also call GxHasher::default() if you'd rather rely on crate default (random)
        GxHasher::with_seed(GXHASH_FIXED_SEED)
    }
}

#[cfg(feature = "rapidhash")]
pub fn hash_file_rapidhash(path: &String) -> std::io::Result<String> {
    use memmap3::Mmap;
    use std::fs::File;
    use std::io::Seek;
    use std::io::SeekFrom;

    let p = std::path::Path::new(path);

    let mut file = File::open(p)?;
    // seek for size (like your other code)
    let _ = file.seek(SeekFrom::End(0));
    let _ = file.seek(SeekFrom::Start(0));

    // map the file and hash the bytes in one shot
    let mmap = unsafe { Mmap::map(&file)? };

    // rapidhash v3 single-shot; portable/stable output
    let h: u64 = v3::rapidhash_v3(&mmap);

    Ok(hexlify(h.to_be_bytes().to_vec()))
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

use crossbeam_channel as cb;
use std::sync::{self, Arc};
use std::sync::{mpsc, Mutex};
use std::thread::{self, JoinHandle};

struct OrderedChunk {
    piece: Vec<u8>,
    order: u64,
}

pub fn hash_xxh3_reference(path: &String) -> std::io::Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut data: Vec<u8> = vec![];
    let r = reader.read_to_end(&mut data)?;

    let mut hasher = Xxh3Default::create();
    hasher.update(&data);

    let result = hasher.finalize();
    return Ok(hexlify(result));
}

/*
 * t1  t2  t3  t4
 * ^   ^   ^   ^
 * fb1 fb2 fb3 fb4
 * --- --- --- ---
 * bb1 bb2 bb3 bb4
 * _______________
 * bb
 * ^R f
 *
 *
 * Main thread manages the reading. Each thread waits until data is in front
 * buffer. If there is, it begins hashing it. Main thread immediately starts to
 * read new data into the back buffer, and when the thread sends the result to
 * corresponding channel, main thread swaps front and back buffer, and tells the
 * thread to start reading again or the thread realises there's data in the front
 * buffer itself, and starts hashing, repeat, for every thread, until the total
 * amount of bytes processed is >= to the file size. This requires multiple front
 * and back buffers and channels to communicate with the main thread in a way where
 * the hashing thread can be identified and the appropriate buffer filled. It can
 * also be one channel, as long as it sends a struct / data with a corresponding ID
 * for the thread. At first, the main thread fills up all buffers, before letting the
 * threads know that they should read. In C I would read everything in one go in one
 * buffer, and that buffer would be the same as the front/back buffer as they are just
 * sub-buffers / offsets within the main buffer that the main thread dumped everything
 * into.
 *
 * */
// #[derive(Debug)]
// struct Message {}

#[derive(Debug)]
enum Message {
    DataPending(Arc<Mutex<Vec<u8>>>),
    DataComplete(usize, Vec<u8>),
    Die,
}

#[derive(Debug)]
struct BiChannel<T> {
    sender: cb::Sender<T>,
    receiver: cb::Receiver<T>,
}

impl<T> BiChannel<T> {
    fn new() -> BiChannel<T> {
        let (tx, rx) = cb::unbounded::<T>();
        BiChannel::<T> {
            receiver: rx,
            sender: tx,
        }
    }
}

#[derive(Debug)]
struct HashThread {
    jh: Option<JoinHandle<()>>,
    fb: Arc<Mutex<Vec<u8>>>,
    bb: Arc<Mutex<Vec<u8>>>,
    work: BiChannel<Message>,
    results: BiChannel<Message>,
    using_back_buffer: bool,
}

impl HashThread {
    fn new(bufsize: usize) -> HashThread {
        let fb = Arc::new(Mutex::new(vec![0u8; bufsize]));
        let bb = Arc::new(Mutex::new(vec![0u8; bufsize]));
        let ab = Arc::clone(&fb);

        let work = BiChannel::<Message>::new();
        let results = BiChannel::<Message>::new();

        let ht = HashThread {
            fb,
            bb,
            jh: None,
            work,
            results,
            using_back_buffer: false,
        };

        ht
    }
}

pub fn hash_xxh3_pfm_pool_fbbb(path: &String) -> std::io::Result<String> {
    let thread_count = 6;

    let mut threads: Vec<HashThread> = vec![];

    for i in 0..thread_count {
        let mut ht = HashThread::new((1024 * 1024) * 512);

        let rx = ht.work.receiver.clone();
        let tx = ht.results.sender.clone();

        ht.jh = Some(thread::spawn(move || {
            let rx = rx;
            let tx = tx;

            loop {
                let buffer: Arc<Mutex<Vec<u8>>> = match rx.recv_timeout(Duration::from_millis(1500))
                {
                    Ok(m) => match m {
                        Message::DataPending(am) => {
                            eprintln!("Got pending data");
                            am
                        }
                        Message::Die => break,
                        o @ _ => {
                            eprintln!("Something else: {:?}", o);
                            continue;
                        }
                    },
                    Err(e) => {
                        eprintln!("Error! {}", e);
                        continue;
                    }
                };

                let mut hasher = Xxh3Default::create();

                hasher.update(&buffer.lock().unwrap());
                let size = buffer.lock().unwrap().len();
                let result = hasher.finalize();

                eprintln!(
                    "Sent complete message for {}, result {}",
                    size,
                    hexlify(result.clone())
                );

                let _ = tx.send(Message::DataComplete(size, result));
            }
        }));

        threads.push(ht);
    }

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let file_size: u64 = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;

    let mut reads = 0;

    for t in &threads {
        let n1 = reader.read(&mut t.fb.lock().unwrap())?;
        let n2 = reader.read(&mut t.bb.lock().unwrap())?;
        reads += 1;

        if n1 + n2 > file_size as usize {
            break;
        }

        eprintln!("R1:{},R2:{}", n1, n2);
        //
        {
            let fb: Vec<u8> = t.fb.lock().unwrap().clone();
            let bb: Vec<u8> = t.bb.lock().unwrap().clone();

            eprintln!(
                "FB: {}\nBB: {}",
                hexlify(fb.clone()[0..100].to_vec()),
                hexlify(bb.clone()[0..100].to_vec())
            );
        }
    }

    let mut i = 0;
    for mut t in &mut threads {
        if i > reads {
            break;
        }
        

        let _ = t.work.sender.send(Message::DataPending(Arc::clone(&t.fb)));
        i += 1;
    }

    let mut processed: usize = 0usize;

    let mut hasher = Xxh3Default::create();

    while processed < file_size as usize {
        for mut t in &mut threads {
            if let Ok(Message::DataComplete(sz, h)) =
                t.results.receiver.recv_timeout(Duration::from_millis(1))
            {
                processed += sz;
                eprintln!("Received chunk: {}", hexlify(h.clone()));
                hasher.update(&h);

                if t.using_back_buffer {
                    t.using_back_buffer = false;
                    t.work
                        .sender
                        .send(Message::DataPending(Arc::clone(&mut t.fb)));
                    let _ = reader.read_exact(&mut t.bb.lock().unwrap());
                    eprintln!("Performed read to bb");
                } else {
                    t.using_back_buffer = true;
                    t.work
                        .sender
                        .send(Message::DataPending(Arc::clone(&mut t.bb)));
                    let _ = reader.read_exact(&mut t.fb.lock().unwrap());
                    eprintln!("Performed read to fb");
                }
            }
        }
    }
    for t in &mut threads {
        eprintln!("Telling threads to die");
        t.work.sender.send(Message::Die);
    }

    for t in threads {
        eprintln!("Joining thread..");
        if let Some(h) = t.jh {
            let _ = h.join();
        }
    }

    let result = hasher.finalize();

    return Ok(hexlify(result));
}

pub fn hash_xxh3_pfm_pool(path: &String) -> std::io::Result<String> {
    let fmb = Arc::new(Mutex::new(BufReader::new(File::open(path)?)));

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let fsize: usize = reader.seek(SeekFrom::End(0))? as usize;
    reader.seek(SeekFrom::Start(0))?;

    let thread_count = 12;
    let chunk_size = (1024 * 1024 * 1024); // 1 GiB

    let mut pool: Vec<JoinHandle<()>> = vec![];

    let (tx, rx) = cb::unbounded::<OrderedChunk>();
    let mut die = Arc::new(sync::atomic::AtomicBool::new(false));

    let mut bbuf: Box<Vec<u8>> = Box::new(vec![]);

    let mut arcbuf: Arc<Mutex<Box<Vec<u8>>>> =
        Arc::new(Mutex::new(Box::new(vec![0u8; chunk_size])));

    let mut bufp: *const Arc<Mutex<Box<Vec<u8>>>> = &arcbuf;

    // let fm = Arc::new(Mutex::new(&reader));

    for i in 0..thread_count {
        let rx = rx.clone();
        let tx = tx.clone();

        let die = Arc::clone(&die);
        let fm2 = Arc::clone(&fmb);
        let ab2 = Arc::clone(&arcbuf);

        pool.push(thread::spawn(move || {
            let ab2 = ab2;

            let fm = fm2;
            let mut hasher = Xxh3Default::create();

            loop {
                if die.load(sync::atomic::Ordering::Relaxed) {
                    break;
                }

                let mut buf: Vec<u8> = vec![0u8; chunk_size];

                {
                    let g = fm.lock().unwrap().read(&mut buf);
                }

                hasher.update(&buf);
            }

            let h = hasher.finalize();
            tx.send(OrderedChunk { order: 0, piece: h });
        }));
    }

    let mut hasher = Xxh3Default::create();
    let mut total = 0usize;

    while total < fsize {
        reader.read(&mut arcbuf.lock().unwrap());

        let r = match rx.recv() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Err in recv {}", e);
                continue;
            }
        };

        hasher.update(&r.piece);
        total += r.piece.len()
    }

    die.store(true, sync::atomic::Ordering::Relaxed);

    for t in pool {
        let _ = t.join();
    }

    let result = hasher.finalize();
    return Ok(hexlify(result));
}

pub fn hash_xxh3_pfm(path: &String) -> std::io::Result<String> {
    let mut data: Vec<u8> = vec![];

    let size: usize;

    {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let read_r = reader.read_to_end(&mut data)?;
        size = read_r;
    }

    let mut dataA: Arc<Box<Vec<u8>>> = Arc::new(Box::new(data));

    let cpu_count = 10;
    let thread_count = cpu_count;
    let chunk_size: usize = (size / thread_count) as usize;

    let mut threads: Vec<thread::JoinHandle<()>> = vec![];
    // let chunk_iter = data.chunks(chunk_size);

    let mut started = 0;

    let (tx, rx) = cb::unbounded::<OrderedChunk>();
    //
    // let chunks: Vec<(usize, &[u8])> = dataA.chunks(chunk_size).enumerate().collect();
    // let bchunks = Arc::new(Box::new(chunks.clone()));

    // for (i, chunk) in data
    //     .par_chunks(chunk_size)
    //     .collect::<Vec<_>>()
    //     .into_iter()
    //     .enumerate()
    // {
    //     let mut hasher = Xxh3Default::create();
    //     hasher.update(&chunk);
    //     let finalized = hasher.finalize();
    //     //
    //     let r = PfmResult {
    //         piece: finalized,
    //         order: i as u64,
    //     };
    //     //
    //     if let Err(e) = tx.send(r) {
    //         eprintln!("Error in thread {} when sending PfmResult: {}", i, e);
    //     }
    // }

    // for (i, c) in chunks {
    //     // let bc = Arc::clone(&bchunks);
    //     let da = Arc::clone(&dataA);
    //
    //     thread::spawn(move || {
    //         let da = da;
    //
    //     });
    // }
    //
    //

    // for (i, c) in dataA.chunks(chunk_size).enumerate() {
    for i in 0..dataA.len() / chunk_size {
        let da = Arc::clone(&dataA);
        let ttx = tx.clone();

        // let piece = c;

        threads.push(thread::spawn(move || {
            let da = da;
            let piece = &da[chunk_size * i..(chunk_size * i) + chunk_size];

            let sender = ttx;
            let order = i as u64;
            let piece = piece;

            let mut hasher = Xxh3Default::create();
            hasher.update(&piece);
            let finalized = hasher.finalize();

            let r = OrderedChunk {
                piece: finalized,
                order: order,
            };

            if let Err(e) = sender.send(r) {
                eprintln!("Error in thread {} when sending PfmResult: {}", order, e);
            }
        }));

        started += 1;
    }

    let mut pending = started;
    let mut results: Vec<OrderedChunk> = vec![];

    while pending > 0 {
        let mnext = rx.recv_timeout(Duration::from_millis(2500));

        let pfmr: OrderedChunk = if let Ok(r) = mnext {
            r
        } else {
            eprintln!("recv timed out after 2.5s");
            continue;
        };

        results.push(pfmr);
        pending -= 1;

        eprintln!("Pending -1: {}", pending);
    }

    results.sort_by(|a: &OrderedChunk, b: &OrderedChunk| a.order.cmp(&b.order));

    let mut hasher = Xxh3Default::create();

    for r in &results {
        println!("{}: {}", r.order, hexlify(r.piece.clone()));
        hasher.update(&r.piece);
    }

    let hash = hasher.finalize();

    for t in threads {
        eprintln!("Joining {} threads", started);
        let _ = t.join();
    }

    return Ok(hexlify(hash));
}
