use jwalk::WalkDir;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

fn traverse(
    dir_path: &String,
    realtime_print: bool,
    count_stats: bool,
    filter: Filter,
    file_counter: Arc<Mutex<u64>>,
    dir_counter: Arc<Mutex<u64>>,
) -> Vec<String> {
    WalkDir::new(dir_path)
        .into_iter()
        .filter_map(|e| {
            match &e {
                Ok(e) => {
                    let path = e.path();
                    let path_str = path.to_string_lossy().to_string();

                    if let Filter::FilesOnly = filter {
                        if !path.is_file() {
                            return None;
                        }
                    } else if let Filter::DirsOnly = filter {
                        if !path.is_dir() {
                            return None;
                        }
                    }

                    if count_stats {
                        if e.path().is_file() {
                            match file_counter.lock() {
                                Ok(mut n) => *n += 1,
                                Err(e) => eprintln!(
                                    "Failed to increment arc mutex counter for counting files, error: {}",
                                    e
                                ),
                            }
                        } else if e.path().is_dir() {
                            match dir_counter.lock() {
                                Ok(mut n) => *n += 1,
                                Err(e) => eprintln!(
                                    "Failed to increment arc mutex counter for counting directories, error: {}",
                                    e
                                ),
                            }
                        }
                    }

                    if realtime_print {
                        println!("{}", path_str);
                    }

                    Some(path)
                }
                Err(_) => None,
            };

            e.ok().and_then(|e| {
                let path = e.path().to_string_lossy().to_string();

                if realtime_print {
                    println!("{}", path);
                }

                Some(path)
            })
        })
        .collect()
}

fn read_stdin() -> Vec<String> {
    let stdin = std::io::stdin();
    let mut buffer = String::new();

    stdin.read_line(&mut buffer).unwrap();

    buffer
        .split_whitespace()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
}

const HELP_MESSAGE: &'static str = "
Usage: jls [options] [directories]\n
Options:
    -h,  --help        The message you're viewing right now.
    -v,  --verbose     Print each path as it is traversed.
    -r,  --rayon       Enable the use of parallel processing, at the cost of performance.
    -s,  --stats       Show statistics about the traversal at the end.
    -of, --only-files  Only show files in the output.
    -od, --only-dirs   Only show directories in the output.
    -   --             Read directories from stdin.";

#[derive(Clone, Copy)]
enum Filter {
    FilesOnly,
    DirsOnly,
    Everything,
}

fn main() {
    let arguments = std::env::args().skip(1);
    let mut verbose_mode: bool = false;

    let mut directories = Vec::<String>::new();
    let mut stdin_mode: bool = false;

    let mut options_over: bool = false;
    let mut enable_rayon: bool = false;
    let mut enable_stats: bool = false;

    let mut file_filter = false;
    let mut directory_filter = false;

    for arg in arguments {
        match arg.as_str() {
            "--help" | "-h" if !options_over => {
                println!("{}", HELP_MESSAGE);
                std::process::exit(0);
            }

            "--verbose" | "-v" if !options_over => verbose_mode = true,
            "--rayon" | "-r" if !options_over => enable_rayon = true,
            "--stats" | "-s" if !options_over => enable_stats = true,

            "--only-files" | "-of" => file_filter = true,
            "--only-dirs" | "-od" => directory_filter = true,

            directory => {
                if !options_over {
                    options_over = true;
                }

                if directory == "--" {
                    stdin_mode = true;
                    break;
                } else {
                    directories.push(directory.to_string());
                }
            }
        }
    }

    let filter = match (file_filter, directory_filter) {
        (true, false) => Filter::FilesOnly,
        (false, true) => Filter::DirsOnly,
        (false, false) => Filter::Everything,
        (true, true) => {
            eprintln!("Cannot use both --only-files and --only-dirs at the same time. Provide neither to allow both.");
            std::process::exit(1);
        }
    };

    if stdin_mode {
        directories = read_stdin();
    }

    if directories.is_empty() {
        eprintln!("Please provide one or more directories.");
        std::process::exit(1);
    } else if vec!["-h".to_string(), "--help".to_string()].contains(&directories[0]) {
        eprintln!("Please provide one or more directories.");
        eprintln!("Example: jls dir1 dir2 dir3");
        std::process::exit(1);
    }

    let file_counter: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let dir_counter: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

    let results: Vec<_>;

    macro_rules! traversem {
        ($iterator:expr, $verbose:expr, $stats:expr, $filter:expr) => {
            $iterator
                .map(|d| {
                    traverse(
                        d,
                        $verbose,
                        $stats,
                        $filter,
                        file_counter.clone(),
                        dir_counter.clone(),
                    )
                })
                .collect::<Vec<_>>()
                .iter()
                .flat_map(|inner| inner.iter())
                .cloned()
                .collect()
        };
    }

    let start_time = Instant::now();

    if enable_rayon {
        results = traversem!(directories.par_iter(), verbose_mode, enable_stats, filter);
    } else {
        results = traversem!(directories.iter(), verbose_mode, enable_stats, filter);
    }

    if !verbose_mode {
        for result in &results {
            println!("{}", result);
        }
    }

    let elapsed_time = start_time.elapsed();

    if enable_stats {
        let file_counter = file_counter.lock();
        let dir_counter = dir_counter.lock();

        match (file_counter, dir_counter) {
            (Ok(fc), Ok(dc)) => {
                let count_message = match filter {
                    Filter::Everything => {
                        format!("{} files and {} directories", fc, dc)
                    }
                    Filter::FilesOnly => format!("{} files", results.len()),
                    Filter::DirsOnly => format!("{} directories", results.len()),
                };

                println!(
                    "\nFinished traversing {} root directories, collecting {} in {} seconds.",
                    &directories.len(),
                    count_message,
                    elapsed_time.as_secs_f64()
                );
            }

            (_, _) => {
                eprintln!("Failed to acquire mutex lock for counting files and directories.");
                std::process::exit(1);
            }
        }
    }
}
