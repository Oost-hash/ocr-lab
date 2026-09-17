// Lab observer preserved from the original harness. This is NOT the production
// watcher: it polls EE.log and uses an inlined memory scanner, without the
// production reward-session, OCR retry, or dismiss lifecycle.
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::Emitter;

const OPEN_PAT: &[u8] = b"VoidProjections: GetVoidProjectionRewards\r";
const LOG_TRIGGER: &str = "voidprojections: getvoidprojectionreward";

#[derive(Clone)]
struct MemoryRegionInfo {
    base_address: usize,
    region_size: usize,
    is_committed: bool,
    is_readable: bool,
    is_writable: bool,
}

struct WindowsProcessHandle {
    handle: isize,
}

impl Drop for WindowsProcessHandle {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle); }
    }
}

impl WindowsProcessHandle {
    fn read_memory(&self, addr: usize, len: usize) -> Option<Vec<u8>> {
        use std::ffi::c_void;
        use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
        unsafe {
            let mut buf = vec![0u8; len];
            let mut read = 0usize;
            if ReadProcessMemory(self.handle, addr as *const c_void, buf.as_mut_ptr() as *mut c_void, len, &mut read) == 0 || read == 0 {
                return None;
            }
            buf.truncate(read);
            Some(buf)
        }
    }

    fn regions_from(&self, start: usize) -> Vec<MemoryRegionInfo> {
        use std::ffi::c_void;
        use std::mem;
        use windows_sys::Win32::System::Memory::{VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT};
        unsafe {
            let mut regions = Vec::new();
            let mut address = start;
            loop {
                let mut mbi: MEMORY_BASIC_INFORMATION = mem::zeroed();
                if VirtualQueryEx(self.handle, address as *const c_void, &mut mbi, mem::size_of::<MEMORY_BASIC_INFORMATION>()) == 0 { break; }
                let next = (mbi.BaseAddress as usize).saturating_add(mbi.RegionSize);
                if next <= address { break; }
                address = next;
                let protect = mbi.Protect;
                regions.push(MemoryRegionInfo {
                    base_address: mbi.BaseAddress as usize,
                    region_size: mbi.RegionSize,
                    is_committed: mbi.State == MEM_COMMIT,
                    is_readable: mbi.State == MEM_COMMIT && (protect & 0x02 != 0 || protect & 0x04 != 0 || protect & 0x08 != 0 || protect & 0x20 != 0 || protect & 0x40 != 0 || protect & 0x80 != 0),
                    is_writable: mbi.State == MEM_COMMIT && (protect & 0x04 != 0 || protect & 0x08 != 0 || protect & 0x40 != 0 || protect & 0x80 != 0),
                });
            }
            regions
        }
    }
}

fn find_warframe_pid() -> Option<u32> {
    use std::mem;
    use windows_sys::Win32::{Foundation::{CloseHandle, INVALID_HANDLE_VALUE}, System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS}};
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE { return None; }
        let mut entry: PROCESSENTRY32 = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32>() as u32;
        let mut result = None;
        if Process32First(snapshot, &mut entry) != 0 {
            loop {
                let length = entry.szExeFile.iter().position(|byte| *byte == 0).unwrap_or(260);
                let name = String::from_utf8_lossy(&entry.szExeFile[..length]).to_lowercase();
                if name.starts_with("warframe") && !["launcher", "companion", "crash", "downloader", "installer", "updater"].iter().any(|excluded| name.contains(excluded)) {
                    result = Some(entry.th32ProcessID);
                    break;
                }
                if Process32Next(snapshot, &mut entry) == 0 { break; }
            }
        }
        CloseHandle(snapshot);
        result
    }
}

fn open_process(pid: u32) -> Option<WindowsProcessHandle> {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
    (handle != 0).then_some(WindowsProcessHandle { handle })
}

// Production-derived scanner with platform abstraction inlined for the lab.
fn scan_heap_for_trigger(pid: u32, pattern: &[u8], cached_bare: Option<u64>) -> (bool, String, Option<u64>) {
    const FULL_MIN: u64 = 0x0000_0001_0000_0000;
    const FULL_MAX: u64 = 0x0000_8000_0000_0000;
    const NARROW_R: u64 = 128 * 1024 * 1024;
    const REGION_MAX: usize = 32 * 1024 * 1024;
    let bare_pattern = &pattern[..pattern.len().saturating_sub(1)];
    let (scan_min, scan_max, rw_only) = match cached_bare {
        Some(address) => (address.saturating_sub(NARROW_R), address.saturating_add(NARROW_R), true),
        None => (FULL_MIN, FULL_MAX, false),
    };
    let Some(handle) = open_process(pid) else { return (false, format!("OpenProcess failed pid={pid}"), cached_bare); };
    let started = std::time::Instant::now();
    let mut address = scan_min as usize;
    let mut regions_read = 0u32;
    let mut bare_hit = None;
    let mut found = false;
    while address < scan_max as usize {
        let regions = handle.regions_from(address);
        if regions.is_empty() { break; }
        for region in regions {
            address = region.base_address + region.region_size;
            if region.base_address < scan_min as usize || region.base_address >= scan_max as usize || !region.is_committed || !region.is_readable || (rw_only && !region.is_writable) || region.region_size > REGION_MAX { continue; }
            let Some(bytes) = handle.read_memory(region.base_address, region.region_size) else { continue; };
            regions_read += 1;
            if bare_hit.is_none() {
                bare_hit = bytes.windows(bare_pattern.len()).position(|window| window == bare_pattern).map(|offset| region.base_address as u64 + offset as u64);
            }
            if bytes.windows(pattern.len()).any(|window| window == pattern) { found = true; break; }
        }
        if found { break; }
    }
    let mode = if cached_bare.is_some() { "narrow" } else { "full" };
    let diagnostic = format!("{mode} scan in {}ms: {regions_read} regions, bare={}, live={found}", started.elapsed().as_millis(), bare_hit.map_or("none".to_string(), |address| format!("{address:#x}")));
    (found, diagnostic, if cached_bare.is_none() { bare_hit } else { cached_bare })
}

fn start_ee_log_trigger(app: tauri::AppHandle, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let Some(log_path) = dirs::data_local_dir().map(|path| path.join("Warframe").join("EE.log")) else { return; };
        let mut offset = std::fs::metadata(&log_path).map(|metadata| metadata.len()).unwrap_or(0);
        while running.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let Ok(mut file) = std::fs::File::open(&log_path) else { continue; };
            let length = std::fs::metadata(&log_path).map(|metadata| metadata.len()).unwrap_or(0);
            if length < offset { offset = 0; }
            if length == offset || file.seek(SeekFrom::Start(offset)).is_err() { continue; }
            let mut appended = String::new();
            if file.read_to_string(&mut appended).is_err() { continue; }
            offset = length;
            if appended.to_lowercase().contains(LOG_TRIGGER) {
                let _ = app.emit("ocr-production-trigger", serde_json::json!({ "source": "ee_log", "detail": "VoidProjections reward event" }));
            }
        }
    });
}

fn start_memory_trigger(app: tauri::AppHandle, running: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut open = false;
        let mut opened_at = None;
        let mut cached_bare = None;
        let mut last_pid = 0;
        while running.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if open && opened_at.is_some_and(|time: std::time::Instant| time.elapsed().as_secs() < 90) { continue; }
            open = false;
            let Some(pid) = find_warframe_pid() else { cached_bare = None; last_pid = 0; continue; };
            if pid != last_pid { cached_bare = None; last_pid = pid; }
            let (found, detail, next_bare) = scan_heap_for_trigger(pid, OPEN_PAT, cached_bare);
            if cached_bare.is_none() { cached_bare = next_bare; }
            let _ = app.emit("ocr-production-scan", serde_json::json!({ "source": "memory", "detail": detail }));
            if found {
                open = true;
                opened_at = Some(std::time::Instant::now());
                let _ = app.emit("ocr-production-trigger", serde_json::json!({ "source": "memory", "detail": "VoidProjections live EE.log ring buffer" }));
            }
        }
    });
}

pub(crate) fn start(app: tauri::AppHandle) {
    let running = Arc::new(AtomicBool::new(true));
    start_ee_log_trigger(app.clone(), Arc::clone(&running));
    start_memory_trigger(app, running);
}
