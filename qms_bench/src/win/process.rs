use crate::{CounterReader, ResourceSample};
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, Thread32First, Thread32Next,
            PROCESSENTRY32W, TH32CS_SNAPPROCESS, TH32CS_SNAPTHREAD, THREADENTRY32,
        },
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
        Threading::{
            GetProcessHandleCount, GetProcessIoCounters, GetProcessTimes, OpenProcess,
            PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
        },
    },
};

pub struct WindowsProcessCounter {
    pub pids: Vec<u32>,
    pub names: Vec<String>,
}

impl WindowsProcessCounter {
    pub fn for_pid(pid: u32) -> Self {
        Self {
            pids: vec![pid],
            names: vec![format!("pid:{pid}")],
        }
    }

    pub fn for_pids(pids: Vec<u32>) -> Self {
        Self {
            pids,
            names: vec![],
        }
    }

    pub fn for_process_names(target_names: &[&str]) -> Result<Self, String> {
        let mut pids = Vec::new();
        for &name in target_names {
            let found = find_pids_by_name(name);
            pids.extend(found);
        }
        pids.sort_unstable();
        pids.dedup();

        if pids.is_empty() {
            return Err("process_not_found".into());
        }

        Ok(Self {
            pids,
            names: target_names.iter().map(|s| s.to_string()).collect(),
        })
    }
}

impl CounterReader for WindowsProcessCounter {
    fn read(&mut self) -> Result<ResourceSample, String> {
        if self.pids.is_empty() {
            return Err("no_targets".into());
        }

        let at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let mut total_working_set: u64 = 0;
        let mut total_private_bytes: u64 = 0;
        let mut total_cpu_time_ms: u64 = 0;
        let mut total_read_bytes: u64 = 0;
        let mut total_write_bytes: u64 = 0;
        let mut total_read_ops: u64 = 0;
        let mut total_write_ops: u64 = 0;
        let mut total_handles: u64 = 0;

        let thread_counts = count_threads_for_pids(&self.pids);
        let mut total_threads: u64 = 0;

        for &pid in &self.pids {
            let handle = open_process_for_query(pid)?;
            let _handle_guard = HandleGuard(handle);

            // 1. 메모리 (Working Set, Private Bytes)
            unsafe {
                let mut pmc: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
                pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
                if GetProcessMemoryInfo(
                    handle,
                    &mut pmc as *mut _ as *mut _,
                    std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                ) != 0
                {
                    total_working_set = total_working_set.saturating_add(pmc.WorkingSetSize as u64);
                    total_private_bytes =
                        total_private_bytes.saturating_add(pmc.PrivateUsage as u64);
                }
            }

            // 2. CPU 시간 (Kernel + User FILETIME)
            unsafe {
                let mut creation = std::mem::zeroed();
                let mut exit = std::mem::zeroed();
                let mut kernel = std::mem::zeroed();
                let mut user = std::mem::zeroed();
                if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) != 0 {
                    let k = ((kernel.dwHighDateTime as u64) << 32) | (kernel.dwLowDateTime as u64);
                    let u = ((user.dwHighDateTime as u64) << 32) | (user.dwLowDateTime as u64);
                    // 100ns 단위 -> 밀리초 (/ 10_000)
                    total_cpu_time_ms = total_cpu_time_ms.saturating_add((k + u) / 10_000);
                }
            }

            // 3. I/O 카운터
            unsafe {
                let mut io_counters = std::mem::zeroed();
                if GetProcessIoCounters(handle, &mut io_counters) != 0 {
                    total_read_bytes =
                        total_read_bytes.saturating_add(io_counters.ReadTransferCount);
                    total_write_bytes =
                        total_write_bytes.saturating_add(io_counters.WriteTransferCount);
                    total_read_ops = total_read_ops.saturating_add(io_counters.ReadOperationCount);
                    total_write_ops =
                        total_write_ops.saturating_add(io_counters.WriteOperationCount);
                }
            }

            // 4. 핸들 수
            unsafe {
                let mut handle_count: u32 = 0;
                if GetProcessHandleCount(handle, &mut handle_count) != 0 {
                    total_handles = total_handles.saturating_add(handle_count as u64);
                }
            }

            if let Some(&threads) = thread_counts.get(&pid) {
                total_threads = total_threads.saturating_add(threads as u64);
            }
        }

        Ok(ResourceSample {
            at_ms,
            working_set_bytes: Some(total_working_set),
            private_bytes: Some(total_private_bytes),
            cpu_time_ms: Some(total_cpu_time_ms),
            read_bytes: Some(total_read_bytes),
            write_bytes: Some(total_write_bytes),
            read_ops: Some(total_read_ops),
            write_ops: Some(total_write_ops),
            handles: Some(total_handles),
            threads: Some(total_threads),
        })
    }
}

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

fn open_process_for_query(pid: u32) -> Result<HANDLE, String> {
    unsafe {
        // 우선 PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ 시도
        let desired_access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ;
        let mut handle = OpenProcess(desired_access, 0, pid);
        if handle.is_null() {
            // 권한이 부족할 경우 PROCESS_QUERY_LIMITED_INFORMATION 만으로 재시도
            handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        }
        if handle.is_null() {
            // 그래도 실패하면 PROCESS_QUERY_INFORMATION 시도
            handle = OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid);
        }

        if handle.is_null() {
            let err = GetLastError();
            if err == 5 {
                // ERROR_ACCESS_DENIED
                return Err(format!("access_denied: pid {pid}"));
            } else if err == 87 {
                // ERROR_INVALID_PARAMETER (대개 프로세스 종료됨)
                return Err(format!("process_not_found: pid {pid}"));
            } else {
                return Err(format!("open_process_failed: pid {pid}, err {err}"));
            }
        }
        Ok(handle)
    }
}

pub fn find_pids_by_name(target_name: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    let clean_target = target_name.trim();
    let clean_with_exe = if !clean_target.ends_with(".exe") {
        format!("{clean_target}.exe")
    } else {
        clean_target.to_string()
    };

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
            return pids;
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let null_pos = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let exe_name = String::from_utf16_lossy(&entry.szExeFile[..null_pos]);

                if exe_name.eq_ignore_ascii_case(clean_target)
                    || exe_name.eq_ignore_ascii_case(&clean_with_exe)
                {
                    pids.push(entry.th32ProcessID);
                }

                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    pids
}

fn count_threads_for_pids(pids: &[u32]) -> std::collections::HashMap<u32, u32> {
    let mut map = std::collections::HashMap::new();
    let pid_set: std::collections::HashSet<u32> = pids.iter().copied().collect();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
            return map;
        }

        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

        if Thread32First(snapshot, &mut entry) != 0 {
            loop {
                if pid_set.contains(&entry.th32OwnerProcessID) {
                    *map.entry(entry.th32OwnerProcessID).or_insert(0) += 1;
                }
                if Thread32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snapshot);
    }
    map
}
