use clap::{self, value_parser, Arg, ArgAction, Command};
use crossbeam_channel::unbounded;
use jwalk::WalkDir;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{spawn, JoinHandle};
use std::time::Duration;

#[macro_use]
pub mod hashutil;
use hashutil::*;

fn read_stdin() -> Vec<String> {
    let stdin = std::io::stdin();
    let mut buffer = String::new();

    stdin.read_line(&mut buffer).unwrap();

    buffer
        .split_whitespace()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
}

const EXCLUDE_FILES: usize = 1;
const EXCLUDE_DIRS: usize = 2;
const EXCLUDE_HIDDEN: usize = 4;
const EXCLUDE_OTHER: usize = 8;

#[derive(Clone, Debug)]
struct Options {
    live_print: bool,
    checksum: Option<HashAlgorithm>,
    checksum_threads: usize,
    depth: usize,
    exclude: usize,
    directories: Vec<String>,
}

fn traverse(options: Options) {
    let exclude = options.exclude;

    for dir in options.directories {
        let max_depth = if options.depth == 0 {
            usize::MAX
        } else {
            options.depth
        };

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

        if options.live_print {
            for entry in walker {
                println!("{}", entry.path().display());
            }
        } else {
            let results = walker.collect::<Vec<_>>();

            for entry in results {
                println!("{}", entry.path().display());
            }
        }
    }
}

fn checksum(options: &Options, algorithm: &HashAlgorithm) {
    let mut hasher_threads: Vec<JoinHandle<()>> = vec![];
    let (send_paths_tx, receive_paths_rx) = unbounded::<String>();
    let (send_hashes_tx, receive_hashes_rx) = unbounded::<String>();
    let thread_count = options.checksum_threads;

    let done_walking = Arc::new(AtomicBool::new(false));

    for _ in 0..thread_count {
        let send_hashes_tx = send_hashes_tx.clone();
        let receive_paths_rx = receive_paths_rx.clone();
        let done_walking = Arc::clone(&done_walking);
        let algorithm = algorithm.clone();

        let handle = spawn(move || {
            while !done_walking.load(Ordering::SeqCst) || !receive_paths_rx.is_empty() {
                let path = match receive_paths_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let hash = hash_file!(algorithm, &path);

                if let Ok(hash) = hash {
                    let formatted = format!("{}:{}", hash, path);
                    if let Err(e) = send_hashes_tx.send(formatted) {
                        eprintln!(
                            "Failed to send hash through channel, breaking thread loop: {}",
                            e
                        );
                        break;
                    }
                }
            }
        });

        hasher_threads.push(handle);
    }

    let mut printer_thread: Option<JoinHandle<()>> = None;

    if options.live_print {
        let done_walking = Arc::clone(&done_walking);

        let receive_hashes_rx = receive_hashes_rx.clone();

        printer_thread = Some(spawn(move || {
            while !done_walking.load(Ordering::SeqCst) || !receive_paths_rx.is_empty() {
                match receive_hashes_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(h) => println!("{}", h),
                    Err(_) => continue,
                };
            }
        }));
    }

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
            .filter_map(|entry| entry.ok().and_then(|e| e.path().is_file().then_some(e)));

        for entry in walker {
            if let Err(e) = send_paths_tx.send(entry.path().to_string_lossy().to_string()) {
                eprintln!("Failed to send path through channel: {}", e);
            }
        }
    }

    done_walking.store(true, Ordering::SeqCst);

    for handle in hasher_threads {
        handle.join().unwrap();
    }

    if let Some(handle) = printer_thread {
        handle.join().unwrap();
    }

    if !options.live_print {
        while !receive_hashes_rx.is_empty() {
            if let Ok(hash) = receive_hashes_rx.recv_timeout(Duration::from_millis(100)) {
                println!("{}", hash);
            }
        }
    }
}

fn main() {
    let matches = Command::new("jw")
        .version("2.0")
        .about("A CLI frontend to jwalk for blazingly fast filesystem traversal!")
        .arg(Arg::new("live-print")
            .long("live")
            .short('l')
            .action(ArgAction::SetTrue)
            .help("Display results in realtime, rather than collecting first and displaying later.")
            .long_help("Display results in realtime, rather than collecting first and displaying later.
This will result in a significant drop in performance due to the constant terminal output."))

        .arg(Arg::new("checksum")
            .long("checksum")
            .short('c')
            .num_args(0..=1)
            .value_parser(["xxh3", "sha224", "sha256", "sha384", "sha512", "md5"])
            .value_name("algorithm")
            .help("Output an index containing the hash of every file using the specified algorithm.")
            .long_help(
"Output an index containing the hash of every file using the specified algorithm.
It is highly recommended you stick with xxh3, as it is significantly more performant,
and directly suited for this use case. SHA2/MD5 are only provided for compatibility."))

        .arg(Arg::new("checksum-threads")
            .long("threads")
            .short('t')
            .value_parser(value_parser!(usize))
            .value_name("count")
            .default_value("4")
            .help("The number of threads to use to hash files in parallel."))

        .arg(Arg::new("depth")
            .long("depth")
            .short('d')
            .value_parser(value_parser!(usize))
            .value_name("limit")
            .default_value("0")
            .help("The recursion depth limit. Setting this to 1 effectively disables recursion."))

        .arg(Arg::new("exclude")
            .long("exclude")
            .short('x')
            .value_parser(["files", "dirs", "dot", "other"])
            .value_name("t1,t2")
            .value_delimiter(',')
            .help("Exclude one more types of entries, separated by coma.")
            .num_args(0..=4))

        .arg(Arg::new("directories")
            .default_value(".")
            .num_args(1..)
            .help("The target directories to traverse, can be multiple. Use -- to read directories from stdin."))

        .get_matches();

    let mut supplied_targets: Vec<String> = match matches.get_many::<String>("directories") {
        Some(dirs) => dirs.into_iter().map(|s| s.to_string()).collect(),
        None => panic!("No directories supplied!"),
    };

    if supplied_targets.first().is_some_and(|s| s == "--") {
        supplied_targets = read_stdin();
    }

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

    let options = Options {
        live_print: *matches.get_one::<bool>("live-print").unwrap_or(&false),
        exclude: exclude_flags,
        checksum: matches.contains_id("checksum").then(|| {
            matches
                .get_one::<String>("checksum")
                .map(HashAlgorithm::from)
                .unwrap_or(HashAlgorithm::Xxh3)
        }),
        checksum_threads: *matches.get_one("checksum-threads").unwrap_or(&4),
        depth: *matches.get_one("depth").unwrap_or(&0),
        directories: supplied_targets,
    };

    if let Some(algorithm) = &options.checksum {
        checksum(&options, algorithm);
    } else {
        traverse(options);
    }
}
