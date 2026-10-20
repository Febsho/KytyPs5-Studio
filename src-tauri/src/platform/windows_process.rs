use std::{
    collections::{HashMap, HashSet},
    path::Path,
    ptr::null,
    sync::{Mutex, OnceLock},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES, FILETIME, HANDLE,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, Thread32First, Thread32Next,
            PROCESSENTRY32W, TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD, THREADENTRY32,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
            JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{
            GetProcessTimes, OpenProcess, OpenThread, QueryFullProcessImageNameW, ResumeThread,
            WaitForSingleObject, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SET_QUOTA, PROCESS_TERMINATE, PROCESS_VM_READ, THREAD_SUSPEND_RESUME,
        },
    },
    UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE},
};

const PROCESS_JOB_WAIT_MS: u32 = 3_000;
const MAX_RECOVERED_JOB_PROCESSES: usize = 4_096;
const PROCESS_SYNCHRONIZE: u32 = 0x0010_0000;

struct Handle(HANDLE);

unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn jobs() -> &'static Mutex<HashMap<u32, Handle>> {
    static JOBS: OnceLock<Mutex<HashMap<u32, Handle>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
pub(super) fn take_job_for_test(pid: u32) -> Option<impl Drop + Send> {
    jobs().lock().ok()?.remove(&pid)
}

fn win_error(operation: &str) -> String {
    format!("{operation}: {}", std::io::Error::last_os_error())
}

fn process_enumeration_end(operation: &str, error: u32) -> Result<(), String> {
    if error == ERROR_NO_MORE_FILES {
        Ok(())
    } else {
        Err(format!(
            "{operation}: {}",
            std::io::Error::from_raw_os_error(error as i32)
        ))
    }
}

fn create_job() -> Result<Handle, String> {
    let job = unsafe { CreateJobObjectW(null(), null()) };
    if job.is_null() {
        return Err(win_error("Could not create a Windows process job"));
    }
    let handle = Handle(job);
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            handle.0,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if configured == 0 {
        return Err(win_error("Could not configure process-tree cleanup"));
    }
    Ok(handle)
}

fn open_process(pid: u32, rights: u32) -> Result<Handle, String> {
    // `process_is_running` waits on the process handle, which requires
    // SYNCHRONIZE in addition to the query rights used by its callers.
    let handle = unsafe { OpenProcess(rights | PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        Err(win_error("Could not open the registered process"))
    } else {
        Ok(Handle(handle))
    }
}

fn open_process_if_alive(pid: u32, rights: u32) -> Result<Option<Handle>, String> {
    let handle = unsafe { OpenProcess(rights, 0, pid) };
    if !handle.is_null() {
        return Ok(Some(Handle(handle)));
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
        Ok(None)
    } else {
        Err(format!(
            "Could not inspect recovered process descendant {pid}: {error}"
        ))
    }
}

fn assign(job: &Handle, process: &Handle) -> Result<(), String> {
    if unsafe { AssignProcessToJobObject(job.0, process.0) } == 0 {
        Err(win_error("Could not assign the process to its cleanup job"))
    } else {
        Ok(())
    }
}

fn resume_primary_thread(pid: u32) -> Result<(), String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot.is_null() || snapshot as isize == -1 {
        return Err(win_error("Could not locate the suspended process thread"));
    }
    let snapshot = Handle(snapshot);
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut found = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
    while found {
        if entry.th32OwnerProcessID == pid {
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread.is_null() {
                return Err(win_error("Could not open the suspended process thread"));
            }
            let thread = Handle(thread);
            if unsafe { ResumeThread(thread.0) } == u32::MAX {
                return Err(win_error("Could not resume the managed process"));
            }
            return Ok(());
        }
        found = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
    }
    Err("The suspended process has no primary thread".into())
}

struct WindowCloseTarget {
    job: HANDLE,
}

fn process_belongs_to_job(process: &Handle, job: HANDLE) -> bool {
    let mut belongs_to_job = 0;
    unsafe { IsProcessInJob(process.0, job, &mut belongs_to_job) != 0 && belongs_to_job != 0 }
}

unsafe extern "system" fn close_process_window(
    hwnd: windows_sys::Win32::Foundation::HWND,
    data: isize,
) -> i32 {
    let target = &*(data as *const WindowCloseTarget);
    let mut pid = 0_u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid != 0 {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !process.is_null() {
            let process = Handle(process);
            if process_belongs_to_job(&process, target.job) {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    }
    1
}

fn request_graceful_close(job: &Handle) {
    let target = WindowCloseTarget { job: job.0 };
    unsafe {
        EnumWindows(
            Some(close_process_window),
            (&target as *const WindowCloseTarget) as isize,
        )
    };
}

fn stop_job(job: &Handle, label: &str) -> Result<(), String> {
    if unsafe { WaitForSingleObject(job.0, 0) } == WAIT_OBJECT_0 {
        return Ok(());
    }
    request_graceful_close(job);
    match unsafe { WaitForSingleObject(job.0, PROCESS_JOB_WAIT_MS) } {
        WAIT_OBJECT_0 => return Ok(()),
        WAIT_TIMEOUT => {}
        _ => return Err(win_error(&format!("Could not wait for {label}"))),
    }
    if unsafe { TerminateJobObject(job.0, 1) } == 0 {
        return Err(win_error(&format!("Could not force-stop {label}")));
    }
    match unsafe { WaitForSingleObject(job.0, PROCESS_JOB_WAIT_MS) } {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err(format!("{label} did not exit after forced termination")),
        _ => Err(win_error(&format!(
            "Could not confirm termination of {label}"
        ))),
    }
}

/// Attach an owned child to a kill-on-close job before returning control to
/// the caller. If assignment fails, terminate this just-created child so it
/// cannot escape the cancellation and timeout lifecycle.
pub fn attach_child(pid: u32) -> Result<(), String> {
    let job = create_job()?;
    let process = open_process(
        pid,
        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE,
    )?;
    if let Err(error) = assign(&job, &process) {
        unsafe { windows_sys::Win32::System::Threading::TerminateProcess(process.0, 1) };
        return Err(error);
    }
    if let Err(error) = resume_primary_thread(pid) {
        unsafe { TerminateJobObject(job.0, 1) };
        return Err(error);
    }
    let previous = jobs()
        .lock()
        .map_err(|_| "Windows process job registry is unavailable".to_string())?
        .insert(pid, job);
    drop(previous);
    Ok(())
}

/// Stop an application-owned process tree. The job handle is keyed by the
/// PID returned by the child we created, so PID reuse cannot redirect this
/// operation to an unrelated process.
pub fn terminate_child(pid: u32) -> Result<(), String> {
    let registry = jobs();
    let registered = registry
        .lock()
        .map_err(|_| "Windows process job registry is unavailable".to_string())?;
    let Some(job) = registered.get(&pid) else {
        return Err("No owned Windows process job is registered for this process".into());
    };
    match stop_job(job, "Windows process job") {
        Ok(()) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Release a completed child's job. Closing the final job handle also kills
/// any descendants that survived the root process.
pub fn release_child(pid: u32) {
    if let Ok(mut registered) = jobs().lock() {
        registered.remove(&pid);
    }
}

fn filetime_value(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

fn process_creation_time(process: &Handle) -> Option<u64> {
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    (unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) } != 0)
        .then(|| filetime_value(created))
}

fn process_tree_entries() -> Result<Vec<(u32, u32)>, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() || snapshot as isize == -1 {
        return Err(win_error("Could not inspect recovered process descendants"));
    }
    let snapshot = Handle(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return Err(win_error(
            "Could not enumerate recovered process descendants",
        ));
    }
    let mut processes = Vec::new();
    loop {
        processes.push((entry.th32ProcessID, entry.th32ParentProcessID));
        if unsafe { Process32NextW(snapshot.0, &mut entry) } != 0 {
            continue;
        }
        process_enumeration_end(
            "Could not enumerate recovered process descendants",
            unsafe { GetLastError() },
        )?;
        break;
    }
    Ok(processes)
}

fn process_is_running(process: &Handle) -> Result<bool, String> {
    match unsafe { WaitForSingleObject(process.0, 0) } {
        WAIT_OBJECT_0 => Ok(false),
        WAIT_TIMEOUT => Ok(true),
        _ => Err(win_error("Could not confirm the Windows process state")),
    }
}

/// A process assigned to a job does not bring its already-running children
/// with it. Adopt live descendants by their pinned parent process handles so
/// a recovered session can still be stopped as a tree. New children inherit
/// the job as soon as each parent is assigned.
fn attach_existing_descendants(
    root_pid: u32,
    root_creation_time: u64,
    root: &Handle,
    job: &Handle,
) -> Result<(), String> {
    let mut creation_times = HashMap::from([(root_pid, root_creation_time)]);
    let mut owned = HashMap::<u32, Handle>::new();
    let mut retired = std::collections::HashSet::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);

    loop {
        let mut exited = Vec::new();
        for (pid, process) in &owned {
            if !process_is_running(process)? {
                exited.push(*pid);
            }
        }
        for pid in exited {
            owned.remove(&pid);
            creation_times.remove(&pid);
            retired.insert(pid);
        }

        let processes = process_tree_entries()?;
        let mut candidates = Vec::new();
        for (pid, parent_pid) in processes {
            if pid == root_pid || creation_times.contains_key(&pid) || retired.contains(&pid) {
                continue;
            }
            let Some(parent_creation_time) = creation_times.get(&parent_pid).copied() else {
                if retired.contains(&parent_pid) {
                    return Err("A recovered process descendant has an exited parent, so its identity cannot be verified safely".into());
                }
                continue;
            };
            let parent = if parent_pid == root_pid {
                root
            } else if let Some(parent) = owned.get(&parent_pid) {
                parent
            } else {
                continue;
            };
            if !process_is_running(parent)? {
                return Err("A recovered process descendant's parent exited before Studio could verify the full process tree".into());
            }

            let Some(child) = open_process_if_alive(
                pid,
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE,
            )?
            else {
                continue;
            };
            let Some(creation_time) = process_creation_time(&child) else {
                if process_is_running(&child)? {
                    return Err(format!(
                        "Could not verify the creation time of live recovered process descendant {pid}"
                    ));
                }
                continue;
            };
            if creation_time < parent_creation_time || !process_is_running(&child)? {
                continue;
            }
            if !process_is_running(parent)? {
                return Err("A recovered process descendant's parent exited before Studio could assign the full process tree".into());
            }
            candidates.push((pid, parent_pid, parent_creation_time, creation_time, child));
        }

        if candidates.is_empty() {
            return Ok(());
        }
        let current_relationships = process_tree_entries()?.into_iter().collect::<HashSet<_>>();
        let mut added = 0;
        for (pid, parent_pid, parent_creation_time, creation_time, child) in candidates {
            if !current_relationships.contains(&(pid, parent_pid)) || !process_is_running(&child)? {
                continue;
            }
            let parent = if parent_pid == root_pid {
                root
            } else if let Some(parent) = owned.get(&parent_pid) {
                parent
            } else {
                continue;
            };
            if creation_times.get(&parent_pid) != Some(&parent_creation_time)
                || !process_is_running(parent)?
            {
                return Err("A recovered process descendant's parent exited before Studio could assign the full process tree".into());
            }
            if !process_belongs_to_job(&child, job.0) {
                if let Err(error) = assign(job, &child) {
                    if process_is_running(&child)? {
                        return Err(error);
                    }
                    continue;
                }
            }
            creation_times.insert(pid, creation_time);
            owned.insert(pid, child);
            added += 1;
            if creation_times.len() > MAX_RECOVERED_JOB_PROCESSES {
                return Err(
                    "Recovered process tree exceeds the supported 4096-process safety limit".into(),
                );
            }
        }
        if added == 0 {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err("Recovered process tree kept changing while Studio was assigning it to the cleanup job".into());
        }
    }
}

fn process_image(process: &Handle) -> Option<String> {
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process.0, 0, buffer.as_mut_ptr(), &mut length) };
    (ok != 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

fn same_path(left: &Path, right: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Globalization::CompareStringOrdinal;

    let left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    let left = left.as_os_str().encode_wide().collect::<Vec<_>>();
    let right = right.as_os_str().encode_wide().collect::<Vec<_>>();
    let (Ok(left_len), Ok(right_len)) = (i32::try_from(left.len()), i32::try_from(right.len()))
    else {
        return false;
    };
    // CompareStringOrdinal implements the case-insensitive comparison Windows
    // uses for path-like identifiers without lossy UTF-16 conversion.
    unsafe { CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) == 2 }
}

/// Verify process incarnation and executable using one pinned process handle,
/// then assign that exact process to a temporary job and terminate the job.
/// This path is used to stop a session recovered after Studio restarted.
pub fn terminate_verified(
    pid: u32,
    expected_creation_time: i64,
    expected_executable: &Path,
) -> Result<(), String> {
    let process = open_process(
        pid,
        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE,
    )?;
    let creation_time = process_creation_time(&process);
    if creation_time != u64::try_from(expected_creation_time).ok()
        || !process_image(&process)
            .as_deref()
            .is_some_and(|image| same_path(Path::new(image), expected_executable))
    {
        return Err("The process identity no longer matches this recorded emulator session".into());
    }
    let job = create_job()?;
    assign(&job, &process)?;
    attach_existing_descendants(
        pid,
        creation_time.ok_or("Could not verify the recovered process creation time")?,
        &process,
        &job,
    )?;
    stop_job(&job, "verified emulator process job")
}

pub struct Snapshot {
    pub creation_time: u64,
    pub cpu_time_100ns: u64,
    pub resident_bytes: u64,
    pub running: bool,
}

pub fn snapshot(pid: u32) -> Option<Snapshot> {
    // K32GetProcessMemoryInfo requires PROCESS_QUERY_INFORMATION and
    // PROCESS_VM_READ; limited-information rights alone only cover the
    // creation time and executable identity queries.
    let process = open_process(
        pid,
        PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
    )
    .ok()?;
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    if unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) } == 0
    {
        return None;
    }
    let mut memory = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    if unsafe {
        K32GetProcessMemoryInfo(
            process.0,
            &mut memory,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    } == 0
    {
        return None;
    }
    Some(Snapshot {
        creation_time: filetime_value(created),
        cpu_time_100ns: filetime_value(kernel).saturating_add(filetime_value(user)),
        resident_bytes: memory.WorkingSetSize as u64,
        running: process_is_running(&process).ok()?,
    })
}

pub fn matches_executable(pid: u32, expected_executable: &Path) -> bool {
    open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)
        .ok()
        .and_then(|process| process_image(&process))
        .is_some_and(|image| same_path(Path::new(&image), expected_executable))
}

pub fn cpu_clock_ticks_per_second() -> f64 {
    10_000_000.0
}

pub fn process_name_exists(expected_name: &str) -> Result<bool, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot.is_null() || snapshot as isize == -1 {
        return Err(win_error("Could not inspect running Windows processes"));
    }
    let handle = Handle(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let first = unsafe { Process32FirstW(handle.0, &mut entry) } != 0;
    if !first {
        return Err(win_error("Could not enumerate running Windows processes"));
    }
    loop {
        let length = entry
            .szExeFile
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..length]).eq_ignore_ascii_case(expected_name)
        {
            return Ok(true);
        }
        if unsafe { Process32NextW(handle.0, &mut entry) } != 0 {
            continue;
        }
        process_enumeration_end("Could not enumerate running Windows processes", unsafe {
            GetLastError()
        })?;
        break;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    #[test]
    fn process_enumeration_finds_the_current_process_case_insensitively() {
        let executable = std::env::current_exe().expect("test executable path is available");
        let name = executable
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .expect("test executable name is Unicode");
        assert!(super::process_name_exists(name).expect("Windows process snapshot is readable"));
        assert!(super::process_name_exists(&name.to_ascii_uppercase())
            .expect("Windows process snapshot is readable"));
        assert!(
            !super::process_name_exists("kytyps5-studio-missing-test.exe")
                .expect("Windows process snapshot is readable")
        );
    }

    #[test]
    fn process_enumeration_only_accepts_the_normal_end_marker() {
        assert!(super::process_enumeration_end(
            "process snapshot",
            windows_sys::Win32::Foundation::ERROR_NO_MORE_FILES,
        )
        .is_ok());
        assert!(super::process_enumeration_end("process snapshot", 5).is_err());
    }

    #[test]
    fn path_identity_comparison_preserves_unicode_and_ignores_case() {
        assert!(super::same_path(
            std::path::Path::new(r"C:\Games\保存したゲーム\Emulator.EXE"),
            std::path::Path::new(r"c:\gAMES\保存したゲーム\emulator.exe"),
        ));
        assert!(!super::same_path(
            std::path::Path::new(r"C:\Games\保存したゲーム\Emulator.EXE"),
            std::path::Path::new(r"C:\Games\別のゲーム\Emulator.EXE"),
        ));
    }

    #[test]
    fn process_creation_timestamps_use_unsigned_filetime_ticks() {
        assert!(
            super::filetime_value(windows_sys::Win32::Foundation::FILETIME {
                dwLowDateTime: 0xffff_fffe,
                dwHighDateTime: 0xf234_5678,
            }) > i64::MAX as u64
        );
    }

    #[test]
    fn process_wait_failures_are_not_reported_as_exited() {
        let invalid_process = super::Handle(std::ptr::null_mut());
        assert!(super::process_is_running(&invalid_process)
            .unwrap_err()
            .contains("Could not confirm the Windows process state"));
    }

    #[test]
    fn job_object_tracks_and_stops_a_real_windows_child() {
        let mut child = Command::new("cmd.exe")
            .args(["/d", "/c", "cmd.exe /d /c ping -n 30 127.0.0.1 >NUL"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0000_0004 | 0x0000_0200)
            .spawn()
            .expect("Windows command processor is available");
        let pid = child.id();
        super::attach_child(pid).expect("child joins the managed job");

        let child_process = super::open_process(pid, super::PROCESS_QUERY_LIMITED_INFORMATION)
            .expect("managed child remains queryable");
        let studio_process =
            super::open_process(std::process::id(), super::PROCESS_QUERY_LIMITED_INFORMATION)
                .expect("test process remains queryable");
        let registered = super::jobs()
            .lock()
            .expect("process job registry is available");
        let job = registered.get(&pid).expect("child job is registered");
        assert!(super::process_belongs_to_job(&child_process, job.0));
        assert!(!super::process_belongs_to_job(&studio_process, job.0));
        drop(registered);

        assert!(super::snapshot(pid).is_some_and(|sample| sample.running));
        super::terminate_child(pid).expect("job termination succeeds");
        let status = child.wait().expect("terminated child is reaped");
        super::release_child(pid);
        assert!(!status.success());
    }

    #[test]
    fn recovered_session_stop_requires_the_matching_process_incarnation() {
        let mut child = Command::new("cmd.exe")
            .args(["/d", "/c", "cmd.exe /d /c ping -n 30 127.0.0.1 >NUL"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Windows command processor is available");
        let process = super::open_process(child.id(), super::PROCESS_QUERY_LIMITED_INFORMATION)
            .expect("child process can be inspected");
        let creation_time = super::process_creation_time(&process)
            .and_then(|value| i64::try_from(value).ok())
            .expect("process creation time fits the persisted session field");
        let executable = std::path::PathBuf::from(
            super::process_image(&process).expect("child executable is available"),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let descendant = loop {
            let tree =
                super::process_tree_entries().expect("Windows process snapshot is available");
            let intermediate_pids = tree
                .iter()
                .filter_map(|(pid, parent_pid)| (*parent_pid == child.id()).then_some(*pid))
                .collect::<std::collections::HashSet<_>>();
            let candidate = tree
                .into_iter()
                .filter(|(_, parent_pid)| intermediate_pids.contains(parent_pid))
                .find_map(|(pid, _)| {
                    let process =
                        super::open_process(pid, super::PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
                    let image = super::process_image(&process)?;
                    image
                        .to_ascii_lowercase()
                        .ends_with("\\ping.exe")
                        .then_some(process)
                });
            if let Some(descendant) = candidate {
                break descendant;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "cmd.exe started the expected ping.exe descendant"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };

        assert!(super::terminate_verified(child.id(), creation_time + 1, &executable,).is_err());
        assert!(super::snapshot(child.id()).is_some_and(|sample| sample.running));
        assert!(super::process_is_running(&descendant).unwrap());

        super::terminate_verified(child.id(), creation_time, &executable)
            .expect("the verified process can be stopped");
        let status = child.wait().expect("verified child is reaped");
        assert!(!status.success());
        assert!(
            !super::process_is_running(&descendant).unwrap(),
            "pre-existing descendant is included in the recovered cleanup job"
        );
    }
}
