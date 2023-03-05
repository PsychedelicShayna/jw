use std::process::exit;

use jwalk::{DirEntry, Error as JwError, WalkDir};

fn main() {
    let mut root_dir: Option<&String> = None;
    let args: Vec<String> = std::env::args().collect();

    for (index, arg) in args.iter().enumerate() {
        let next_arg: &String = match args.get(index + 1) {
            Some(next_arg) => next_arg,
            None => continue,
        };

        if arg == "-r" || arg == "--root" {
            root_dir = Some(next_arg)
        }
    }

    let root_dir: &String = match root_dir {
        Some(root_dir) => root_dir,
        None => {
            println!("Please provide a root directory using --root (-r)");
            return;
        }
    };

    let walk = WalkDir::new(root_dir);
    let dir_entries: Vec<Result<DirEntry<((), ())>, JwError>> = walk.into_iter().collect();

    for dir_entry in dir_entries {
        if let Ok(dir_entry) = dir_entry {
            println!("{:?}", dir_entry.path());
        }
    }
}
