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
    depth: usize,
    exclude: usize,
    silent: bool,
    directories: Vec<String>,
    print_stats: bool,
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

        let mut file_count: usize = 0;
        let mut dir_count: usize = 0;
        let mut other_count: usize = 0;

        // The choice to repeat myself by nesting the same for loop under
        // several branches, rather than putting those branches into the
        // for loop is a deliberate one. Applying DRY to everything will
        // result in shittier code in some scenarios. Apply DRY where it
        // makes sense. In this case, it would reduce performance of each
        // iteration at a rate of O(N). For what? A handful of fewer lines?
        if options.live_print {
            if options.print_stats {
                for entry in walker {
                    let path = entry.path();

                    if path.is_file() {
                        file_count += 1;
                    } else if path.is_dir() {
                        dir_count += 1;
                    } else {
                        other_count += 1;
                    }

                    println!("{}", path.display());
                }
            } else {
                for entry in walker {
                    println!("{}", entry.path().display());
                }
            }
        } else {
            let results = walker.collect::<Vec<_>>();

            if options.print_stats {
                if options.silent {
                    for entry in results {
                        let path = entry.path();

                        if path.is_file() {
                            file_count += 1;
                        } else if path.is_dir() {
                            dir_count += 1;
                        } else {
                            other_count += 1;
                        }
                    }
                } else {
                    for entry in results {
                        let path = entry.path();

                        if path.is_file() {
                            file_count += 1;
                        } else if path.is_dir() {
                            dir_count += 1;
                        } else {
                            other_count += 1;
                        }

                        println!("{}", entry.path().display());
                    }
                }
            } else if !options.silent {
                for entry in results {
                    println!("{}", entry.path().display());
                }
            }
        }

        if options.print_stats {
            println!(
                "\nCounted {} files, {} directories, and {} misc entries.",
                file_count, dir_count, other_count,
            );
        }
    }
}

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
            .par_bridge()
            .filter_map(|e| {
                e.ok().and_then(|e| {
                    e.path()
                        .is_file()
                        .then_some(e.path().to_str())
                        .flatten()
                        .map(str::to_string)
                })
            });

        let hashes: Vec<(String, String)> = if options.live_print {
            walker
                .filter_map(|file_path| {
                    hash_file!(algorithm, &file_path)
                        .map(|hash| {
                            println!("{}{}", hash, file_path);
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
    }
}

fn checksum_diff(algorithm: HashAlgorithm, paths: &[String], print_stats: bool) {
    let mut paths = paths.iter();

    let convert = |path: &String| -> Option<PathBuf> {
        Some(PathBuf::from(path))
            .filter(|p| p.is_file())
            .or_else(|| {
                eprintln!("Doesn't exist/not a file: {:?}", path);
                exit(1);
            })
    };

    let base_file: PathBuf = paths.next().and_then(convert).unwrap_or_else(|| {
        eprintln!("Not enough files to perform a diff. Missing the first.");
        exit(1);
    });

    let subsequent_files: Vec<PathBuf> = paths
        .next()
        .or_else(|| {
            eprintln!("Not enough files to perform a diff. Missing the second.");
            exit(1);
        })
        .into_iter()
        .chain(paths)
        .filter_map(convert)
        .collect();

    let digest_length: usize = algorithm.digest_size() * 2;

    let read_hashes = |file: &PathBuf| -> HashMap<String, String> {
        let parse_line = |line: String| -> Option<(String, String)> {
            line.split_at_checked(digest_length)
                .map(|(hash, line)| (line.to_string(), hash.to_string()))
        };

        let line_reader = BufReader::new(File::open(file).unwrap_or_else(|e| {
            eprintln!("Failed to open file: {}", e);
            exit(1);
        }))
        .lines()
        .map_while(Result::ok);

        line_reader.filter_map(parse_line).collect()
    };

    let base_hashes: HashMap<String, String> = read_hashes(&base_file);

    let subsequent_hash_files: Vec<(HashMap<String, String>, PathBuf)> = subsequent_files
        .into_iter()
        .map(|pb| (read_hashes(&pb), pb))
        .collect();

    let mut discrepancies: usize = 0;

    let mut msg_mismatches: Vec<String> = vec![];
    let mut msg_missing: Vec<String> = vec![];
    let mut msg_excess: Vec<String> = vec![];

    for (file_name, base_hash) in &base_hashes {
        for (other_hashes, hash_file) in &subsequent_hash_files {
            if let Some(other_hash) = other_hashes.get(file_name) {
                if *other_hash != *base_hash {
                    msg_mismatches.push(format!(
                        "[!({})] {} != {} == {}",
                        hash_file.display(),
                        other_hash,
                        base_hash,
                        file_name,
                    ));

                    discrepancies += 1;
                }
            } else {
                msg_missing.push(format!("[-({})] {}", hash_file.display(), file_name));
                discrepancies += 1;
            }
        }
    }

    for (other_hashes, hash_file) in &subsequent_hash_files {
        for (file_name, other_hash) in other_hashes {
            if !base_hashes.contains_key(file_name) {
                msg_excess.push(format!(
                    "[+({})] {} {}",
                    hash_file.display(),
                    other_hash,
                    file_name
                ));

                discrepancies += 1;
            }
        }
    }

    msg_mismatches.sort();
    msg_missing.sort();
    msg_excess.sort();

    for msg in msg_mismatches
        .iter()
        .chain(msg_missing.iter())
        .chain(msg_excess.iter())
    {
        println!("{}", msg);
    }

    if print_stats {
        if discrepancies == 0 {
            println!("All entries validated without any discrepancies.");
            exit(0);
        } else {
            println!("\nFound {} total discrepancies!", discrepancies);
            println!(
                "  {} Mismatching Hashes\n  {} Missing Files\n  {} Excess Files",
                msg_mismatches.len(),
                msg_missing.len(),
                msg_excess.len()
            );
            exit(1);
        }
    }
}

fn main() {
    let matches = Command::new("jw")
        .version("2.2.10")
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
            .action(ArgAction::SetTrue)
            .help("Generate an index of file hashes and their associated file names, and print it.")
            .long_help("Generate an index of file hashes and their associated file names, and print it.
The algorithm used by default is Xxh3, which is the recommended choice. Though
if you want to use a different algorithm, use --checksum-with (-C) instead."))

        .arg(Arg::new("checksum-algo")
            .long("checksum-with")
            .short('C')
            .value_parser(["xxh3", "sha224", "sha256", "sha384", "sha512", "md5"])
            .default_value("xxh3")
            .ignore_case(true)
            .value_name("algorithm")
            .default_value("xxh3")
            .help("Performs --checksum but with the specified hashing algorithm.")
            .long_help("Performs --checksum but with the specified hashing algorithm.
If another argument changes the operating mode of the program, e.g. --diff, then
the algorithm specified will only be stored, and no checksum will be performed.
Stick to Xxh3 and just use -c unless you have a reason to use a different one."))

        .arg(Arg::new("hdiff")
            .long("diff")
            .short('D')
            .value_names(["file1", "file2"])
            .num_args(2..)
            .help("Validate hashes from two or more files containing output from `jw --checksum`")
            .long_help("Validate hashes from two or more files containing output from `jw --checksum`
The first file will be treated as the \"correct\" one; any discrepant hashes
in the subseqeunt files will be reported. If entries from the first file are
missing in the subsequent files, or if the subsequent files have entries not 
present in the first file, that will be reported as well.

The hash length must be known for -D to parse the input files and separate
hashes from file paths. A length of 16 is assumed by default as that's how
long Xxh3 hashes are. If you used a different algorithm however, then you
must specify the algorithm before -D, e.g. `jw -C sha256 -D file1 file2`

If you stuck with defaults: `jw -c`, then you can just `jw -D file1 file2`"))

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

        .arg(Arg::new("silent")
            .long("silent")
            .short('S')
            .action(ArgAction::SetTrue)
            .help("Suppress output, useful for benchmarking, or just counting files via --stats"))

        .arg(Arg::new("stats")
            .long("stats")
            .short('s')
            .action(ArgAction::SetTrue)
            .help("Count the number of files, dirs, and other entries, and print at the end.")
            .long_help("Count the number of files, dirs, and other entries, and print at the end.
This will decrease performance. This will cause a significant slowdown
and is primarily here for debugging or benchmarking. A more efficient
method to do this will be implemented in the future.")
            )

        .arg(Arg::new("directories")
            .default_value(".")
            .num_args(1..)
            .help("The target directories to traverse, can be multiple. Use -- to read paths from stdin."))
        .get_matches();

    if let Some(checksum_files) = matches.get_many::<String>("hdiff").map(|fp| {
        fp.into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
    }) {
        checksum_diff(
            HashAlgorithm::from(
                matches
                    .get_one::<String>("checksum-algo")
                    .unwrap_or(&"xxh3".to_string()),
            ),
            &checksum_files,
            *matches.get_one("stats").unwrap_or(&false),
        );
        exit(0);
    }

    let mut walk_dirs: Vec<String> = matches
        .get_many::<String>("directories")
        .map(|dirs| dirs.into_iter().map(|s| s.to_string()).collect())
        .expect("No directories provided!");

    if walk_dirs.first().is_some_and(|s| s == "--") {
        walk_dirs = read_stdin();
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

    let checksum_mode = matches!(
        matches.value_source("checksum"),
        Some(ValueSource::CommandLine)
    ) || matches!(
        matches.value_source("checksum-algo"),
        Some(ValueSource::CommandLine)
    );

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
}
