use std::{ffi::CString, fs, os::fd::OwnedFd, path::PathBuf, process::exit};

use nix::{
    mount::{MntFlags, MsFlags, mount, umount2},
    sched::{CloneFlags, clone},
    sys::{
        signal::Signal,
        stat::{Mode, SFlag, mknod},
        wait::{WaitStatus, waitpid},
    },
    unistd::{chdir, execvp, pipe, pivot_root, read, sethostname, write},
};

struct Bonker {
    id: String,
}

impl Bonker {
    pub fn new(id: &str) -> Self {
        mount(
            None::<&str>,
            "/",
            None::<&str>,
            MsFlags::MS_REC | MsFlags::MS_PRIVATE,
            None::<&str>,
        )
        .unwrap();
        Self { id: id.to_string() }
    }

    pub fn set_host_name(&self) -> nix::Result<()> {
        sethostname("bonker")?;
        Ok(())
    }
    pub fn customize_limits(&self, max_memory: u32, max_pids: u32) -> Result<(), std::io::Error> {
        let id = &self.id;
        let cgroup = PathBuf::from("/sys/fs/cgroup").join(format!("oyster-{id}"));
        fs::create_dir_all(&cgroup)?;

        fs::write(cgroup.join("memory.max"), (max_memory).to_string())?;
        fs::write(cgroup.join("pids.max"), max_pids.to_string())?;
        let pid = std::process::id();
        fs::write(cgroup.join("cgroup.procs"), pid.to_string())?;
        Ok(())
    }

    // will also initlize root at merged
    pub fn initlize_fs(&self) -> Result<(), std::io::Error> {
        let id = self.id.clone();
        let cwd = std::env::current_dir()?;

        let lower = cwd.join("lower");
        let upper = cwd.join(format!("containers/{id}/upper"));
        let work = cwd.join(format!("containers/{id}/work"));
        let merged = cwd.join(format!("containers/{id}/merged"));

        let options = format!(
            "lowerdir={},upperdir={},workdir={}",
            lower.display(),
            upper.display(),
            work.display(),
        );

        fs::create_dir_all(&merged)?;
        fs::create_dir_all(&upper)?;
        fs::create_dir_all(&work)?;

        //create overlayFS system
        mount(
            Some("overlay"),
            &merged,
            Some("overlay"),
            MsFlags::empty(),
            Some(options.as_str()),
        )?;
        mount(
            Some(&merged),
            &merged,
            None::<&str>,
            MsFlags::MS_BIND | MsFlags::MS_REC,
            None::<&str>,
        )?;
        fs::create_dir_all(format!("{}/old_root", merged.to_string_lossy()))?;

        chdir(&merged)?;
        pivot_root(".", "old_root")?;

        chdir("/")?;

        umount2("/old_root", MntFlags::MNT_DETACH)?;

        fs::remove_dir("/old_root")?;
        Ok(())
    }

    pub fn mount_proc(&self) -> nix::Result<()> {
        mount(
            Some("proc"),
            "/proc",
            Some("proc"),
            MsFlags::empty(),
            None::<&str>,
        )?;
        Ok(())
    }

    pub fn mount_dev(&self) -> nix::Result<()> {
        mount(
            Some("dev"),
            "/dev",
            Some("tmpfs"),
            MsFlags::empty(),
            None::<&str>,
        )?;

        //null
        let null = nix::sys::stat::makedev(1, 3);

        mknod(
            CString::new("/dev/null").unwrap().as_c_str(),
            SFlag::S_IFCHR,
            Mode::from_bits_truncate(0o666),
            null,
        )?;

        //tty
        let tty = nix::sys::stat::makedev(5, 0);

        mknod(
            CString::new("/dev/tty").unwrap().as_c_str(),
            SFlag::S_IFCHR,
            Mode::from_bits_truncate(0o666),
            tty,
        )?;
        Ok(())
    }
}

fn child_function(command: &[String]) -> Result<(), std::io::Error> {
    if command.is_empty() {
        eprintln!("No command specified");
        std::process::exit(1);
    }
    let cstrings: Vec<CString> = command
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();

    let argv: Vec<&std::ffi::CStr> = cstrings.iter().map(|s| s.as_c_str()).collect();
    unsafe {
        std::env::set_var("PATH", "/bin:/usr/bin");
    }
    match execvp(argv[0], &argv) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("execvp failed: {}", e);
            exit(1);
        }
    };

    Ok(())
}

fn child_main(command: &[String], id: &str, write_fd: &OwnedFd) -> anyhow::Result<()> {
    let oyster = Bonker::new(id);
    oyster.set_host_name()?;
    oyster.customize_limits(10 * 1024 * 1024, 3)?;
    oyster.initlize_fs()?;
    oyster.mount_proc()?;
    oyster.mount_dev()?;

    write(write_fd, &[1])?;

    child_function(command)?;

    Ok(())
}

fn cleanup(id: &str) {
    let _ = fs::remove_dir(PathBuf::from("/sys/fs/cgroup").join(format!("oyster-{id}")));
    let _ = fs::remove_dir_all(format!("containers/{id}"));
}

fn main() {
    // let _total = Instant::now();
    let (read_fd, write_fd) = pipe().unwrap();
    let mut stack = vec![0u8; 1024 * 1024];

    let flags = CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWUTS;
    let id = uuid::Uuid::new_v4().to_string();
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 || args[1] != "run" {
        eprintln!("Usage: bonker run <command> [args...]");
        std::process::exit(1);
    }

    let command = args[2..].to_vec();
    let pid = unsafe {
        clone(
            Box::new(|| match child_main(&command, &id, &write_fd) {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!("{e}");

                    let _ = write(&write_fd, &[0]);

                    1
                }
            }),
            &mut stack,
            flags,
            Some(Signal::SIGCHLD as i32),
        )
    }
    .unwrap();

    // println!("Parent: child pid = {}", pid);

    let mut buf = [0];

    match read(&read_fd, &mut buf) {
        Ok(0) => {
            eprintln!("child exited before becoming ready");
            cleanup(&id);
            exit(1);
        }

        Ok(1) => match buf[0] {
            1 => {}
            0 => {
                eprintln!("child failed during setup");
                cleanup(&id);
                exit(1);
            }
            _ => unreachable!(),
        },
        Ok(_) => unreachable!("read more bytes than buffer size"),
        Err(e) => {
            eprintln!("{e}");
            cleanup(&id);
            exit(1);
        }
    }

    // println!("Container startup: {:?}", total.elapsed());

    drop(read_fd);
    let status = waitpid(pid, None).unwrap();
    let exit_code = match status {
        WaitStatus::Exited(_, code) => code,

        WaitStatus::Signaled(_, signal, _) => {
            eprintln!("process terminated by {:?}", signal);
            128 + signal as i32
        }

        _ => {
            eprintln!("unexpected wait status: {:?}", status);
            1
        }
    };
    cleanup(&id);
    exit(exit_code);
}
