//! Coverage for reading directories from stdin.
//!
//! `--` never reaches the program (clap consumes it as the end-of-options
//! marker), so `-` is the sentinel. Only observable from outside, so these
//! drive the real binary.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const JW: &str = env!("CARGO_BIN_EXE_jw");

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

fn jw_with_stdin(cwd: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(JW)
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("couldn't run jw");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .expect("couldn't write stdin");

    child.wait_with_output().expect("couldn't wait for jw")
}

fn lines_of(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"").unwrap();
}

#[test]
fn dash_reads_one_directory_per_line() {
    let scratch = Scratch::new("stdin-basic");
    touch(&scratch.path().join("a/one"));
    touch(&scratch.path().join("b/two"));
    touch(&scratch.path().join("c/three"));

    let out = jw_with_stdin(scratch.path(), &["-x", "dirs", "-"], "a\nb\n");
    assert!(out.status.success());

    let mut got = lines_of(&out);
    got.sort();
    assert_eq!(got, vec!["a/one", "b/two"], "only the piped dirs are walked");
}

#[test]
fn paths_with_spaces_survive() {
    let scratch = Scratch::new("stdin-spaces");
    touch(&scratch.path().join("with space/file"));

    let out = jw_with_stdin(scratch.path(), &["-x", "dirs", "-"], "with space\n");
    assert!(out.status.success());
    assert_eq!(lines_of(&out), vec!["with space/file"]);
}

#[test]
fn dash_mixes_with_arguments_in_place() {
    let scratch = Scratch::new("stdin-mixed");
    touch(&scratch.path().join("a/one"));
    touch(&scratch.path().join("b/two"));
    touch(&scratch.path().join("c/three"));

    let out = jw_with_stdin(scratch.path(), &["-x", "dirs", "a", "-", "c"], "b\n");
    assert!(out.status.success());

    let mut got = lines_of(&out);
    got.sort();
    assert_eq!(got, vec!["a/one", "b/two", "c/three"]);
}

#[test]
fn blank_lines_and_crlf_are_tolerated() {
    let scratch = Scratch::new("stdin-blank");
    touch(&scratch.path().join("a/one"));

    let out = jw_with_stdin(scratch.path(), &["-x", "dirs", "-"], "\r\na\r\n\n");
    assert!(out.status.success());
    assert_eq!(lines_of(&out), vec!["a/one"]);
}
