# jw - Jwalk CLI Frontend

Are you frustrated with tools like `find`, `fd`, `erd`, `lsd`, `legdur` and others that seem to excel in some areas but fall short in others? I was too, so I built a solution that prioritizes speed and simplicity above all else. The design philosophy of modern tools have a tendency to stray away from the original Linux philosophy of each command doing a single thing, and doing it very well, instead opting to cram as many features in as possible. 

This isn't necessarily a bad thing, I enjoy those features, but there are many times where I simply want to grep every single path from the root of my drive, and that's when those abstractions start backfiring. All the additional rendering tanks performance, the colorized output sometimes messes up your regex, you pipe it to Neovim and are met with a clusterfuck of ANSI escape codes. Higher level languages that are easier to make pretty CLI/TUIs with being single threaded, the creator never anticipating that someone would feed a terrabyte of data to it, and output immediately starts getting dumped to the terminal creating massive I/O bottlenecks... **enough**

Sometimes you just need to take a page out of the Sesto Elemento's book.

## What is jw exactly?
jw is a command line frontend for [jwalk](https://github.com/byron/jwalk), a blazingly fast filesystem traversal library. While jwalk itself provides unparalleled performance in recursively traversing directories, it lacks a CLI, so I created jw to fill that gap. This utility leverages the power of jwalk to allow you to efficiently sift through directories containing a massive number of files, with a focus on raw performance and minimal abstraction.

It also doubles as a way to hash a very large number of files, thanks to the insanely fast [xxHash](https://github.com/Cyan4973/xxHash) algorithm; jwalk and xxh3 go together like bread and butter.

Rather than fancy colorized outputs, TUIs, gathering statistics, etc, jw sticks to the essentials, providing the raw performance without any of the bloat.

It simply gives you the raw output as fast as possible, for you to pipe to other utilities, such as ripgrep/grep, xargs, fzf, and the like, with no additional nonsense.


https://github.com/user-attachments/assets/9f4a3cf5-4dfa-4a57-845b-a26ded3f660a



https://github.com/user-attachments/assets/f27bda63-a97f-441f-be86-2514fdc64d37


## Performance

To give you a rough idea of the performance, JWalk was capable of traversing thorugh 492 GB worth of files in **3 seconds**. That's all it takes, three seconds and you can already grep for file paths.

As for Xxh3 combined with JWalk, it was capable of hashing 7.2GB across more than 10,000 files, in **500 milliseconds**. Yes, it's that fast. Stupid fast.

The SHA2 family and MD5 is also supported but that's only there for compatibility.

### A Personal Request
Making Rust go fast is a different beast than making C++ go fast. A lot of the techniques that came to mind when trying to squeeze even more performance out of this utility simply don't apply to Rust without breaking the spirit of the language. I'm not a Rust wizard, there's a lot I still don't know. However, I know for a fact that `jw` could run even faster. This [article proving that an optimization "impossible" in Rust, is possible in Rust](https://tunglevo.com/note/an-optimization-thats-impossible-in-rust/) is a prime example of how Rust has its own flavor of black magic I've yet to grasp. I welcome any and all PRs, it's a much appreciated learning experience. By all means, if you spot a way to make it faster, don't hesitate to make a PR, I'd love to learn, even if it's just shaving off a few milliseconds.

The main aspiration I have for `jw` is **speed** above all else, both traversal and hashing, but especially hashing.


https://github.com/user-attachments/assets/2db684a0-a6f6-4416-a2fc-4b65c0da5963



https://github.com/user-attachments/assets/1ecdfc70-8233-4fdb-b75d-00d3c7ca22a5



https://github.com/user-attachments/assets/9d959641-2fcd-41bc-b397-2d7098d59174




## Diffing two copies of the same tree

An index entry is identified by its path, and `jw` records paths exactly the way you typed them. That's fine when you're re-checking the same directory later, but it falls apart the moment you want to compare two copies of the same data living under different roots: `/mnt/backup1/photos/cat.jpg` and `/mnt/backup2/photos/cat.jpg` are the same bytes, but `--diff` has no way to tell, so every single entry gets reported as missing on one side and excess on the other.

The old answer was to `cd` into each tree and index `.`, so both came out as `./photos/cat.jpg`. `--relative-to` (`-r`) is that, without the `cd`: it makes `jw` behave as if the given path were the current directory.

```sh
jw -c -r /mnt/backup1 > backup1.hf     # same bytes as: cd /mnt/backup1 && jw -c
jw -c -r /mnt/backup2 > backup2.hf
jw -s -D backup1.hf backup2.hf
```

Directories default to `.` and relative ones resolve under the base, so `jw -c -r /mnt/backup1 photos` records `photos/cat.jpg`, as it would from a shell sitting there. An absolute directory is walked as typed, has to be under the base, and is recorded with the base removed.

`--diff` takes `-r` as well, for indexes you already wrote with absolute paths. The base is stripped from that index's entries on the way in, so nothing has to be re-indexed. Give it once for every index, or once per index, in order, and it can sit right after the index it belongs to:

```sh
jw -s -D backup1.hf -r /mnt/backup1 backup2.hf -r /mnt/backup2
```

A leading `./` is ignored when comparing, so an index taken by `cd`'ing in, one taken with `-r`, and an absolute one paired with `-r` under `--diff` all agree with each other.

Everything is lexical. Nothing is canonicalized, because jwalk records paths spelled exactly the way the root was typed, symlinks and all, so the base has to be spelled the way the directories are.

## Usage

```
A CLI frontend to jwalk for blazingly fast filesystem traversal!

Usage: jw [OPTIONS] [directories]...

Arguments:
  [directories]...
          The target directories to traverse, can be multiple. Use - to read paths from stdin, one per line.
          
          [default: .]

Options:
  -l, --live
          Display results in realtime, rather than collecting first and displaying later.
          This will result in a significant drop in performance due to the constant terminal output.

  -c, --checksum
          Generate an index of file hashes and their associated file names, and print it.
          The algorithm used by default is Xxh3, which is the recommended choice. Though
          if you want to use a different algorithm, use --checksum-with (-C) instead.

  -C, --checksum-with <algorithm>
          Performs --checksum but with the specified hashing algorithm.
          If another argument changes the operating mode of the program, e.g. --diff, then
          the algorithm specified will only be stored, and no checksum will be performed.
          Stick to Xxh3 and just use -c unless you have a reason to use a different one.
          
          [default: xxh3]
          [possible values: xxh3, blake3, sha224, sha256, sha384, sha512, md5]

  -D, --diff <file1> <file2>...
          Validate hashes from two or more files containing output from `jw --checksum`
          The first file will be treated as the "correct" one; any discrepant hashes
          in the subseqeunt files will be reported. If entries from the first file are
          missing in the subsequent files, or if the subsequent files have entries not 
          present in the first file, that will be reported as well.
          
          The hash length must be known for -D to parse the input files and separate
          hashes from file paths. A length of 16 is assumed by default as that's how
          long Xxh3 hashes are. If you used a different algorithm however, then you
          must specify the algorithm before -D, e.g. `jw -C sha256 -D file1 file2`
          
          If you stuck with defaults: `jw -c`, then you can just `jw -D file1 file2`
          
          Index files may also follow other options, so `-D a.hf -r /x b.hf -r /y` reads
          the same as `-D a.hf b.hf -r /x -r /y`. See --relative-to for what that does.

  -r, --relative-to <path>
          Behave as if this were the current directory, without cd'ing there.
          An index entry is identified by its path, so two trees indexed from different
          places never line up under --diff; every entry looks new on both sides even when
          the bytes are identical. The fix used to be cd'ing into each root and indexing `.`
          so both recorded `./sub/file`. --relative-to does that without the cd:
          
            jw -c -r /mnt/backup1 > a.hf        # records ./sub/file, as `cd /mnt/backup1; jw -c` would
            jw -c -r /mnt/backup2 > b.hf
            jw -s -D a.hf b.hf
          
          With --checksum, directories default to `.` and relative ones resolve under the
          base, exactly as they would from a shell sitting there. An absolute directory is
          walked as typed and must sit under the base; it is recorded with the base removed.
          
          With --diff, the base is instead removed from the entries of an index that was
          written with absolute paths, so old indexes diff without being rewritten. Give it
          once to apply to every index, or once per index in order:
          
            jw -c /mnt/backup1 > a.hf           # entries are /mnt/backup1/sub/file
            jw -c /mnt/backup2 > b.hf
            jw -s -D a.hf b.hf -r /mnt/backup1 -r /mnt/backup2
          
          A leading `./` is ignored when comparing, so an index taken by cd'ing in, one
          taken with -r, and an absolute one paired with -r under --diff all agree.
          
          Everything is lexical. Nothing is canonicalized, because jwalk records paths
          spelled exactly the way the root was typed, symlinks and all (walking `link`
          records `link/f`, not `real/f`), and resolving the base would leave it unable to
          match those. Spell the base the way you spell the directories.

  -d, --depth <limit>
          The recursion depth limit. Setting this to 1 effectively disables recursion.
          
          [default: 0]

  -x, --exclude <t1,t2>
          Exclude one or more types of entries, separated by comma.
          
          [possible values: files, dirs, dot, other]

  -S, --silent
          Suppress output, useful for benchmarking, or just counting files via --stats

  -s, --stats
          Count the number of files, dirs, and other entries, and print at the end.
          This will decrease performance. This will cause a significant slowdown
          and is primarily here for debugging or benchmarking. A more efficient
          method to do this will be implemented in the future.

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
