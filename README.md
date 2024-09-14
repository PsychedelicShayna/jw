# jw - Jwalk CLI Frontend

Are you frustrated with tools like `find`, `fd`, `erd`, `lsd`, `legdur` and others that seem to excel in some areas but fall short in others? I was too, so I built a solution that prioritizes speed and simplicity above all else. The design philosophy of modern tools have a tendency to stray away from the original Linux philosophy of each command doing a single thing, and doing it very well, instead opting to cram as many features in as possible. 

This isn't necessarily a bad thing, I enjoy those features, but there are many times where I simply want to grep every single path from the root of my drive, and that's when those abstractions start backfiring. All the additional rendering tanks performance, the colorized output sometimes messes up your regex, you pipe it to Neovim and are met with a clusterfuck of ANSI escape codes. Higher level languages that are easier to make pretty CLI/TUIs with being single threaded, the creator never anticipating that someone would feed a terrabyte of data to it, and output immediately starts getting dumped to the terminal creating massive I/O bottlenecks... **enough**

Sometimes you just need to take a page out of the Sesto Elemento's book.

## What is jw exactly?
jw is a command line frontend for [jwalk](https://github.com/byron/jwalk), a blazingly fast filesystem traversal library. While jwalk itself provides unparalleled performance in recursively traversing directories, it lacks a CLI, so I created jw to fill that gap. This utility leverages the power of jwalk to allow you to efficiently sift through directories containing a massive number of files, with a focus on raw performance and minimal abstraction.

It also doubles as a way to hash a very large number of files, thanks to the insanely fast [xxHash](https://github.com/Cyan4973/xxHash) algorithm; jwalk and xxh3 go together like bread and butter.

Rather than fancy colorized outputs, TUIs, gathering statistics, etc, jw sticks to the essentials, providing the raw performance without any of the bloat.

It simply gives you the raw output as fast as possible, for you to pipe to other utilities, such as ripgrep/grep, xargs, and the like, with no additional nonsense.

## Performance

To give you a rough idea of the performance, JWalk was capable of traversing thorugh 492 GB worth of files in **3 seconds**. That's all it takes, three seconds and you can already grep for file paths.

As for Xxh3 combined with JWalk, it was capable of hashing 7.2GB across more than 10,000 files, in **500 milliseconds**. Yes, it's that fast. Stupid fast.

The SHA2 family and MD5 is also supported but that's only there for compatibility.

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
          Output an index containing the hash of every file using the specified algorithm.
          Uses the default algorithm. To specify one use --calgo. 
          Note: specifying --calgo makes this redundant.

  -C, --checksum-with <algorithm>
          Performs --checksum but with the specified hashing algorithm.
          Using xxh3 is the recommended choice. Unless you have a reason to use something else,
          stick with the default. SHA2 and MD5 are provided for compatibility with other tools
          and existing data. If you're only using jw, you stand to gain a large increase in
          performance by using xxh3.

          [default: xxh3]
          [possible values: xxh3, sha224, sha256, sha384, sha512, md5]

  -D, --diff <file1> <file2>...
          Validate hashes from two or more files containing output from `jw --checksum`
          The first file will be treated as the "correct" one; any discrepant hashes
          in the subseqeunt files will be reported. If entries from the first file are
          missing in the subsequent files, or if the subsequent files have entries not
          present in the first file, that will be reported as well.

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
          This will decrease performance. Unnoticeable in most cases, but the more
          files you're traversing, the more it begins to add up.

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
