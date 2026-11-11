//! Reads processes, CPUs and memory. Portable through `sysinfo`; on Linux some extra
//! numbers (priority, nice, shared memory, CPU split) come straight from /proc like the original.

use std::collections::{HashMap, HashSet};
use std::time::Instant;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};

#[derive(Clone, Debug)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub tgid: u32,
    pub user: String,
    pub uid: String,
    pub is_mine: bool,
    pub pri: Option<i64>,
    pub nice: Option<i64>,
    pub virt_kb: u64,
    pub res_kb: u64,
    pub shr_kb: Option<u64>,
    pub state: char,
    pub cpu: f32,
    pub mem: f32,
    pub time_cs: u64,
    pub start_time: u64,
    pub nlwp: u32,
    pub processor: Option<u32>,
    pub cmdline: Vec<String>,
    pub comm: String,
    pub exe: Option<String>,
    pub is_thread: bool,
    pub is_kthread: bool,
    pub read_rate: f64,
    pub write_rate: f64,
    pub rbytes: u64,
    pub wbytes: u64,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct CpuSample {
    /// Percentages of one CPU: nice, user, kernel, virtual (steal + guest).
    pub nice: f64,
    pub user: f64,
    pub kernel: f64,
    pub virt: f64,
}

impl CpuSample {
    pub fn total(&self) -> f64 {
        self.nice + self.user + self.kernel + self.virt
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct MemInfo {
    pub total_kb: u64,
    pub used_kb: u64,
    pub shared_kb: u64,
    pub buffers_kb: u64,
    pub cache_kb: u64,
    pub swap_total_kb: u64,
    pub swap_used_kb: u64,
    pub swap_cache_kb: u64,
    pub compressed_kb: u64,
    pub swap_compressed_kb: u64,
}

pub struct Sampler {
    sys: System,
    users: Option<Users>,
    pub procs: Vec<Proc>,
    pub cpus: Vec<CpuSample>,
    pub avg_cpu: CpuSample,
    pub mem: MemInfo,
    pub load: (f64, f64, f64),
    pub uptime: u64,
    pub tasks: u32,
    pub threads: u32,
    pub kthreads: u32,
    pub running: u32,
    pub pid_width: usize,
    my_uid: Option<String>,
    last: Instant,
    #[allow(dead_code)]
    prev_stat: Vec<[u64; 10]>,
    #[allow(dead_code)]
    clk_tck: u64,
}

fn kb(bytes: u64) -> u64 {
    bytes / 1024
}

impl Sampler {
    pub fn new() -> Sampler {
        let sys = System::new().expect("system information is not available on this computer");
        let users = Users::new_with_refreshed_list().ok();
        let my_uid = None;
        #[cfg(unix)]
        let clk_tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
        #[cfg(not(unix))]
        let clk_tck = 100;
        let mut s = Sampler {
            sys,
            users,
            procs: Vec::new(),
            cpus: Vec::new(),
            avg_cpu: CpuSample::default(),
            mem: MemInfo::default(),
            load: (0.0, 0.0, 0.0),
            uptime: 0,
            tasks: 0,
            threads: 0,
            kthreads: 0,
            running: 0,
            pid_width: 5,
            my_uid,
            last: Instant::now(),
            prev_stat: Vec::new(),
            clk_tck,
        };
        s.pid_width = pid_width();
        s
    }

    pub fn cpu_count(&self) -> usize {
        self.cpus.len().max(1)
    }

    pub fn user_names(&self) -> Vec<String> {
        let mut v: Vec<String> = match &self.users {
            Some(u) => u.iter().map(|u| u.name().to_string()).collect(),
            None => Vec::new(),
        };
        v.sort();
        v.dedup();
        v
    }

    pub fn user_exists(&self, name: &str) -> bool {
        match &self.users {
            Some(u) => u.iter().any(|u| u.name() == name),
            None => true,
        }
    }

    /// Process environment, read on demand.
    pub fn environ(&mut self, pid: u32) -> Option<Vec<String>> {
        let p = Pid::from_u32(pid);
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[p]),
            false,
            ProcessRefreshKind::nothing().with_environ(UpdateKind::Always),
        );
        self.sys.process(p).map(|pr| pr.environ().iter().map(|s| s.to_string_lossy().into_owned()).collect())
    }

    pub fn kill(&self, pid: u32, sig: sysinfo::Signal) -> bool {
        match self.sys.process(Pid::from_u32(pid)) {
            Some(p) => p.kill_with(sig).unwrap_or_else(|| p.kill()),
            None => false,
        }
    }

    pub fn scan(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64().max(0.001);
        self.last = now;
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_user(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet)
                .with_tasks(),
        );
        if self.my_uid.is_none() {
            if let Ok(me) = sysinfo::get_current_pid() {
                self.my_uid = self.sys.process(me).and_then(|p| p.user_id()).map(|u| u.to_string());
            }
        }
        self.read_cpus();
        self.read_memory();
        if let Ok(l) = System::load_average() {
            self.load = (l.one, l.five, l.fifteen);
        }
        self.uptime = System::uptime().unwrap_or(0);
        self.read_procs(dt);
    }

    fn read_cpus(&mut self) {
        #[cfg(target_os = "linux")]
        if self.read_cpus_linux() {
            return;
        }
        let list: Vec<CpuSample> = self
            .sys
            .cpus()
            .iter()
            .map(|c| CpuSample { user: c.usage() as f64, ..Default::default() })
            .collect();
        self.avg_cpu = CpuSample { user: self.sys.global_cpu_usage() as f64, ..Default::default() };
        self.cpus = list;
    }

    #[cfg(target_os = "linux")]
    fn read_cpus_linux(&mut self) -> bool {
        let Ok(text) = std::fs::read_to_string("/proc/stat") else { return false };
        let mut rows: Vec<[u64; 10]> = Vec::new();
        for line in text.lines() {
            if !line.starts_with("cpu") {
                break;
            }
            let mut v = [0u64; 10];
            for (i, f) in line.split_whitespace().skip(1).take(10).enumerate() {
                v[i] = f.parse().unwrap_or(0);
            }
            rows.push(v);
        }
        if rows.len() < 2 {
            return false;
        }
        let calc = |now: &[u64; 10], old: &[u64; 10]| -> CpuSample {
            let d: Vec<f64> = (0..10).map(|i| now[i].saturating_sub(old[i]) as f64).collect();
            // user nice system idle iowait irq softirq steal guest guest_nice
            let guest = d[8] + d[9];
            let user = (d[0] - d[8]).max(0.0);
            let nice = (d[1] - d[9]).max(0.0);
            let total = user + nice + d[2] + d[3] + d[4] + d[5] + d[6] + d[7] + guest;
            if total <= 0.0 {
                return CpuSample::default();
            }
            CpuSample {
                nice: nice / total * 100.0,
                user: user / total * 100.0,
                kernel: (d[2] + d[5] + d[6]) / total * 100.0,
                virt: (d[7] + guest) / total * 100.0,
            }
        };
        if self.prev_stat.len() == rows.len() {
            self.avg_cpu = calc(&rows[0], &self.prev_stat[0]);
            self.cpus = (1..rows.len()).map(|i| calc(&rows[i], &self.prev_stat[i])).collect();
        } else {
            self.cpus = vec![CpuSample::default(); rows.len() - 1];
        }
        self.prev_stat = rows;
        true
    }

    fn read_memory(&mut self) {
        #[cfg(target_os = "linux")]
        if let Ok(text) = std::fs::read_to_string("/proc/meminfo") {
            let mut m: HashMap<&str, u64> = HashMap::new();
            for line in text.lines() {
                let mut it = line.split_whitespace();
                if let (Some(k), Some(v)) = (it.next(), it.next()) {
                    m.insert(k.trim_end_matches(':'), v.parse().unwrap_or(0));
                }
            }
            let g = |k: &str| *m.get(k).unwrap_or(&0);
            let total = g("MemTotal");
            let free = g("MemFree");
            let shared = g("Shmem");
            let buffers = g("Buffers");
            let cached = g("Cached") + g("SReclaimable");
            let diff = free + cached + buffers;
            let used = if total >= diff { total - diff } else { total - free };
            self.mem = MemInfo {
                total_kb: total,
                // Machine_scan 0x41bc0: used = total - (free + buffers + cached + sreclaimable),
                // which already leaves shared memory out (it is inside "Cached").
                used_kb: used.saturating_sub(g("Zswap")),
                compressed_kb: g("Zswap"),
                shared_kb: shared,
                buffers_kb: buffers,
                cache_kb: cached.saturating_sub(shared),
                swap_total_kb: g("SwapTotal"),
                // Platform_setSwapValues 0x3bca0: zswapped pages are shown as their own segment
                swap_used_kb: g("SwapTotal").saturating_sub(g("SwapFree")).saturating_sub(g("SwapCached")).saturating_sub(g("Zswapped")),
                swap_compressed_kb: g("Zswapped"),
                swap_cache_kb: g("SwapCached"),
            };
            return;
        }
        let total = kb(self.sys.total_memory());
        let avail = kb(self.sys.available_memory());
        let free = kb(self.sys.free_memory());
        self.mem = MemInfo {
            total_kb: total,
            used_kb: total.saturating_sub(avail),
            shared_kb: 0,
            buffers_kb: 0,
            cache_kb: avail.saturating_sub(free),
            swap_total_kb: kb(self.sys.total_swap()),
            swap_used_kb: kb(self.sys.used_swap()),
            swap_cache_kb: 0,
            compressed_kb: 0,
            swap_compressed_kb: 0,
        };
    }

    fn read_procs(&mut self, dt: f64) {
        let total_kb = (self.mem.total_kb.max(1)) as f64;
        // Which process does each thread belong to?
        let mut owner: HashMap<u32, u32> = HashMap::new();
        for (pid, p) in self.sys.processes() {
            if let Some(tasks) = p.tasks() {
                for t in tasks {
                    if t != pid {
                        owner.insert(t.as_u32(), pid.as_u32());
                    }
                }
            }
        }
        let mut out = Vec::with_capacity(self.sys.processes().len());
        let (mut tasks, mut threads, mut kthreads, mut running) = (0, 0, 0, 0);
        let mut kernel_pids: HashSet<u32> = HashSet::new();
        for (pid, p) in self.sys.processes() {
            let pid = pid.as_u32();
            let kind = p.thread_kind();
            let is_thread = matches!(kind, Some(sysinfo::ThreadKind::Userland)) || owner.contains_key(&pid);
            let mut is_kthread = matches!(kind, Some(sysinfo::ThreadKind::Kernel));
            #[cfg(target_os = "linux")]
            {
                if !is_kthread && p.cmd().is_empty() && p.exe().is_none() && pid != 1 {
                    // Kernel threads have no command line and no program file.
                    is_kthread = p.parent().map(|pp| pp.as_u32() == 2).unwrap_or(false) || pid == 2;
                }
            }
            if pid == 0 {
                is_kthread = false;
            }
            let tgid = owner.get(&pid).copied().unwrap_or(pid);
            let ppid = if is_thread && tgid != pid { tgid } else { p.parent().map(|x| x.as_u32()).unwrap_or(0) };
            let uid = p.user_id().map(|u| u.to_string()).unwrap_or_default();
            let user = p
                .user_id()
                .and_then(|u| self.users.as_ref().and_then(|us| us.get_user_by_id(u)))
                .map(|u| u.name().to_string())
                .unwrap_or_else(|| if uid.is_empty() { "?".into() } else { uid.clone() });
            let is_mine = self.my_uid.as_deref().map(|m| m == uid).unwrap_or(true);
            let du = p.disk_usage();
            let comm = p.name().to_string_lossy().into_owned();
            let cmdline: Vec<String> = p.cmd().iter().map(|s| s.to_string_lossy().into_owned()).collect();
            let mut pr = Proc {
                pid,
                ppid,
                tgid,
                user,
                uid,
                is_mine,
                pri: None,
                nice: None,
                virt_kb: kb(p.virtual_memory()),
                res_kb: kb(p.memory()),
                shr_kb: None,
                state: state_char(p.status()),
                cpu: p.cpu_usage(),
                mem: (kb(p.memory()) as f64 / total_kb * 100.0) as f32,
                time_cs: p.accumulated_cpu_time() / 10,
                start_time: p.start_time(),
                nlwp: p.tasks().map(|t| t.len() as u32).unwrap_or(1).max(1),
                processor: None,
                cmdline,
                comm,
                exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
                is_thread,
                is_kthread,
                read_rate: du.read_bytes as f64 / dt,
                write_rate: du.written_bytes as f64 / dt,
                rbytes: du.total_read_bytes,
                wbytes: du.total_written_bytes,
            };
            self.platform_extras(&mut pr);
            if is_kthread {
                kthreads += 1;
                kernel_pids.insert(pid);
            } else if is_thread {
                threads += 1;
            } else {
                tasks += 1;
            }
            if pr.state == 'R' {
                running += 1;
            }
            out.push(pr);
        }
        // Threads of kernel threads are kernel threads too.
        for p in out.iter_mut() {
            if p.is_thread && kernel_pids.contains(&p.tgid) {
                p.is_kthread = true;
            }
        }
        self.tasks = tasks;
        self.threads = threads;
        self.kthreads = kthreads;
        self.running = running;
        self.procs = out;
    }

    #[cfg(target_os = "linux")]
    fn platform_extras(&self, p: &mut Proc) {
        let base = if p.tgid != p.pid { format!("/proc/{}/task/{}", p.tgid, p.pid) } else { format!("/proc/{}", p.pid) };
        if let Ok(stat) = std::fs::read_to_string(format!("{base}/stat")) {
            if let Some(close) = stat.rfind(')') {
                let f: Vec<&str> = stat[close + 1..].split_whitespace().collect();
                // f[0] = state (field 3), so field n is f[n-3].
                if let Some(s) = f.first().and_then(|s| s.chars().next()) {
                    p.state = s;
                }
                let num = |n: usize| f.get(n - 3).and_then(|s| s.parse::<i64>().ok());
                p.pri = num(18);
                p.nice = num(19);
                if let (Some(u), Some(s)) = (num(14), num(15)) {
                    p.time_cs = ((u + s).max(0) as u64) * 100 / self.clk_tck;
                }
                if let Some(c) = num(39) {
                    p.processor = Some(c as u32);
                }
                if let Some(n) = num(20) {
                    p.nlwp = n.max(1) as u32;
                }
            }
        }
        if let Ok(statm) = std::fs::read_to_string(format!("{base}/statm")) {
            let page_kb = 4u64;
            if let Some(sh) = statm.split_whitespace().nth(2).and_then(|s| s.parse::<u64>().ok()) {
                p.shr_kb = Some(sh * page_kb);
            }
        }
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    fn platform_extras(&self, p: &mut Proc) {
        unsafe {
            *libc_errno() = 0;
            let n = libc::getpriority(libc::PRIO_PROCESS, p.pid as libc::id_t);
            if *libc_errno() == 0 {
                p.nice = Some(n as i64);
                p.pri = Some(20 + n as i64);
            }
        }
    }

    #[cfg(not(unix))]
    fn platform_extras(&self, _p: &mut Proc) {}
}

#[cfg(all(unix, not(target_os = "linux")))]
unsafe fn libc_errno() -> *mut libc::c_int {
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    {
        unsafe { libc::__error() }
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd")))]
    {
        unsafe { libc::__errno() }
    }
}

fn state_char(s: sysinfo::ProcessStatus) -> char {
    use sysinfo::ProcessStatus as S;
    match s {
        S::Run => 'R',
        S::Sleep => 'S',
        S::Idle => 'I',
        S::Zombie => 'Z',
        S::Stop => 'T',
        S::Tracing => 't',
        S::Dead => 'X',
        S::UninterruptibleDiskSleep => 'D',
        S::Waking | S::Wakekill => 'W',
        S::Parked => 'P',
        S::LockBlocked => 'L',
        S::Suspended => 'T',
        _ => '?',
    }
}

fn pid_width() -> usize {
    #[cfg(target_os = "linux")]
    if let Ok(s) = std::fs::read_to_string("/proc/sys/kernel/pid_max") {
        if let Ok(n) = s.trim().parse::<u64>() {
            return (n.saturating_sub(1)).to_string().len().max(5);
        }
    }
    #[cfg(target_os = "macos")]
    return 5;
    #[allow(unreachable_code)]
    7
}

/// Change a process's nice value by `delta`. Returns false when the system refuses.
pub fn renice(pid: u32, current: Option<i64>, delta: i64) -> bool {
    #[cfg(unix)]
    unsafe {
        let cur = current.unwrap_or_else(|| libc::getpriority(libc::PRIO_PROCESS, pid as libc::id_t) as i64);
        let new = (cur + delta).clamp(-20, 19);
        return libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, new as libc::c_int) == 0;
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, current, delta);
        false
    }
}
