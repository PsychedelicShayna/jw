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




## Usage

```
A CLI frontend to jwalk for blazingly fast filesystem traversal!

Usage: jw [OPTIONS] [directories]...

Arguments:
  [directories]...
          The target directories to traverse, can be multiple. Use -- to read paths from stdin.

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
          [possible values: xxh3, sha224, sha256, sha384, sha512, md5]

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

  -d, --depth <limit>
          The recursion depth limit. Setting this to 1 effectively disables recursion.

          [default: 0]

  -x, --exclude [<t1,t2>...]
          Exclude one more types of entries, separated by coma.

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
