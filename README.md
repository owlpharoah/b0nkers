<p align="center">
   <img width="220" alt="bonker" src="https://github.com/user-attachments/assets/2c134544-5d4d-424f-9865-0b59809fd7b7" />
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-000000?style=flat&logo=rust&logoColor=white" alt="rust" />
  <img src="https://img.shields.io/badge/linux-FCC624?style=flat&logo=linux&logoColor=black" alt="linux" />
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="license" />
</p>

## Overview

b0nker is a minimal container runtime written in Rust. It implements the core mechanisms containers are built on directly, without a daemon, an image format, or an OCI runtime layer. 
Given a command, it creates a set of Linux namespaces, applies cgroup resource limits, constructs an isolated root filesystem using an overlay mount and pivot_root, and executes the command inside that environment.

## How it works

b0nker relies on four Linux mechanisms, each responsible for a specific type of isolation.

Namespaces, created with `clone` and `unshare`, control what the process can see: other processes, mount points, and hostname.
pivot_root gives the process its own root filesystem safely, avoiding the escape issues associated with chroot. 
cgroups v2 limit how much memory and how many processes the container can use. 
Overlayfs allows the root filesystem to be shared across containers without modifying the underlying base image.

A full breakdown of these mechanisms, including the underlying syscalls, is available in the accompanying [blog post](https://op3kay.dev/writing/b0nker)

## Usage

```bash
sudo -E cargo build --release
sudo -E ./target/release/b0nker run <command> [args...]
```

Example:

```bash
$ sudo -E ./target/release/b0nker run ps
PID   USER     TIME  COMMAND
    1 0         0:00 ps
```

Inside the container, the process sees itself as PID 1, with no visibility into host processes or the host filesystem.

## Benchmarks

100 runs of `b0nker run true`:

```
mean:    37.0 ms
median:  36.3 ms
min:     27.2 ms
max:     57.6 ms
```
## License

Licensed under the MIT License. See [LICENSE](./LICENSE) for details.
