use jwalk::WalkDir;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

fn prettify_int(num: u64) -> String {
    if num < 1000 {
        return num.to_string();
    }
    let mut chunks_of_three = Vec::<String>::new();
    let mut chunk_buffer = String::new();

    let mut digits = num.to_string();

    while digits.len() % 3 != 0 {
        let digit = digits.remove(0);
        chunk_buffer.push(digit);
    }

    if !chunk_buffer.is_empty() {
        chunks_of_three.push(chunk_buffer.clone());
        chunk_buffer.clear();
    }

    for digit in digits.chars() {
        chunk_buffer.push(digit);

        if chunk_buffer.len() >= 3 {
            chunks_of_three.push(chunk_buffer.clone());
            chunk_buffer.clear();
        }
    }

    chunks_of_three.join(",").to_string()
}

fn traverse(
    dir_path: &String,
    realtime_print: bool,
    count_stats: bool,
    skip_hidden: bool,
    filter: Filter,
    file_counter: Arc<Mutex<u64>>,
    dir_counter: Arc<Mutex<u64>>,
    other_counter: Arc<Mutex<u64>>,
) -> Vec<String> {
    WalkDir::new(dir_path)
        .skip_hidden(skip_hidden)
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
                    } else if let Filter::OtherOnly = filter {
                        if path.is_dir() || path.is_file() {
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
                        } else {
                            match other_counter.lock() {
                                Ok(mut n) => *n += 1,
                                Err(e) => eprintln!(
                                    "Failed to increment arc mutex counter for counting other miscellaneous file types, error: {}",
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
Usage: jw [options] [directories]\n
Options:
    -h,  --help           The message you're viewing right now.
    -R,  --realtime       Print each path as soon as possible in realtime, rather than waiting to collect them all.
    -r,  --rayon          Enable the use of parallel processing, at the cost of performance.
    -s,  --stats          Show statistics about the traversal at the end.
    -sh  --skip-hidden    Don't include .hidden files (included by default).
    -of, --only-files     Only show files in the output.
    -od, --only-dirs      Only show directories in the output.
    -oo, --only-other     Only show entries that aren't files or directories.
    -   --                Read directories from stdin.";

#[derive(Clone, Copy)]
enum Filter {
    FilesOnly,
    DirsOnly,
    OtherOnly,
    Everything,
}

fn main() {
    let arguments = std::env::args().skip(1);
    let mut realtime_output: bool = false;

    let mut directories = Vec::<String>::new();
    let mut stdin_mode: bool = false;

    let mut options_over: bool = false;
    let mut enable_rayon: bool = false;
    let mut enable_stats: bool = false;

    let mut file_filter = false;
    let mut directory_filter = false;
    let mut other_filter = false;

    let mut skip_hidden: bool = false;

    for arg in arguments {
        match arg.as_str() {
            "--help" | "-h" if !options_over => {
                println!("{}", HELP_MESSAGE);
                std::process::exit(0);
            }

            "--verbose" | "-v" if !options_over => realtime_output = true,
            "--rayon" | "-r" if !options_over => enable_rayon = true,
            "--stats" | "-s" if !options_over => enable_stats = true,

            "--only-files" | "-of" => file_filter = true,
            "--only-dirs" | "-od" => directory_filter = true,
            "--only-other" | "-oo" => other_filter = true,
            "--skip-hidden" | "-sh" => skip_hidden = false,

            directory => {
                if !options_over {
                    options_over = true;
                }

                if matches!(directory, "--" | "-") {
                    stdin_mode = true;
                    break;
                } else {
                    directories.push(directory.to_string());
                }
            }
        }
    }

    let filter = match (file_filter, directory_filter, other_filter) {
        (false, false, false) => Filter::Everything,
        (true, false, false) => Filter::FilesOnly,
        (false, true, false) => Filter::DirsOnly,
        (false, false, true) => Filter::OtherOnly,
        (_, _, _) => {
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
    let other_counter: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

    macro_rules! traversem {
        ($iterator:expr, $verbose:expr, $stats:expr, $skip_hidden:expr, $filter:expr) => {
            $iterator
                .map(|d| {
                    traverse(
                        d,
                        $verbose,
                        $stats,
                        $skip_hidden,
                        $filter,
                        file_counter.clone(),
                        dir_counter.clone(),
                        other_counter.clone(),
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

    let results: Vec<String> = if enable_rayon {
        traversem!(
            directories.par_iter(),
            realtime_output,
            enable_stats,
            skip_hidden,
            filter
        )
    } else {
        traversem!(
            directories.iter(),
            realtime_output,
            enable_stats,
            skip_hidden,
            filter
        )
    };

    let elapsed_time = start_time.elapsed();

    if !realtime_output {
        for result in &results {
            println!("{}", result);
        }
    }

    if enable_stats {
        let file_counter = file_counter.lock();
        let dir_counter = dir_counter.lock();
        let other_counter = other_counter.lock();

        match (file_counter, dir_counter, other_counter) {
            (Ok(fc), Ok(dc), Ok(oc)) => {
                let file_count = prettify_int(*fc);
                let dir_count = prettify_int(*dc);
                let other_count = prettify_int(*oc);

                let total_count = prettify_int(results.len() as u64);

                let count_message = match filter {
                    Filter::Everything => {
                        format!(
                            "{} paths total, {} files, {} directories, and {} other",
                            total_count, file_count, dir_count, other_count
                        )
                    }
                    Filter::FilesOnly => format!("{} files", total_count),
                    Filter::DirsOnly => {
                        format!("{} directories", total_count)
                    }
                    Filter::OtherOnly => {
                        format!("{} misc", total_count)
                    }
                };

                println!(
                    "\nFinished traversing {} root directories, collecting {} in {} seconds.",
                    directories.len(),
                    count_message,
                    elapsed_time.as_secs_f64()
                );
            }

            (_, _, _) => {
                eprintln!("Failed to acquire mutex lock for counting files and directories.");
                std::process::exit(1);
            }
        }
    }
}
