# JW: Detailed Code Walkthrough

A comprehensive linear explanation of how the `jw` CLI tool works, from initialization to execution.

## Overview

**jw** is a high-performance filesystem traversal and file hashing CLI tool built in Rust. It leverages the `jwalk` library for fast directory walking and `xxHash` for extremely fast file hashing. The primary philosophy is: raw speed and simplicity, with minimal abstraction.

---

## Architecture & Flow

### 1. Initialization & Argument Parsing

**File:** `src/main.rs` (lines 1-30)

The program starts by importing dependencies and defining data structures:

```rust
use clap::parser::ValueSource;
use clap::{self, value_parser, Arg, ArgAction, Command};
use jwalk::WalkDir;
use rayon::iter::*;
use std::path::PathBuf;
use std::process::exit;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
};

#[macro_use]
pub mod hashutil;
use hashutil::*;
```

**Key Imports:**
- `clap`: Command-line argument parsing library
- `jwalk::WalkDir`: Parallel filesystem walker
- `rayon`: Data parallelism library for multi-threaded operations
- `hashutil`: Custom module for hashing operations

---

### 2. Options Structure

**File:** `src/main.rs` (lines 40-48)

```rust
#[derive(Clone, Debug)]
struct Options {
    live_print: bool,        // Print results as they're found (slower)
    checksum: Option<HashAlgorithm>,  // Which hash algo to use, if any
    depth: usize,            // Max recursion depth (0 = unlimited)
    exclude: usize,          // Bitmask for what to exclude
    silent: bool,            // Suppress all output
    directories: Vec<String>, // Target directories to traverse
    print_stats: bool,       // Count files/dirs/other at the end
}
```

This struct encapsulates all command-line options, making them easy to pass through the program.

---

### 3. Traversal Mode (Default)

**File:** `src/main.rs` (lines 50-147)

This is the core filesystem walking function when **not** in checksum mode.

#### 3.1 The Exclusion Bitmask System

```rust
const EXCLUDE_FILES: usize = 1;  // 0001
const EXCLUDE_DIRS: usize = 2;   // 0010
const EXCLUDE_HIDDEN: usize = 4;  // 0100
const EXCLUDE_OTHER: usize = 8;   // 1000
```

Rather than storing exclusions as a vector (more memory, slower comparisons), the code uses bitwise flags. For example, if user passes `--exclude files,dot`, the resulting bitmask would be `1 | 4 = 5 (0101)`.

#### 3.2 The Main Walk Loop

The `traverse()` function is called when no checksum is requested:

```rust
fn traverse(options: Options) {
    let exclude = options.exclude;

    for dir in options.directories {
        let max_depth = if options.depth == 0 {
            usize::MAX
        } else {
            options.depth
        };
```

**Note on Design:** A `0` depth means "unlimited recursion," so it's mapped to `usize::MAX`.

#### 3.3 Creating the Walker

```rust
let walker = WalkDir::new(dir)
    .skip_hidden((options.exclude & EXCLUDE_HIDDEN) != 0)
    .max_depth(max_depth)
    .into_iter()
    .filter_map(|entry| {
        entry.ok().and_then(|e| {
            let path = e.path();
            (!((exclude & EXCLUDE_DIRS != 0 && path.is_dir())
                || (exclude & EXCLUDE_FILES != 0 && path.is_file())
                || (exclude & EXCLUDE_OTHER != 0 && (!path.is_dir() && !path.is_file()))))
            .then_some(e)
        })
    });
```

**What's happening:**
1. Create a `WalkDir` from `jwalk` (parallel by default)
2. `.skip_hidden()` filters out dot-files if the user specified `--exclude dot`
3. `.into_iter()` converts it into an iterator
4. `.filter_map()` removes entries that should be excluded:
   - If `EXCLUDE_DIRS` is set and the entry is a directory, filter it out
   - If `EXCLUDE_FILES` is set and the entry is a file, filter it out
   - If `EXCLUDE_OTHER` is set and it's neither file nor dir (symlinks, sockets, etc.), filter it out

#### 3.4 Output Modes

The code then has **three distinct code paths**, not unified with conditionals (a deliberate design choice noted in comments):

**Path 1: Live Print + Stats**
```rust
if options.live_print {
    if options.print_stats {
        for entry in walker {
            let path = entry.path();
            if path.is_file() { file_count += 1; }
            else if path.is_dir() { dir_count += 1; }
            else { other_count += 1; }
            println!("{}", path.display());
        }
    } else {
        // Just live print, no stats
    }
} else {
    // Collect first, then print
}
```

**Design Note:** The developer explicitly chose to repeat the loop logic rather than nest conditionals. This is **intentional**: checking conditions inside a tight loop (O(N) overhead) is slower than duplicating code. They prioritize performance.

**Path 2: Collect + Stats**
```rust
} else {
    let results = walker.collect::<Vec<_>>();
    // Collect all, then process
}
```

Collecting into a Vec means the filesystem walk is fully parallel, only slowing down at the end.

---

### 4. Checksum Mode with Rayon

**File:** `src/main.rs` (lines 149-185)

When the user specifies `--checksum` or `--checksum-with`, this function is called instead:

```rust
fn checksum_rayon(options: &Options, algorithm: &HashAlgorithm) {
    for dir in &options.directories {
        let max_depth = if options.depth == 0 {
            usize::MAX
        } else {
            options.depth
        };

        let walker = WalkDir::new(dir)
            .skip_hidden((options.exclude & EXCLUDE_HIDDEN) != 0)
            .max_depth(max_depth)
            .into_iter()
            .par_bridge()  // ← Key difference: Parallel bridge
            .filter_map(|e| {
                e.ok().and_then(|e| {
                    e.path()
                        .is_file()
                        .then_some(e.path().to_str())
                        .flatten()
                        .map(str::to_string)
                })
            });
```

**Critical Difference:** `.par_bridge()` converts the iterator into a parallel Rayon iterator. This enables multi-threaded hashing across multiple files simultaneously.

The filtering here is **different**: it only keeps files (discards directories), and converts paths to strings for hashing.

#### 4.1 Live Print vs Collected Output

```rust
let hashes: Vec<(String, String)> = if options.live_print {
    walker
        .filter_map(|file_path| {
            hash_file!(algorithm, &file_path)
                .map(|hash| {
                    println!("{}{}", hash, file_path);  // Print immediately
                    (file_path, hash)
                })
                .ok()
        })
        .collect()
} else {
    walker
        .filter_map(|file_path| {
            hash_file!(algorithm, &file_path)
                .map(|hash| (file_path, hash))
                .ok()
        })
        .collect()
};

if !options.silent && !options.live_print {
    for (file_path, hash) in hashes {
        println!("{}{}", hash, file_path);
    }
}
```

**Efficiency Note:** Live printing during parallel iteration can cause I/O contention. The collected approach is faster overall because it lets all threads hash in parallel, then prints afterward.

---

### 5. Hash Difference Mode

**File:** `src/main.rs` (lines 187-285)

This is the `--diff` feature for comparing hash files:

```rust
fn checksum_diff(algorithm: HashAlgorithm, paths: &[String], print_stats: bool) {
    let mut paths = paths.iter();
    
    // Validate inputs
    let base_file: PathBuf = paths.next().and_then(convert).unwrap_or_else(|| {
        eprintln!("Not enough files to perform a diff. Missing the first.");
        exit(1);
    });
    
    let subsequent_files: Vec<PathBuf> = paths
        .next()
        .or_else(|| { ... })
        .into_iter()
        .chain(paths)
        .filter_map(convert)
        .collect();
```

The user provides 2+ hash files. The first is the "correct" baseline; others are compared against it.

#### 5.1 Hash File Parsing

```rust
let digest_length: usize = algorithm.digest_size() * 2;  // Hex is 2x bytes

let read_hashes = |file: &PathBuf| -> HashMap<String, String> {
    let parse_line = |line: String| -> Option<(String, String)> {
        line.split_at_checked(digest_length)
            .map(|(hash, line)| (line.to_string(), hash.to_string()))
    };
    // ...
};
```

Hash files are formatted as: `HASHVALUE/path/to/file`. The code splits at `digest_length` to separate hash from path.

#### 5.2 Comparison Logic

```rust
for (file_name, base_hash) in &base_hashes {
    for (other_hashes, hash_file) in &subsequent_hash_files {
        if let Some(other_hash) = other_hashes.get(file_name) {
            if *other_hash != *base_hash {
                msg_mismatches.push(format!("[!({})] {} != {} == {}",
                    hash_file.display(), other_hash, base_hash, file_name,
                ));
                discrepancies += 1;
            }
        } else {
            msg_missing.push(format!("[-({})] {}", hash_file.display(), file_name));
            discrepancies += 1;
        }
    }
}

// Also check for excess files in other_hashes not in base_hashes
for (other_hashes, hash_file) in &subsequent_hash_files {
    for (file_name, other_hash) in other_hashes {
        if !base_hashes.contains_key(file_name) {
            msg_excess.push(format!("[+({})] {} {}", ...));
            discrepancies += 1;
        }
    }
}
```

Three types of discrepancies are detected:
1. **Mismatches** `[!(...)]`: Hash differs between files
2. **Missing** `[-(...)]`: File in base but not in comparison
3. **Excess** `[+(...)]`: File in comparison but not in base

---

### 6. Command-Line Interface Setup

**File:** `src/main.rs` (lines 287-370)

The `main()` function constructs the CLI:

```rust
fn main() {
    let matches = Command::new("jw")
        .version("2.2.10")
        .about("A CLI frontend to jwalk for blazingly fast filesystem traversal!")
        
        .arg(Arg::new("live-print")
            .long("live")
            .short('l')
            .action(ArgAction::SetTrue)
            .help("Display results in realtime..."))
        
        .arg(Arg::new("checksum")
            .long("checksum")
            .short('c')
            .action(ArgAction::SetTrue)
            .help("Generate an index of file hashes..."))
        
        // ... more args ...
        
        .get_matches();
```

Each arg is defined with long name, short alias, help text, and parsing rules.

#### 6.1 Mode Detection

```rust
if let Some(checksum_files) = matches.get_many::<String>("hdiff").map(|fp| {
    fp.into_iter()
        .map(|s| s.to_string())
        .collect::<Vec<String>>()
}) {
    checksum_diff(...);
    exit(0);
}
```

First check: Is the user in `--diff` mode? If so, run diff and exit.

#### 6.2 Directory Argument Handling

```rust
let mut walk_dirs: Vec<String> = matches
    .get_many::<String>("directories")
    .map(|dirs| dirs.into_iter().map(|s| s.to_string()).collect())
    .expect("No directories provided!");

if walk_dirs.first().is_some_and(|s| s == "--") {
    walk_dirs = read_stdin();
}
```

Special case: If the user passes `--` (double-dash), paths are read from stdin instead:

```rust
fn read_stdin() -> Vec<String> {
    let stdin = std::io::stdin();
    let mut buffer = String::new();
    stdin.read_line(&mut buffer).unwrap();
    buffer
        .split_whitespace()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
}
```

#### 6.3 Exclusion Flags Processing

```rust
let exclude_flags = matches.get_many::<String>("exclude").map_or(0, |flags| {
    flags
        .into_iter()
        .fold(0, |acc, flag| match flag.to_lowercase().as_str() {
            "files" => acc | EXCLUDE_FILES,
            "dirs" => acc | EXCLUDE_DIRS,
            "dot" => acc | EXCLUDE_HIDDEN,
            "other" => acc | EXCLUDE_OTHER,
            _ => acc,
        })
});
```

Each exclude flag is OR'd into the bitmask.

#### 6.4 Checksum Mode Detection

```rust
let checksum_mode = matches!(
    matches.value_source("checksum"),
    Some(ValueSource::CommandLine)
) || matches!(
    matches.value_source("checksum-algo"),
    Some(ValueSource::CommandLine)
);
```

Check if either `--checksum` or `--checksum-with` was explicitly passed.

#### 6.5 Options Construction & Dispatch

```rust
let options = Options {
    live_print: *matches.get_one::<bool>("live-print").unwrap_or(&false),
    exclude: exclude_flags,
    checksum: checksum_mode.then(|| {
        matches
            .get_one::<String>("checksum-algo")
            .map(HashAlgorithm::from)
            .unwrap_or(HashAlgorithm::Xxh3)
    }),
    silent: *matches.get_one::<bool>("silent").unwrap_or(&false),
    depth: *matches.get_one("depth").unwrap_or(&0),
    directories: walk_dirs,
    print_stats: *matches.get_one("stats").unwrap_or(&false),
};

if let Some(algorithm) = &options.checksum {
    checksum_rayon(&options, algorithm);
} else {
    traverse(options);
}
```

Final dispatch: If checksum mode, run `checksum_rayon()`. Otherwise, run `traverse()`.

---

## Hashing Module Deep Dive

**File:** `src/hashutil.rs`

### 1. Algorithm Enum & Digest Sizes

```rust
#[derive(Debug, Clone)]
pub enum HashAlgorithm {
    Xxh3,      // 128-bit (16 bytes)
    Sha224,    // 224-bit (28 bytes)
    Sha256,    // 256-bit (32 bytes)
    Sha384,    // 384-bit (48 bytes)
    Sha512,    // 512-bit (64 bytes)
    Md5,       // 128-bit (16 bytes)
}

impl HashAlgorithm {
    pub fn digest_size(&self) -> usize {
        match self {
            Self::Xxh3 => 16,
            Self::Sha224 => 28,
            // ...
        }
    }
}
```

Each algorithm has a known digest size, critical for parsing hash files in diff mode.

### 2. Hash Algorithm Macro

```rust
macro_rules! hash_file {
    ($algo:expr, $path:expr) => {
        match $algo {
            HashAlgorithm::Xxh3 => hash_file::<Xxh3Default>($path),
            HashAlgorithm::Sha224 => hash_file::<Sha224>($path),
            // ...
        }
    };
}
```

This macro enables **compile-time polymorphism**. Rather than runtime type checking, the macro expands to the correct type at compile time, allowing the generic `hash_file::<H>()` function to be monomorphized.

### 3. The Hash Function (Generic)

```rust
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
```

**Key Optimization:**
- **Files > 20 MB:** Use memory mapping (`mmap`) to let the OS handle efficient large-file I/O
- **Files ≤ 20 MB:** Use buffered reading with 128 KB chunks (good balance)

This adaptive strategy ensures performance across file sizes.

### 4. The Hasher Trait

```rust
pub trait Hasher {
    fn update(&mut self, data: &[u8]);
    fn finalize(self) -> Vec<u8>;
    fn create() -> Self;
}
```

Abstracts the interface for all hashing algorithms (xxHash, SHA2, MD5).

### 5. Implementations for Each Algorithm

For **Xxh3**:
```rust
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
```

Similar implementations exist for Sha224, Sha256, Sha384, Sha512, and Md5Context.

### 6. Hexlify Utility

```rust
pub fn hexlify(digest: Vec<u8>) -> String {
    digest.iter().fold(String::new(), |mut acc, b| {
        write!(acc, "{:02x}", b).unwrap();
        acc
    })
}
```

Converts raw bytes to hexadecimal string (e.g., `[255, 0]` → `"ff00"`).

---

## Performance Insights

### Why This Design is Fast

1. **Parallel Walking:** `jwalk` with `rayon` enables true multi-threaded filesystem traversal.

2. **Parallel Hashing:** `.par_bridge()` on the file iterator hashes multiple files concurrently.

3. **Smart I/O:** Memory mapping for large files, buffered reading for small ones.

4. **Minimal Abstraction:** Direct filesystem operations, no fancy rendering or TUI overhead.

5. **Compile-Time Polymorphism:** The macro system avoids runtime type checking.

6. **Bitwise Exclusions:** Bitmask comparisons are faster than vector lookups.

### Benchmarks (from README)

- **Directory Traversal:** 492 GB in 3 seconds
- **Hashing with xxHash:** 7.2 GB (10K+ files) in 500 ms

---

## Use Cases

### 1. Find All Python Files

```bash
jw --exclude files | grep "\.py$"
```

Walk directories, exclude non-directories, pipe to grep.

### 2. Hash Everything for Backup Verification

```bash
jw --checksum > backup.hashes
# Later...
jw --checksum > current.hashes
jw --diff backup.hashes current.hashes
```

### 3. Find Large Directories

```bash
jw --exclude files --depth 1 /mnt/storage
```

List immediate subdirectories only.

### 4. Live Traversal of Huge Directories

```bash
jw --live /path/to/millions/of/files | head -100
```

Start printing immediately rather than collecting everything first.

---

## Summary

**jw** is a masterclass in Rust performance optimization:

- **Multi-threaded** parallel walking and hashing
- **Adaptive I/O** strategies for files of any size
- **Zero bloat** — does one thing (fast filesystem traversal) extremely well
- **Composable** — outputs pure text for piping to other tools
- **Memory efficient** — uses hashing and bitwise operations instead of allocating vectors

The codebase demonstrates that raw speed and clean design go hand-in-hand.
