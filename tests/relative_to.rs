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
fn base_alone_indexes_itself_like_a_cd_would() {
    let scratch = Scratch::new("cwd");
    let (one, _) = identical_trees(&scratch);

    // No directories at all: the base is the tree, and the entries are spelled
    // exactly as `cd root-one && jw -c` spells them.
    let via_r = jw_in(scratch.path(), &["-c", "-r", "root-one"]);
    let via_cd = jw_in(&one, &["-c"]);

    assert!(via_r.status.success(), "{}", stderr_of(&via_r));

    let paths_of = |o: &Output| {
        let mut paths: Vec<String> = stdout_of(o)
            .lines()
            .map(|line| line.split_at(32).1.to_string())
            .collect();
        paths.sort();
        paths
    };

    assert_eq!(paths_of(&via_r), ["./sub/nested.txt", "./top.txt"]);
    assert_eq!(paths_of(&via_r), paths_of(&via_cd));
}

#[test]
fn relative_directories_resolve_under_the_base() {
    let scratch = Scratch::new("under");
    let (one, _) = identical_trees(&scratch);

    // `sub` is found beneath the base, and recorded as `sub/...`, which is what
    // `cd root-one && jw -c sub` records.
    let output = jw(&["-c", "-r", one.to_str().unwrap(), "sub"]);

    assert!(output.status.success(), "{}", stderr_of(&output));

    let paths: Vec<String> = stdout_of(&output)
        .lines()
        .map(|line| line.split_at(32).1.to_string())
        .collect();

    assert_eq!(paths, ["sub/nested.txt"]);
}

#[test]
fn an_absolute_directory_outside_the_base_is_rejected() {
    let scratch = Scratch::new("ancestor");
    let (one, two) = identical_trees(&scratch);

    let output = jw(&["-c", "-r", two.to_str().unwrap(), one.to_str().unwrap()]);

    assert!(!output.status.success(), "expected a rejection");
    assert!(stderr_of(&output).contains("is not an ancestor"));
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
fn diff_strips_a_base_per_index() {
    let scratch = Scratch::new("with-diff");
    let (one, two) = identical_trees(&scratch);

    // Indexes written the old way, with absolute entries, still line up when
    // --diff is told what each one's root was; the base may sit after its
    // index file, as it reads most naturally.
    let index_one = write_index(&scratch, "one.hf", &one, None);
    let index_two = write_index(&scratch, "two.hf", &two, None);

    let output = jw(&[
        "-s",
        "-D",
        index_one.to_str().unwrap(),
        "-r",
        one.to_str().unwrap(),
        index_two.to_str().unwrap(),
        "-r",
        two.to_str().unwrap(),
    ]);

    assert!(
        output.status.success(),
        "expected a clean diff, got:\n{}",
        stdout_of(&output)
    );

    // One base for three indexes is neither one-for-all nor one-each.
    let index_three = write_index(&scratch, "three.hf", &one, None);

    let uneven = jw(&[
        "-D",
        index_one.to_str().unwrap(),
        index_two.to_str().unwrap(),
        index_three.to_str().unwrap(),
        "-r",
        one.to_str().unwrap(),
        "-r",
        two.to_str().unwrap(),
    ]);

    assert!(!uneven.status.success(), "expected a rejection");
    assert!(stderr_of(&uneven).contains("one per index"));
}

#[test]
fn cd_made_and_base_made_indexes_agree_under_diff() {
    let scratch = Scratch::new("agree");
    let (one, two) = identical_trees(&scratch);

    let via_cd = jw_in(&one, &["-c"]);
    let via_r = jw(&["-c", "-r", two.to_str().unwrap()]);
    let via_abs = jw(&["-c", one.to_str().unwrap()]);

    let cd_index = scratch.path().join("cd.hf");
    let r_index = scratch.path().join("r.hf");
    let abs_index = scratch.path().join("abs.hf");

    fs::write(&cd_index, &via_cd.stdout).unwrap();
    fs::write(&r_index, &via_r.stdout).unwrap();
    fs::write(&abs_index, &via_abs.stdout).unwrap();

    // ./sub/file, ./sub/file, and /abs/one/sub/file with -r /abs/one all key
    // as sub/file; the trailing empty base is `.`-shaped and strips nothing.
    let output = jw(&[
        "-s",
        "-D",
        cd_index.to_str().unwrap(),
        r_index.to_str().unwrap(),
        abs_index.to_str().unwrap(),
        "-r",
        ".",
        "-r",
        ".",
        "-r",
        one.to_str().unwrap(),
    ]);

    assert!(
        output.status.success(),
        "expected a clean diff, got:\n{}",
        stdout_of(&output)
    );
}

#[test]
fn a_checksum_index_has_one_base() {
    let scratch = Scratch::new("two-bases");
    let (one, two) = identical_trees(&scratch);

    let output = jw(&["-c", "-r", one.to_str().unwrap(), "-r", two.to_str().unwrap()]);

    assert!(!output.status.success(), "expected a rejection");
    assert!(stderr_of(&output).contains("one base"));
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
