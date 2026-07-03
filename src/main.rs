use std::{ffi::CString, fs, path::PathBuf};

use nix::{
    mount::{MntFlags, MsFlags, mount, umount2},
    sched::{CloneFlags, clone},
    sys::{signal::Signal, wait::waitpid},
    unistd::{chdir, execvp, pivot_root, sethostname},
};

fn child_function(id: &str) {
    sethostname("bonker").unwrap();

    // mount events shouldnt propagate to the host
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )
    .unwrap();

    let cgroup = PathBuf::from("/sys/fs/cgroup").join(format!("bonker-{id}"));
    fs::create_dir_all(&cgroup).unwrap();

    fs::write(cgroup.join("memory.max"), (100 * 1024 * 1024).to_string()).unwrap();
    fs::write(cgroup.join("pids.max"), "8").unwrap();
    let pid = std::process::id();
    fs::write(cgroup.join("cgroup.procs"), pid.to_string()).unwrap();

    let cwd = std::env::current_dir().unwrap();

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
    fs::create_dir_all(&merged).unwrap();
    fs::create_dir_all(&upper).unwrap();
    fs::create_dir_all(&work).unwrap();

    //create overlayFS system
    mount(
        Some("overlay"),
        &merged,
        Some("overlay"),
        MsFlags::empty(),
        Some(options.as_str()),
    )
    .unwrap();

    // make merged a mount point; done to itself to make pivot root recognize merged as a valid mount point.
    mount(
        Some(&merged),
        &merged,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )
    .unwrap();

    fs::create_dir_all(format!("{}/old_root", merged.to_string_lossy())).unwrap();

    chdir(&merged).unwrap();
    pivot_root(".", "old_root").unwrap();

    chdir("/").unwrap();

    umount2("/old_root", MntFlags::MNT_DETACH).unwrap();

    fs::remove_dir("/old_root").unwrap();

    mount(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    )
    .unwrap();

    let shell = CString::new("/bin/sh").unwrap();

    execvp(shell.as_c_str(), &[shell.as_c_str()]).unwrap();

    return;
}

fn main() {
    let mut stack = vec![0u8; 1024 * 1024];

    let flags = CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWUTS;
    let id = uuid::Uuid::new_v4().to_string();
    let pid = unsafe {
        clone(
            Box::new(|| {
                child_function(&id);
                0
            }),
            &mut stack,
            flags,
            Some(Signal::SIGCHLD as i32),
        )
    }
    .unwrap();

    println!("Parent: child pid = {}", pid);

    waitpid(pid, None).unwrap();
    fs::remove_dir(PathBuf::from("/sys/fs/cgroup").join(format!("bonker-{id}"))).unwrap();

    fs::remove_dir_all(format!("containers/{id}")).unwrap();
}
