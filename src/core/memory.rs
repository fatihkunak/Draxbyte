//! Memory overview, per-app memory use, closing apps the user picked, and the working-set trim.
//!
//! Only explicit PIDs chosen by the user are ever touched; system processes and Draxbyte itself
//! are refused (`is_protected_process`). Nothing here runs by itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemInfo {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

impl MemInfo {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
}

/// One running process, as read from the OS.
#[derive(Debug, Clone)]
pub struct RawProc {
    pub pid: u32,
    pub name: String,
    pub exe: Option<PathBuf>,
    pub bytes: u64,
}

/// All processes of one program (a browser has dozens) shown as one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcGroup {
    pub name: String,
    pub exe: Option<PathBuf>,
    pub pids: Vec<u32>,
    pub bytes: u64,
    pub protected: bool,
}

const WIN_PROTECTED: &[&str] = &[
    "system", "system idle process", "registry", "memory compression", "secure system", "smss.exe",
    "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "svchost.exe",
    "fontdrvhost.exe", "dwm.exe", "explorer.exe", "sihost.exe", "ctfmon.exe", "taskhostw.exe",
    "runtimebroker.exe", "audiodg.exe", "spoolsv.exe", "wudfhost.exe", "dllhost.exe", "conhost.exe",
    "msmpeng.exe", "securityhealthservice.exe", "searchindexer.exe", "startmenuexperiencehost.exe",
    "shellexperiencehost.exe", "textinputhost.exe", "lockapp.exe", "logonui.exe",
];
const UNIX_PROTECTED: &[&str] = &[
    "launchd", "kernel_task", "windowserver", "loginwindow", "dock", "finder", "systemuiserver",
    "coreservicesd", "opendirectoryd", "securityd", "syslogd", "systemd", "init", "kthreadd",
    "dbus-daemon", "xorg", "gnome-shell", "sshd",
];

/// True for anything that must never be closed from here: the OS itself, shells, ourselves.
pub fn is_protected_process(name: &str, exe: Option<&Path>, pid: u32, own_pid: u32) -> bool {
    if pid == own_pid || pid == 0 || (pid == 4 && cfg!(windows)) || (pid == 1 && !cfg!(windows)) {
        return true;
    }
    let n = name.to_lowercase();
    if WIN_PROTECTED.contains(&n.as_str()) || UNIX_PROTECTED.contains(&n.as_str()) {
        return true;
    }
    if let Some(e) = exe {
        let s = e.to_string_lossy().replace('\\', "/").to_lowercase();
        let sys = [
            "/windows/system32/", "/windows/syswow64/", "/windows/winsxs/", "/system/library/",
            "/usr/libexec/", "/usr/sbin/", "/sbin/", "/system/applications/",
        ];
        if sys.iter().any(|p| s.contains(p)) {
            return true;
        }
    }
    false
}

/// Groups processes by executable (falling back to name) and sorts by memory, biggest first.
pub fn group(procs: Vec<RawProc>, own_pid: u32) -> Vec<ProcGroup> {
    let mut by: BTreeMap<String, ProcGroup> = BTreeMap::new();
    for p in procs {
        let key = p
            .exe
            .as_ref()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| p.name.to_lowercase());
        let prot = is_protected_process(&p.name, p.exe.as_deref(), p.pid, own_pid);
        let g = by.entry(key).or_insert_with(|| ProcGroup {
            name: p.name.clone(),
            exe: p.exe.clone(),
            pids: Vec::new(),
            bytes: 0,
            protected: false,
        });
        g.pids.push(p.pid);
        g.bytes += p.bytes;
        g.protected |= prot;
    }
    let mut v: Vec<ProcGroup> = by.into_values().collect();
    v.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    v
}

fn read_system() -> (sysinfo::System, u32) {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    (sys, std::process::id())
}

/// Memory totals plus the process groups, biggest first.
pub fn snapshot() -> (MemInfo, Vec<ProcGroup>) {
    let (sys, own) = read_system();
    let info = MemInfo {
        total: sys.total_memory(),
        available: sys.available_memory(),
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
    };
    let procs = sys
        .processes()
        .iter()
        .map(|(pid, p)| RawProc {
            pid: pid.as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            exe: p.exe().map(|e| e.to_path_buf()),
            bytes: p.memory(),
        })
        .collect();
    (info, group(procs, own))
}

pub fn available_memory() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.available_memory()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CloseReport {
    pub asked: usize,
    pub refused: Vec<u32>,
    pub failed: Vec<u32>,
}

/// Asks the given processes to quit. `force` ends them at once (unsaved work is lost);
/// otherwise Windows gets a normal close request (`taskkill` without `/F`) and unix gets SIGTERM.
/// Protected processes and PIDs that no longer exist are reported, never touched.
pub fn close_pids(pids: &[u32], force: bool) -> CloseReport {
    let (sys, own) = read_system();
    let mut rep = CloseReport::default();
    for &pid in pids {
        let Some(p) = sys.process(sysinfo::Pid::from_u32(pid)) else {
            rep.failed.push(pid);
            continue;
        };
        let name = p.name().to_string_lossy().into_owned();
        if is_protected_process(&name, p.exe(), pid, own) {
            rep.refused.push(pid);
            continue;
        }
        rep.asked += 1;
        let ok = if force {
            p.kill()
        } else if cfg!(windows) {
            let mut c = std::process::Command::new("taskkill");
            c.args(["/PID", &pid.to_string()]);
            hide_window(&mut c);
            c.output().map(|o| o.status.success()).unwrap_or(false)
        } else {
            p.kill_with(sysinfo::Signal::Term).unwrap_or(false)
        };
        if !ok {
            rep.failed.push(pid);
        }
    }
    rep
}

#[cfg(windows)]
pub fn hide_window(c: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
pub fn hide_window(_c: &mut std::process::Command) {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrimReport {
    pub trimmed: usize,
    pub skipped: usize,
    pub available_before: u64,
    pub available_after: u64,
}

/// Moves the idle part of every process's memory out of RAM (Windows `EmptyWorkingSet`).
/// This is what "RAM cleaners" do: memory frees at once, but a process that needs those pages
/// again reads them back from the page file, so the gain is temporary. Windows only.
#[cfg(windows)]
pub fn trim_working_sets() -> Result<TrimReport, String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::K32EmptyWorkingSet;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
    };

    let before = available_memory();
    let (sys, _) = read_system();
    let (mut trimmed, mut skipped) = (0, 0);
    for pid in sys.processes().keys().map(|p| p.as_u32()) {
        if pid == 0 || pid == 4 {
            continue;
        }
        // SAFETY: plain Win32 calls; the handle is closed right after use.
        let ok = unsafe {
            let h = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION, 0, pid);
            if h.is_null() {
                false
            } else {
                let r = K32EmptyWorkingSet(h) != 0;
                CloseHandle(h);
                r
            }
        };
        if ok {
            trimmed += 1;
        } else {
            skipped += 1;
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(700));
    Ok(TrimReport {
        trimmed,
        skipped,
        available_before: before,
        available_after: available_memory(),
    })
}

#[cfg(not(windows))]
pub fn trim_working_sets() -> Result<TrimReport, String> {
    Err(crate::i18n::t(
        "Bu işlem yalnızca Windows'ta var (macOS'ta belleği sistem kendisi yönetir).",
        "Only available on Windows (macOS manages memory itself).",
    )
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, name: &str, exe: &str, mb: u64) -> RawProc {
        RawProc { pid, name: name.into(), exe: Some(PathBuf::from(exe)), bytes: mb << 20 }
    }

    #[test]
    fn processes_of_one_program_become_one_row_sorted_by_memory() {
        let g = group(
            vec![
                p(10, "chrome.exe", r"C:\Chrome\chrome.exe", 300),
                p(11, "chrome.exe", r"C:\Chrome\chrome.exe", 200),
                p(12, "notepad.exe", r"C:\Win\notepad.exe", 600),
                p(13, "Code.exe", r"C:\VS\Code.exe", 100),
            ],
            9999,
        );
        assert_eq!(g.len(), 3);
        assert_eq!((g[0].name.as_str(), g[0].bytes >> 20), ("notepad.exe", 600));
        assert_eq!((g[1].name.as_str(), g[1].pids.len(), g[1].bytes >> 20), ("chrome.exe", 2, 500));
    }

    #[test]
    fn system_shell_and_self_are_protected() {
        assert!(is_protected_process("lsass.exe", None, 700, 1));
        assert!(is_protected_process("explorer.exe", None, 800, 1));
        assert!(is_protected_process(
            "whatever.exe",
            Some(Path::new(r"C:\Windows\System32\whatever.exe")),
            5,
            1
        ));
        assert!(is_protected_process("draxbyte.exe", None, 4242, 4242));
        assert!(is_protected_process("Dock", None, 300, 1));
        assert!(!is_protected_process(
            "chrome.exe",
            Some(Path::new(r"C:\Program Files\Chrome\chrome.exe")),
            55,
            1
        ));
    }

    #[test]
    fn group_is_protected_when_any_member_is() {
        let g = group(vec![p(1, "svchost.exe", r"C:\Windows\System32\svchost.exe", 50)], 9);
        assert!(g[0].protected);
    }

    #[test]
    fn snapshot_reads_real_memory_and_protects_ourselves() {
        let (info, groups) = snapshot();
        assert!(info.total > 0 && info.available <= info.total);
        let own = std::process::id();
        let mine = groups.iter().find(|g| g.pids.contains(&own)).expect("own process listed");
        assert!(mine.protected);
    }

    #[test]
    fn closing_a_protected_or_missing_pid_does_nothing() {
        let own = std::process::id();
        let rep = close_pids(&[own, 4_000_000_000], true);
        assert_eq!(rep.asked, 0);
        assert_eq!(rep.refused, vec![own]);
        assert_eq!(rep.failed, vec![4_000_000_000]);
    }
}
