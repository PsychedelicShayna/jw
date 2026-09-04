//! Coverage for --relative-to.
//!
//! The entire point of the flag is that two indexes taken from two different
//! roots become diffable, which is only observable from the outside, so these
//! drive the real binary end to end rather than poking at internals.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const JW: &str = env!("CARGO_BIN_EXE_jw");

/// Scratch directory that cleans up after itself, so the tests stay free of
/// dev-dependencies.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let path = std::env::temp_dir().join(format!(
            "jw-test-{}-{}-{}",
            label,
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));

        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("couldn't create scratch directory");

        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn jw(args: &[&str]) -> Output {
    jw_in(Path::new("."), args)
}

fn jw_in(cwd: &Path, args: &[&str]) -> Output {
    Command::new(JW)
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("couldn't run jw")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Two trees holding identical bytes, under two differently named roots at two
/// different depths; this is the situation --relative-to exists to fix.
fn identical_trees(scratch: &Scratch) -> (PathBuf, PathBuf) {
    let one = scratch.path().join("root-one");
    let two = scratch.path().join("elsewhere/root-two");

    for root in [&one, &two] {
        fs::create_dir_all(root.join("sub")).expect("couldn't create tree");
        fs::write(root.join("top.txt"), b"top level contents").expect("couldn't write file");
        fs::write(root.join("sub/nested.txt"), b"nested contents").expect("couldn't write file");
    }

    (one, two)
}

/// Runs `jw -c [-r base] <root>` and stores the index, returning its path.
fn write_index(scratch: &Scratch, name: &str, root: &Path, relative_to: Option<&Path>) -> PathBuf {
    let root = root.to_str().unwrap();

    let output = match relative_to {
        Some(base) => jw(&["-c", "-r", base.to_str().unwrap(), root]),
        None => jw(&["-c", root]),
    };

    assert!(
        output.status.success(),
        "indexing {} failed: {}",
        root,
        stderr_of(&output)
    );

    let path = scratch.path().join(name);
    fs::write(&path, &output.stdout).expect("couldn't write index");

    path
}

/// The recorded paths of an index, sorted; jw's output order is nondeterministic
/// because the walk is parallel.
fn recorded_paths(index: &Path) -> Vec<String> {
    // Xxh3 digests are 16 bytes, so 32 hex characters, and there is no separator
    // between the hash and the path.
    let mut paths: Vec<String> = fs::read_to_string(index)
        .expect("couldn't read index")
        .lines()
        .map(|line| line.split_at(32).1.to_string())
        .collect();

    paths.sort();
    paths
}

/// `jw -s -D`; -s is what makes the diff report a summary and set an exit code.
fn diff(indexes: &[&Path]) -> Output {
    let mut args = vec!["-s", "-D"];
    args.extend(indexes.iter().map(|p| p.to_str().unwrap()));

    jw(&args)
}

#[test]
fn identical_trees_diff_clean_when_relative_to_is_set() {
    let scratch = Scratch::new("clean");
    let (one, two) = identical_trees(&scratch);

    let index_one = write_index(&scratch, "one.hf", &one, Some(&one));
    let index_two = write_index(&scratch, "two.hf", &two, Some(&two));

    assert_eq!(recorded_paths(&index_one), ["sub/nested.txt", "top.txt"]);
    assert_eq!(recorded_paths(&index_two), ["sub/nested.txt", "top.txt"]);

    let diffed = diff(&[&index_one, &index_two]);

    assert!(
        diffed.status.success(),
        "expected a clean diff, got:\n{}",
        stdout_of(&diffed)
    );

    assert!(stdout_of(&diffed).contains("without any discrepancies"));
}

#[test]
fn live_print_records_the_same_paths() {
    let scratch = Scratch::new("live");
    let (one, _) = identical_trees(&scratch);

    // -l takes a separate branch through checksum_rayon, so it has to be pinned
    // independently; the two must not drift apart.
    let output = jw_in(
        scratch.path(),
        &[
            "-c",
            "-l",
            "-r",
            one.to_str().unwrap(),
            one.to_str().unwrap(),
        ],
    );

    assert!(output.status.success(), "{}", stderr_of(&output));

    let mut paths: Vec<String> = stdout_of(&output)
        .lines()
        .map(|line| line.split_at(32).1.to_string())
        .collect();

    paths.sort();

    assert_eq!(paths, ["sub/nested.txt", "top.txt"]);
}

#[test]
fn every_walked_directory_is_checked_against_the_base() {
    let scratch = Scratch::new("multi");
    let (one, two) = identical_trees(&scratch);

    // One base shared by several roots; each keeps whatever sits below the base.
    let output = jw(&[
        "-c",
        "-r",
        scratch.path().to_str().unwrap(),
        one.to_str().unwrap(),
        two.to_str().unwrap(),
    ]);

    assert!(output.status.success(), "{}", stderr_of(&output));

    let mut paths: Vec<String> = stdout_of(&output)
        .lines()
        .map(|line| line.split_at(32).1.to_string())
        .collect();

    paths.sort();

    assert_eq!(
        paths,
        [
            "elsewhere/root-two/sub/nested.txt",
            "elsewhere/root-two/top.txt",
            "root-one/sub/nested.txt",
            "root-one/top.txt",
        ]
    );

    // A single root outside the base sinks the whole run, not just its own entries.
    let outside = jw(&[
        "-c",
        "-r",
        one.to_str().unwrap(),
        one.to_str().unwrap(),
        two.to_str().unwrap(),
    ]);

    assert!(!outside.status.success(), "expected a rejection");
    assert!(stderr_of(&outside).contains("is not an ancestor"));
}

#[test]
fn identical_trees_diff_dirty_without_relative_to() {
    let scratch = Scratch::new("dirty");
    let (one, two) = identical_trees(&scratch);

    let index_one = write_index(&scratch, "one.hf", &one, None);
    let index_two = write_index(&scratch, "two.hf", &two, None);

    let diffed = diff(&[&index_one, &index_two]);
    let stdout = stdout_of(&diffed);

    assert!(!diffed.status.success(), "expected a dirty diff");

    // Same bytes, but every entry reads as missing on one side and excess on the
    // other purely because of the root prefix.
    assert!(stdout.contains("2 Missing Files"), "{}", stdout);
    assert!(stdout.contains("2 Excess Files"), "{}", stdout);
    assert!(stdout.contains("0 Mismatching Hashes"), "{}", stdout);
}

#[test]
fn content_changes_are_still_detected_when_relative_to_is_set() {
    let scratch = Scratch::new("mismatch");
    let (one, two) = identical_trees(&scratch);

    fs::write(two.join("top.txt"), b"different contents").expect("couldn't write file");

    let index_one = write_index(&scratch, "one.hf", &one, Some(&one));
    let index_two = write_index(&scratch, "two.hf", &two, Some(&two));

    let diffed = diff(&[&index_one, &index_two]);
    let stdout = stdout_of(&diffed);

    assert!(!diffed.status.success(), "expected a dirty diff");
    assert!(stdout.contains("1 Mismatching Hashes"), "{}", stdout);
    assert!(stdout.contains("top.txt"), "{}", stdout);
}

#[test]
fn only_the_given_base_is_stripped() {
    let scratch = Scratch::new("partial");
    let (one, _) = identical_trees(&scratch);

    // Base sits above the scan root, so the root's own name is kept.
    let index = write_index(&scratch, "one.hf", &one, Some(scratch.path()));

    assert_eq!(
        recorded_paths(&index),
        ["root-one/sub/nested.txt", "root-one/top.txt"]
    );
}

#[test]
fn relative_bases_are_accepted() {
    let scratch = Scratch::new("relative");
    let (_, _) = identical_trees(&scratch);

    // Nothing is canonicalized, so a relative base matches a relative root.
    let output = jw_in(scratch.path(), &["-c", "-r", "root-one", "root-one"]);

    assert!(output.status.success(), "{}", stderr_of(&output));

    let mut paths: Vec<String> = stdout_of(&output)
        .lines()
        .map(|line| line.split_at(32).1.to_string())
        .collect();

    paths.sort();

    assert_eq!(paths, ["sub/nested.txt", "top.txt"]);
}

#[test]
fn a_base_that_is_not_an_ancestor_is_rejected() {
    let scratch = Scratch::new("ancestor");
    let (one, two) = identical_trees(&scratch);

    let output = jw(&["-c", "-r", two.to_str().unwrap(), one.to_str().unwrap()]);

    assert!(!output.status.success(), "expected a rejection");
    assert!(stderr_of(&output).contains("is not an ancestor"));

    // Absolute base against a relative root is the same mistake, lexically.
    let mixed = jw_in(
        scratch.path(),
        &["-c", "-r", one.to_str().unwrap(), "root-one"],
    );

    assert!(!mixed.status.success(), "expected a rejection");
    assert!(stderr_of(&mixed).contains("is not an ancestor"));
}

#[test]
fn relative_to_requires_checksum_mode() {
    let scratch = Scratch::new("mode");
    let (one, _) = identical_trees(&scratch);

    let output = jw(&["-r", one.to_str().unwrap(), one.to_str().unwrap()]);

    assert!(!output.status.success(), "expected a rejection");
    assert!(stderr_of(&output).contains("requires --checksum"));
}

#[test]
fn relative_to_is_rejected_alongside_diff() {
    let scratch = Scratch::new("with-diff");
    let (one, two) = identical_trees(&scratch);

    let index_one = write_index(&scratch, "one.hf", &one, None);
    let index_two = write_index(&scratch, "two.hf", &two, None);

    let output = jw(&[
        "-r",
        scratch.path().to_str().unwrap(),
        "-D",
        index_one.to_str().unwrap(),
        index_two.to_str().unwrap(),
    ]);

    assert!(!output.status.success(), "expected a rejection");
    assert!(stderr_of(&output).contains("no effect on --diff"));
}

#[test]
fn indexes_written_without_relative_to_are_unaffected() {
    let scratch = Scratch::new("compat");
    let (one, _) = identical_trees(&scratch);

    // The format is untouched, so an index written the old way still holds
    // absolute paths and still diffs cleanly against itself.
    let index = write_index(&scratch, "one.hf", &one, None);
    let copy = write_index(&scratch, "one-again.hf", &one, None);

    assert_eq!(
        recorded_paths(&index),
        [
            one.join("sub/nested.txt").to_str().unwrap(),
            one.join("top.txt").to_str().unwrap()
        ]
    );

    let diffed = diff(&[&index, &copy]);

    assert!(
        diffed.status.success(),
        "expected a clean diff, got:\n{}",
        stdout_of(&diffed)
    );
}
