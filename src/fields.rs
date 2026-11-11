//! The columns of the process list and the number formats (see MECHANICS.md).

use crate::data::Proc;
use crate::screen::{Col, Style};

pub type Seg = (String, Style);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Field {
    Pid,
    Command,
    State,
    Ppid,
    Priority,
    Nice,
    StartTime,
    Processor,
    MVirt,
    MResident,
    MShare,
    StUid,
    PercentCpu,
    PercentNormCpu,
    PercentMem,
    User,
    Time,
    Nlwp,
    Tgid,
    Elapsed,
    RBytes,
    WBytes,
    IoReadRate,
    IoWriteRate,
    IoRate,
    IoPriority,
    PercentSwapDelay,
    PercentIoDelay,
    Comm,
    Exe,
}

pub struct FieldInfo {
    pub f: Field,
    pub name: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub sort_desc: bool,
}

macro_rules! fi {
    ($f:ident, $n:expr, $t:expr, $d:expr, $s:expr) => {
        FieldInfo { f: Field::$f, name: $n, title: $t, desc: $d, sort_desc: $s }
    };
}

pub const FIELDS: &[FieldInfo] = &[
    fi!(Pid, "PID", "PID", "Process or thread number", false),
    fi!(Command, "Command", "Command", "Full command line", false),
    fi!(State, "STATE", "S", "State (R running, S sleeping, D disk wait, Z zombie, T stopped, I idle)", false),
    fi!(Ppid, "PPID", "PPID", "Parent process number", false),
    fi!(Priority, "PRIORITY", "PRI", "Scheduling priority given by the kernel", false),
    fi!(Nice, "NICE", " NI", "Nice value (higher lets other processes go first)", false),
    fi!(StartTime, "STARTTIME", "START", "Clock time when the process started", false),
    fi!(Processor, "PROCESSOR", "CPU", "CPU the process last ran on", false),
    fi!(MVirt, "M_VIRT", " VIRT", "Virtual memory size", true),
    fi!(MResident, "M_RESIDENT", "  RES", "Resident memory (what is really in RAM)", true),
    fi!(MShare, "M_SHARE", "  SHR", "Shared memory pages", true),
    fi!(StUid, "ST_UID", "  UID", "Owner's user id", false),
    fi!(PercentCpu, "PERCENT_CPU", " CPU%", "CPU use in the last interval (100 = one whole CPU)", true),
    fi!(PercentNormCpu, "PERCENT_NORM_CPU", "NCPU%", "CPU use divided by the number of CPUs", true),
    fi!(PercentMem, "PERCENT_MEM", "MEM%", "Share of the computer's memory in use (resident)", true),
    fi!(User, "USER", "USER      ", "Owner's user name", false),
    fi!(Time, "TIME", "  TIME+ ", "CPU time used, user plus system", true),
    fi!(Nlwp, "NLWP", "NLWP", "Number of threads", true),
    fi!(Tgid, "TGID", "TGID", "Thread group (the process the thread belongs to)", false),
    fi!(Elapsed, "ELAPSED", "ELAPSED  ", "Time since the process started", true),
    fi!(RBytes, "RBYTES", " IO_RD", "Bytes read from storage", true),
    fi!(WBytes, "WBYTES", " IO_WR", "Bytes written to storage", true),
    fi!(IoReadRate, "IO_READ_RATE", "  DISK READ", "Read speed in bytes per second", true),
    fi!(IoWriteRate, "IO_WRITE_RATE", " DISK WRITE", "Write speed in bytes per second", true),
    fi!(IoRate, "IO_RATE", "   DISK R/W", "Read plus write speed in bytes per second", true),
    fi!(IoPriority, "IO_PRIORITY", "IO", "Input/output priority", false),
    fi!(PercentSwapDelay, "PERCENT_SWAP_DELAY", "SWPD%", "Time spent waiting for swap (needs Linux delay accounting)", true),
    fi!(PercentIoDelay, "PERCENT_IO_DELAY", " IOD%", "Time spent waiting for disk (needs Linux delay accounting)", true),
    fi!(Comm, "COMM", "COMM           ", "Short program name", false),
    fi!(Exe, "EXE", "EXE            ", "Program file name", false),
];

pub fn info(f: Field) -> &'static FieldInfo {
    FIELDS.iter().find(|i| i.f == f).unwrap()
}

/// Exact name, as `-s` compares with strcmp (CommandLine_run 0x1d310).
pub fn by_exact_name(name: &str) -> Option<Field> {
    FIELDS.iter().find(|i| i.name == name).map(|i| i.f)
}

pub fn by_name(name: &str) -> Option<Field> {
    FIELDS.iter().find(|i| i.name.eq_ignore_ascii_case(name)).map(|i| i.f)
}

pub const MAIN_COLUMNS: &[Field] = &[
    Field::Pid, Field::User, Field::Priority, Field::Nice, Field::MVirt, Field::MResident,
    Field::MShare, Field::State, Field::PercentCpu, Field::PercentMem, Field::Time, Field::Command,
];

pub const IO_COLUMNS: &[Field] = &[
    Field::Pid, Field::User, Field::IoPriority, Field::IoRate, Field::IoReadRate,
    Field::IoWriteRate, Field::PercentSwapDelay, Field::PercentIoDelay, Field::Command,
];

// ------------------------------------------------------------------ colours ---

pub const C_DEFAULT: Style = Style::new(Col::Default, Col::Default, false);
pub const C_SHADOW: Style = Style::new(Col::Gray, Col::Default, true);
pub const C_MEGA: Style = Style::new(Col::Cyan, Col::Default, false);
pub const C_GIGA: Style = Style::new(Col::Green, Col::Default, false);
pub const C_LARGE: Style = Style::new(Col::Red, Col::Default, false);
pub const C_TREE: Style = Style::new(Col::Cyan, Col::Default, false);
pub const C_THREAD: Style = Style::new(Col::Green, Col::Default, false);
pub const C_HIGH_PRI: Style = Style::new(Col::Red, Col::Default, false);
pub const C_LOW_PRI: Style = Style::new(Col::Green, Col::Default, false);
pub const C_RUN: Style = Style::new(Col::Green, Col::Default, false);
pub const C_DSTATE: Style = Style::new(Col::Red, Col::Default, true);

// ------------------------------------------------------------------ formats ---

/// Row_printKBytes (0x30b70): a size in KiB, 6 characters wide.
pub fn kbytes(out: &mut Vec<Seg>, n: Option<u64>, coloring: bool) {
    let Some(n) = n else {
        // Row_printKBytes 0x30b70: unknown size is "  N/A " (6 wide) in the shadow colour
        out.push(("  N/A ".into(), if coloring { C_SHADOW } else { C_DEFAULT }));
        return;
    };
    let colors = if coloring { [C_DEFAULT, C_MEGA, C_GIGA, C_LARGE] } else { [C_DEFAULT; 4] };
    if n < 1000 {
        out.push((format!("{:5} ", n), C_DEFAULT));
        return;
    }
    if n < 100000 {
        out.push((format!("{:2}", n / 1000), colors[1]));
        out.push((format!("{:03} ", n % 1000), C_DEFAULT));
        return;
    }
    // hundredths of MiB
    let mut v = n * 100 / 1024;
    let mut unit = 1usize;
    // colours: previous unit, this unit, next unit (Row_printKBytes 0x30b70)
    let mut prev = colors[0];
    let mut cur = colors[1];
    let next_of = |u: usize, cur: Style| if u + 1 < 4 { colors[u + 1] } else { cur };
    while v >= 1000000 {
        v >>= 10;
        unit += 1;
        prev = cur;
        cur = colors[unit.min(3)];
    }
    let next = next_of(unit, cur);
    let letter = b"KMGTPEZYRQ"[unit.min(9)] as char;
    let whole = v / 100;
    if v >= 10000 {
        if v < 100000 {
            out.push((format!("{:4}{} ", whole, letter), cur));
        } else {
            let th = whole / 1000;
            out.push((format!("{:1}", th), next));
            out.push((format!("{:03}{} ", whole - th * 1000, letter), cur));
        }
    } else if v < 1000 {
        out.push((format!("{:1}", whole), cur));
        out.push((format!(".{:02}", v % 100), prev));
        out.push((format!("{} ", letter), cur));
    } else {
        out.push((format!("{:2}", whole), cur));
        out.push((format!(".{:1}", (v % 100) / 10), prev));
        out.push((format!("{} ", letter), cur));
    }
}

/// Row_printTime (0x31250): centiseconds, 9 characters wide.
pub fn time(out: &mut Vec<Seg>, cs: u64, coloring: bool) {
    let (hc, dc, yc) = if coloring { (C_MEGA, C_GIGA, C_LARGE) } else { (C_DEFAULT, C_DEFAULT, C_DEFAULT) };
    let total_s = cs / 100;
    let s = total_s % 60;
    let m = (total_s / 60) % 60;
    let h_all = total_s / 3600;
    if cs < 360_000 {
        out.push((format!("{:2}:{:02}.{:02} ", total_s / 60, s, cs % 100), C_DEFAULT));
    } else if cs < 8_640_000 {
        out.push((format!("{:2}h", h_all), hc));
        out.push((format!("{:02}:{:02} ", m, s), C_DEFAULT));
    } else {
        let days = h_all / 24;
        let h = h_all % 24;
        if cs < 86_400_000 {
            out.push((format!("{:1}d", days), dc));
            out.push((format!("{:02}h", h), hc));
            out.push((format!("{:02}m ", m), C_DEFAULT));
        } else if total_s < 365 * 86400 {
            out.push((format!("{:4}d", days), dc));
            out.push((format!("{:02}h ", h), hc));
        } else {
            let years = days / 365;
            if years < 1000 {
                out.push((format!("{:3}y", years), yc));
                out.push((format!("{:03}d ", days % 365), dc));
            } else if years < 10_000_000 {
                out.push((format!("{:7}y ", years), yc));
            } else {
                out.push(("eternity ".into(), yc));
            }
        }
    }
}

/// Row_printPercentage (0x2e760).
pub fn percent(out: &mut Vec<Seg>, val: f32, width: usize) {
    if !(val >= 0.0) {
        out.push((format!("{:>w$} ", "N/A", w = width), C_SHADOW));
        return;
    }
    let mut st = C_DEFAULT;
    if val < 0.05 {
        st = C_SHADOW;
    } else if val >= 99.9 {
        st = C_MEGA;
    }
    if width == 4 && val > 99.9 {
        out.push((format!("{:>4.0} ", 100.0), st));
    } else {
        out.push((format!("{:>w$.1} ", val, w = width), st));
    }
}

/// Row_printRate (0x31650): bytes per second, 12 characters wide.
pub fn rate(out: &mut Vec<Seg>, r: f64, coloring: bool) {
    let (mc, lc) = if coloring { (C_MEGA, C_LARGE) } else { (C_DEFAULT, C_DEFAULT) };
    const K: f64 = 1024.0;
    if !(r >= 0.0) {
        out.push(("        N/A ".into(), C_SHADOW));
    } else if r < 0.005 {
        out.push((format!("{:7.2} B/s ", r), C_SHADOW));
    } else if r < K {
        out.push((format!("{:7.2} B/s ", r), C_DEFAULT));
    } else if r < K * K {
        out.push((format!("{:7.2} K/s ", r / K), C_DEFAULT));
    } else if r < K * K * K {
        out.push((format!("{:7.2} M/s ", r / K / K), mc));
    } else if r < K * K * K * K {
        out.push((format!("{:7.2} G/s ", r / K / K / K), lc));
    } else if r < K * K * K * K * K {
        out.push((format!("{:7.2} T/s ", r / K / K / K / K), lc));
    } else {
        out.push((format!("{:7.2} P/s ", r / K / K / K / K / K), lc));
    }
}

/// Meter_humanUnit (0x28690): a KiB value as "5.91G".
pub fn human_unit(kib: f64) -> String {
    let mut v = kib;
    let mut i = 0usize;
    while v >= 1024.0 && i < 9 {
        v /= 1024.0;
        i += 1;
    }
    let letter = b"KMGTPEZYRQ"[i] as char;
    if i == 0 {
        return format!("{:.0}{}", v, letter);
    }
    let prec = if v > 99.9 { 0 } else if v > 9.99 { 1 } else { 2 };
    format!("{:.*}{}", prec, v, letter)
}

pub struct RowCtx<'a> {
    pub pid_w: usize,
    pub highlight_mb: bool,
    pub highlight_threads: bool,
    pub show_path: bool,
    pub tree_prefix: &'a str,
    pub cpu_count: usize,
    pub now_secs: u64,
}

pub fn header_title(f: Field, pid_w: usize) -> String {
    match f {
        Field::Pid | Field::Ppid | Field::Tgid => format!("{:>w$} ", info(f).title, w = pid_w),
        Field::Command => "Command ".into(),
        _ => format!("{} ", info(f).title),
    }
}

fn start_str(epoch: u64) -> String {
    // Hours:minutes in local time is not portable without a time zone library; use UTC
    // for the clock and keep the original's 5-character "HH:MM" shape.
    let secs = epoch % 86400;
    format!("{:02}:{:02}", secs / 3600, (secs / 60) % 60)
}

pub fn write_field(out: &mut Vec<Seg>, f: Field, p: &Proc, c: &RowCtx) {
    match f {
        Field::Pid => out.push((format!("{:>w$} ", p.pid, w = c.pid_w), C_DEFAULT)),
        Field::Ppid => out.push((format!("{:>w$} ", p.ppid, w = c.pid_w), C_DEFAULT)),
        // Process_writeField 0x2c3e0: TGID is grey when it equals the PID
        Field::Tgid => out.push((format!("{:>w$} ", p.tgid, w = c.pid_w), if p.tgid == p.pid { C_SHADOW } else { C_DEFAULT })),
        Field::User => {
            let st = if p.is_mine { C_DEFAULT } else { C_SHADOW };
            let name: String = p.user.chars().take(10).collect();
            out.push((format!("{:<10} ", name), st));
        }
        Field::StUid => out.push((format!("{:>5} ", p.uid.chars().take(5).collect::<String>()), C_DEFAULT)),
        Field::Priority => match p.pri {
            Some(v) if v <= -100 => out.push((" RT ".into(), C_DEFAULT)),
            Some(v) => out.push((format!("{:3} ", v), C_DEFAULT)),
            None => out.push(("  - ".into(), C_SHADOW)),
        },
        Field::Nice => match p.nice {
            Some(v) => {
                let st = if v < 0 { C_HIGH_PRI } else if v > 0 { C_LOW_PRI } else { C_SHADOW };
                out.push((format!("{:3} ", v), st));
            }
            None => out.push(("  - ".into(), C_SHADOW)),
        },
        Field::MVirt => kbytes(out, Some(p.virt_kb), c.highlight_mb),
        Field::MResident => kbytes(out, Some(p.res_kb), c.highlight_mb),
        Field::MShare => kbytes(out, p.shr_kb, c.highlight_mb),
        Field::State => {
            // Process_writeField 0x2c3e0, state letters "?URQWDBPTtZXIS" (0x4db18):
            // runnable/running green; disk wait, blocked, paging, traced, zombie, defunct bold red;
            // queued, waiting, idle, sleeping grey; stopped and unknown default.
            let st = match p.state {
                'U' | 'R' => C_RUN,
                'D' | 'B' | 'P' | 't' | 'Z' | 'X' => C_DSTATE,
                'Q' | 'W' | 'I' | 'S' => C_SHADOW,
                _ => C_DEFAULT,
            };
            out.push((format!("{} ", p.state), st));
        }
        Field::PercentCpu => percent(out, p.cpu, 5),
        Field::PercentNormCpu => percent(out, p.cpu / c.cpu_count as f32, 5),
        Field::PercentMem => percent(out, p.mem, 4),
        Field::Time => time(out, p.time_cs, c.highlight_mb),
        Field::Elapsed => time(out, c.now_secs.saturating_sub(p.start_time) * 100, c.highlight_mb),
        Field::StartTime => out.push((format!("{} ", start_str(p.start_time)), C_DEFAULT)),
        Field::Nlwp => out.push((format!("{:4} ", p.nlwp), if p.nlwp == 1 { C_SHADOW } else { C_DEFAULT })),
        Field::Processor => match p.processor {
            Some(v) => out.push((format!("{:3} ", v), C_DEFAULT)),
            None => out.push(("  - ".into(), C_SHADOW)),
        },
        Field::RBytes => kbytes(out, Some(p.rbytes / 1024), c.highlight_mb),
        Field::WBytes => kbytes(out, Some(p.wbytes / 1024), c.highlight_mb),
        Field::IoReadRate => rate(out, p.read_rate, c.highlight_mb),
        Field::IoWriteRate => rate(out, p.write_rate, c.highlight_mb),
        Field::IoRate => rate(out, p.read_rate + p.write_rate, c.highlight_mb),
        Field::IoPriority => match p.nice {
            // No explicit class set: best effort, level derived from nice.
            Some(n) => out.push((format!("B{} ", ((n + 20) / 5).clamp(0, 7)), C_DEFAULT)),
            None => out.push(("?? ".into(), C_SHADOW)),
        },
        Field::PercentSwapDelay | Field::PercentIoDelay => percent(out, -1.0, 5),
        Field::Comm => out.push((format!("{:<15} ", p.comm.chars().take(15).collect::<String>()), C_DEFAULT)),
        Field::Exe => match p.exe.as_deref().map(basename) {
            Some(e) => out.push((format!("{:<15} ", e.chars().take(15).collect::<String>()), C_DEFAULT)),
            // no program file: "KTHREAD" for kernel threads, else "N/A", grey
            None => out.push((format!("{:<15} ", if p.is_kthread { "KTHREAD" } else { "N/A" }), C_SHADOW)),
        },
        Field::Command => {
            if !c.tree_prefix.is_empty() {
                out.push((c.tree_prefix.to_string(), C_TREE));
            }
            // threads (userland and kernel) get the thread colour when highlight_threads is on
            let st = if (p.is_thread || p.is_kthread) && c.highlight_threads { C_THREAD } else { C_DEFAULT };
            out.push((command_text(p, c.show_path), st));
        }
    }
}

pub fn basename(s: &str) -> &str {
    s.rsplit(['/', '\\']).next().unwrap_or(s)
}

pub fn command_text(p: &Proc, show_path: bool) -> String {
    if p.cmdline.is_empty() || p.is_kthread {
        return p.comm.clone();
    }
    let mut parts = p.cmdline.clone();
    if !show_path {
        let b = basename(&parts[0]).to_string();
        parts[0] = b;
    }
    parts.join(" ")
}
