//! Screen layout, drawing and keys.

use crate::cli::{Opts, NAME, VERSION};
use crate::data::{self, Proc, Sampler};
use crate::fields::{self, Field, RowCtx, Seg};
use crate::screen::{Buf, Col, Style};
use crate::settings::Settings;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const S_DEFAULT: Style = Style::new(Col::Default, Col::Default, false);
const S_BOLD: Style = Style::new(Col::Default, Col::Default, true);
const S_SHADOW: Style = Style::new(Col::Gray, Col::Default, true);
const S_METER_CAPTION: Style = Style::new(Col::Cyan, Col::Default, false);
const S_METER_VALUE: Style = Style::new(Col::Cyan, Col::Default, true);
const S_GREEN_BOLD: Style = Style::new(Col::Green, Col::Default, true);
const S_HEADER: Style = Style::new(Col::Black, Col::Green, false);
const S_HEADER_SORT: Style = Style::new(Col::Black, Col::Cyan, false);
const S_SELECT: Style = Style::new(Col::Black, Col::Cyan, false);
const S_SELECT_UNFOCUS: Style = Style::new(Col::Black, Col::White, false);
const S_FKEY: Style = Style::new(Col::Default, Col::Default, false);
const S_FLABEL: Style = Style::new(Col::Black, Col::Cyan, false);
const S_TAB_ON: Style = Style::new(Col::Black, Col::Green, false);
const S_TAB_ON_BR: Style = Style::new(Col::Green, Col::Green, false);
const S_TAB_OFF: Style = Style::new(Col::Black, Col::Blue, false);
const S_TAB_OFF_BR: Style = Style::new(Col::Blue, Col::Blue, false);
const S_TAG: Style = Style::new(Col::Yellow, Col::Default, true);
const S_NEW: Style = Style::new(Col::Black, Col::Green, false);
const S_TOMB: Style = Style::new(Col::Black, Col::Red, false);
const S_FAILED_SEARCH: Style = Style::new(Col::White, Col::Red, true);
const S_PAUSED: Style = Style::new(Col::Black, Col::Yellow, true);

const C_NICE: Style = Style::new(Col::Blue, Col::Default, false);
const C_USER: Style = Style::new(Col::Green, Col::Default, false);
const C_KERNEL: Style = Style::new(Col::Red, Col::Default, false);
const C_VIRT: Style = Style::new(Col::Cyan, Col::Default, false);
const C_MEM_USED: Style = Style::new(Col::Green, Col::Default, false);
const C_MEM_SHARED: Style = Style::new(Col::Magenta, Col::Default, false);
const C_MEM_BUFFERS: Style = Style::new(Col::Blue, Col::Default, true);
const C_MEM_CACHE: Style = Style::new(Col::Yellow, Col::Default, false);
const C_MEM_COMPRESSED: Style = Style::new(Col::Gray, Col::Default, true);
const C_SWAP: Style = Style::new(Col::Red, Col::Default, false);

#[derive(Clone, Copy, PartialEq)]
enum RowKind {
    Normal,
    New,
    Tomb,
}

#[derive(Clone)]
struct Row {
    p: Proc,
    prefix: String,
    kind: RowKind,
    #[allow(dead_code)]
    has_children: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum MenuKind {
    Sort,
    Kill,
    User,
}

struct Menu {
    kind: MenuKind,
    title: String,
    items: Vec<String>,
    sel: usize,
    scroll: usize,
}

enum Mode {
    Normal,
    Search { text: String, failed: bool },
    Filter { text: String },
    Menu(Menu),
    Help,
    Setup { sel: usize },
    Info { title: String, lines: Vec<String>, scroll: usize },
}

const SETUP_TOGGLES: usize = 13;

pub struct App {
    pub s: Settings,
    opts: Opts,
    pub data: Sampler,
    tab: usize,
    rows: Vec<Row>,
    sel: usize,
    scroll: usize,
    hscroll: usize,
    follow: Option<u32>,
    tagged: HashSet<u32>,
    collapsed: HashSet<u32>,
    mode: Mode,
    filter: String,
    user_filter: Option<String>,
    pids: Option<HashSet<u32>>,
    pub paused: bool,
    hide_meters: bool,
    all_collapsed: bool,
    first_seen: HashMap<u32, Instant>,
    tombs: HashMap<u32, (Proc, Instant)>,
    last_procs: HashMap<u32, Proc>,
    scans: u64,
    pub quit: bool,
    list_y0: usize,
    list_h: usize,
    list_x0: usize,
    header_y: usize,
    tabs_y: Option<usize>,
    tab_hits: Vec<(usize, usize, usize)>,
    col_hits: Vec<(usize, usize, Field)>,
    fbar_y: usize,
    fbar_hits: Vec<(usize, usize, KeyCode)>,
    pid_search: (String, Instant),
    signals: Vec<(String, sysinfo::Signal)>,
}

fn signal_list() -> Vec<(String, sysinfo::Signal)> {
    use sysinfo::Signal as S;
    #[cfg(unix)]
    {
        let mut l: Vec<(i32, &str, S)> = vec![
            (libc::SIGHUP, "SIGHUP", S::Hangup),
            (libc::SIGINT, "SIGINT", S::Interrupt),
            (libc::SIGQUIT, "SIGQUIT", S::Quit),
            (libc::SIGILL, "SIGILL", S::Illegal),
            (libc::SIGTRAP, "SIGTRAP", S::Trap),
            (libc::SIGABRT, "SIGABRT", S::Abort),
            (libc::SIGBUS, "SIGBUS", S::Bus),
            (libc::SIGFPE, "SIGFPE", S::FloatingPointException),
            (libc::SIGKILL, "SIGKILL", S::Kill),
            (libc::SIGUSR1, "SIGUSR1", S::User1),
            (libc::SIGSEGV, "SIGSEGV", S::Segv),
            (libc::SIGUSR2, "SIGUSR2", S::User2),
            (libc::SIGPIPE, "SIGPIPE", S::Pipe),
            (libc::SIGALRM, "SIGALRM", S::Alarm),
            (libc::SIGTERM, "SIGTERM", S::Term),
            (libc::SIGCHLD, "SIGCHLD", S::Child),
            (libc::SIGCONT, "SIGCONT", S::Continue),
            (libc::SIGSTOP, "SIGSTOP", S::Stop),
            (libc::SIGTSTP, "SIGTSTP", S::TSTP),
        ];
        l.sort_by_key(|(n, _, _)| *n);
        l.into_iter().map(|(n, name, s)| (format!("{:2} {}", n, name), s)).collect()
    }
    #[cfg(not(unix))]
    {
        vec![(" 9 Terminate".to_string(), S::Kill)]
    }
}

impl App {
    pub fn new(s: Settings, opts: Opts, data: Sampler) -> App {
        let mut s = s;
        if let Some(d) = opts.delay {
            s.delay = d;
        }
        if opts.no_color {
            s.monochrome = true;
        }
        if opts.no_mouse {
            s.enable_mouse = false;
        }
        if opts.tree {
            s.screens[0].tree = true;
        }
        if let Some(k) = opts.sort_key {
            let sc = &mut s.screens[0];
            let d = if fields::info(k).sort_desc { -1 } else { 1 };
            sc.sort_key = k;
            sc.sort_dir = d;
            if sc.tree {
                sc.tree_sort_key = k;
                sc.tree_sort_dir = d;
            }
        }
        s.changed = false;
        App {
            filter: opts.filter.clone().unwrap_or_default(),
            user_filter: opts.user.clone(),
            pids: opts.pids.as_ref().map(|v| v.iter().copied().collect()),
            s,
            opts,
            data,
            tab: 0,
            rows: Vec::new(),
            sel: 0,
            scroll: 0,
            hscroll: 0,
            follow: None,
            tagged: HashSet::new(),
            collapsed: HashSet::new(),
            mode: Mode::Normal,
            paused: false,
            hide_meters: false,
            all_collapsed: false,
            first_seen: HashMap::new(),
            tombs: HashMap::new(),
            last_procs: HashMap::new(),
            scans: 0,
            quit: false,
            list_y0: 0,
            list_h: 0,
            list_x0: 0,
            header_y: 0,
            tabs_y: None,
            tab_hits: Vec::new(),
            col_hits: Vec::new(),
            fbar_y: 0,
            fbar_hits: Vec::new(),
            pid_search: (String::new(), Instant::now()),
            signals: signal_list(),
        }
    }

    pub fn delay(&self) -> Duration {
        Duration::from_millis(self.s.delay as u64 * 100)
    }

    /// Takes a new sample and remembers which processes appeared or went away.
    pub fn scan(&mut self) {
        self.data.scan();
        self.scans += 1;
        let now = Instant::now();
        let mut current: HashMap<u32, Proc> = HashMap::new();
        for p in &self.data.procs {
            if self.scans > 2 {
                self.first_seen.entry(p.pid).or_insert(now);
            }
            current.insert(p.pid, p.clone());
        }
        if self.opts.highlight.is_some() {
            for (pid, p) in self.last_procs.drain() {
                if !current.contains_key(&pid) {
                    self.tombs.insert(pid, (p, now));
                }
            }
            let keep = Duration::from_secs(self.opts.highlight.unwrap_or(5) as u64);
            self.tombs.retain(|_, (_, t)| now.duration_since(*t) < keep);
            self.first_seen.retain(|pid, t| current.contains_key(pid) && now.duration_since(*t) < keep);
        }
        self.last_procs = current;
        let alive = &self.last_procs;
        self.tagged.retain(|pid| alive.contains_key(pid));
    }

    fn screen(&self) -> &crate::settings::Screen {
        &self.s.screens[self.tab.min(self.s.screens.len() - 1)]
    }

    fn screen_mut(&mut self) -> &mut crate::settings::Screen {
        let i = self.tab.min(self.s.screens.len() - 1);
        &mut self.s.screens[i]
    }

    fn active_sort(&self) -> (Field, i32) {
        let sc = self.screen();
        if sc.tree { (sc.tree_sort_key, sc.tree_sort_dir) } else { (sc.sort_key, sc.sort_dir) }
    }

    fn set_sort(&mut self, key: Field) {
        let tree = self.screen().tree;
        let sc = self.screen_mut();
        let (k, d) = if tree { (&mut sc.tree_sort_key, &mut sc.tree_sort_dir) } else { (&mut sc.sort_key, &mut sc.sort_dir) };
        if *k == key {
            *d = -*d;
        } else {
            *k = key;
            *d = if fields::info(key).sort_desc { -1 } else { 1 };
        }
        self.s.changed = true;
    }

    fn invert_sort(&mut self) {
        let tree = self.screen().tree;
        let sc = self.screen_mut();
        if tree {
            sc.tree_sort_dir = -sc.tree_sort_dir
        } else {
            sc.sort_dir = -sc.sort_dir
        }
        self.s.changed = true;
    }

    fn visible(&self, p: &Proc) -> bool {
        if p.is_kthread && self.s.hide_kernel_threads {
            return false;
        }
        if p.is_thread && !p.is_kthread && self.s.hide_userland_threads {
            return false;
        }
        if let Some(u) = &self.user_filter {
            if &p.user != u && &p.uid != u {
                return false;
            }
        }
        if let Some(ps) = &self.pids {
            if !ps.contains(&p.pid) && !ps.contains(&p.tgid) {
                return false;
            }
        }
        if !self.filter.is_empty() {
            let t = fields::command_text(p, true).to_lowercase();
            if !t.contains(&self.filter.to_lowercase()) {
                return false;
            }
        }
        true
    }

    fn compare(&self, a: &Proc, b: &Proc, key: Field, dir: i32) -> Ordering {
        let fcmp = |x: f64, y: f64| x.partial_cmp(&y).unwrap_or(Ordering::Equal);
        let o = match key {
            Field::Pid => a.pid.cmp(&b.pid),
            Field::Ppid => a.ppid.cmp(&b.ppid),
            Field::Tgid => a.tgid.cmp(&b.tgid),
            Field::User => a.user.to_lowercase().cmp(&b.user.to_lowercase()),
            Field::StUid => a.uid.cmp(&b.uid),
            Field::Priority => a.pri.cmp(&b.pri),
            Field::Nice => a.nice.cmp(&b.nice),
            Field::MVirt => a.virt_kb.cmp(&b.virt_kb),
            Field::MResident => a.res_kb.cmp(&b.res_kb),
            Field::MShare => a.shr_kb.cmp(&b.shr_kb),
            Field::State => a.state.cmp(&b.state),
            Field::PercentCpu | Field::PercentNormCpu => fcmp(a.cpu as f64, b.cpu as f64),
            Field::PercentMem => a.res_kb.cmp(&b.res_kb),
            Field::Time => a.time_cs.cmp(&b.time_cs),
            Field::Elapsed => b.start_time.cmp(&a.start_time),
            Field::StartTime => a.start_time.cmp(&b.start_time),
            Field::Nlwp => a.nlwp.cmp(&b.nlwp),
            Field::Processor => a.processor.cmp(&b.processor),
            Field::RBytes => a.rbytes.cmp(&b.rbytes),
            Field::WBytes => a.wbytes.cmp(&b.wbytes),
            Field::IoReadRate => fcmp(a.read_rate, b.read_rate),
            Field::IoWriteRate => fcmp(a.write_rate, b.write_rate),
            Field::IoRate => fcmp(a.read_rate + a.write_rate, b.read_rate + b.write_rate),
            Field::IoPriority => a.nice.cmp(&b.nice),
            Field::PercentSwapDelay | Field::PercentIoDelay => Ordering::Equal,
            Field::Comm => a.comm.to_lowercase().cmp(&b.comm.to_lowercase()),
            Field::Exe => a.exe.cmp(&b.exe),
            Field::Command => fields::command_text(a, true).cmp(&fields::command_text(b, true)),
        };
        let o = if dir < 0 { o.reverse() } else { o };
        o.then(a.pid.cmp(&b.pid))
    }

    pub fn rebuild(&mut self) {
        let now = Instant::now();
        let keep = Duration::from_secs(self.opts.highlight.unwrap_or(5) as u64);
        let mut list: Vec<(Proc, RowKind)> = Vec::new();
        for p in &self.data.procs {
            if !self.visible(p) {
                continue;
            }
            let kind = match (self.opts.highlight, self.first_seen.get(&p.pid)) {
                (Some(_), Some(t)) if now.duration_since(*t) < keep => RowKind::New,
                _ => RowKind::Normal,
            };
            list.push((p.clone(), kind));
        }
        if self.opts.highlight.is_some() {
            for (p, _) in self.tombs.values() {
                if self.visible(p) {
                    list.push((p.clone(), RowKind::Tomb));
                }
            }
        }
        let (key, dir) = self.active_sort();
        let ascii = self.opts.no_unicode;
        let mut rows = Vec::with_capacity(list.len());
        if self.screen().tree {
            let pids: HashSet<u32> = list.iter().map(|(p, _)| p.pid).collect();
            let mut kids: HashMap<u32, Vec<usize>> = HashMap::new();
            let mut roots: Vec<usize> = Vec::new();
            for (i, (p, _)) in list.iter().enumerate() {
                if p.ppid != p.pid && pids.contains(&p.ppid) {
                    kids.entry(p.ppid).or_default().push(i);
                } else {
                    roots.push(i);
                }
            }
            roots.sort_by(|&a, &b| self.compare(&list[a].0, &list[b].0, key, dir));
            for v in kids.values_mut() {
                v.sort_by(|&a, &b| self.compare(&list[a].0, &list[b].0, key, dir));
            }
            // depth-first walk; `open` says, for each level, whether more siblings follow
            let mut stack: Vec<(usize, usize, Vec<bool>)> = roots.iter().rev().map(|&r| (r, 0usize, Vec::new())).collect();
            let mut seen: HashSet<usize> = HashSet::new();
            while let Some((i, depth, open)) = stack.pop() {
                if !seen.insert(i) {
                    continue;
                }
                let (p, kind) = &list[i];
                let children = kids.get(&p.pid).cloned().unwrap_or_default();
                let is_collapsed = self.collapsed.contains(&p.pid);
                let mut prefix = String::new();
                if depth > 0 {
                    for lvl in open.iter().take(depth - 1) {
                        prefix.push_str(if *lvl {
                            if ascii { "|  " } else { "│  " }
                        } else {
                            "   "
                        });
                    }
                    let last = !open[depth - 1];
                    let bend = match (ascii, last) {
                        (false, false) => '├',
                        (false, true) => '└',
                        (true, false) => '|',
                        (true, true) => '`',
                    };
                    let horiz = if is_collapsed && !children.is_empty() {
                        '+'
                    } else if ascii {
                        '-'
                    } else {
                        '─'
                    };
                    prefix.push(bend);
                    prefix.push(horiz);
                    prefix.push(' ');
                } else if is_collapsed && !children.is_empty() {
                    prefix.push_str("+ ");
                }
                rows.push(Row { p: p.clone(), prefix, kind: *kind, has_children: !children.is_empty() });
                if !is_collapsed {
                    for (n, &c) in children.iter().enumerate().rev() {
                        let mut o = open.clone();
                        o.push(n + 1 < children.len());
                        stack.push((c, depth + 1, o));
                    }
                }
            }
        } else {
            list.sort_by(|a, b| self.compare(&a.0, &b.0, key, dir));
            for (p, kind) in list {
                rows.push(Row { p, prefix: String::new(), kind, has_children: false });
            }
        }
        self.rows = rows;
        if let Some(f) = self.follow {
            if let Some(i) = self.rows.iter().position(|r| r.p.pid == f) {
                self.sel = i;
            } else {
                self.follow = None;
            }
        }
        if self.sel >= self.rows.len() {
            self.sel = self.rows.len().saturating_sub(1);
        }
    }

    // ------------------------------------------------------------- drawing ---

    pub fn draw(&mut self, w: usize, h: usize) -> Buf {
        let mut b = Buf::new(w, h);
        if w == 0 || h == 0 {
            return b;
        }
        match &self.mode {
            Mode::Help => {
                self.draw_help(&mut b);
                return b;
            }
            Mode::Setup { sel } => {
                let sel = *sel;
                self.draw_setup(&mut b, sel);
                return b;
            }
            Mode::Info { .. } => {
                self.draw_info(&mut b);
                return b;
            }
            _ => {}
        }
        let mut y = 0;
        if !self.hide_meters {
            y = self.draw_header(&mut b, w);
        }
        self.tabs_y = None;
        self.tab_hits.clear();
        if self.s.screen_tabs && y < h {
            self.tabs_y = Some(y);
            let mut x = 2;
            for (i, sc) in self.s.screens.iter().enumerate() {
                let (br, tx) = if i == self.tab { (S_TAB_ON_BR, S_TAB_ON) } else { (S_TAB_OFF_BR, S_TAB_OFF) };
                let x0 = x;
                x = b.put(x, y, "[", br);
                x = b.put(x, y, &sc.name, tx);
                x = b.put(x, y, "]", br);
                self.tab_hits.push((x0, x, i));
                x += 1;
            }
            y += 1;
        }
        let show_fbar = !self.s.hide_function_bar || !matches!(self.mode, Mode::Normal);
        let bottom = if show_fbar { h.saturating_sub(1) } else { h };
        // side menu
        let mut x0 = 0;
        if let Mode::Menu(m) = &self.mode {
            let longest = m.items.iter().map(|s| s.chars().count()).max().unwrap_or(10).max(m.title.chars().count());
            let mw = (longest + 2).max(14).min(w / 2);
            if y < bottom {
                b.fill(0, y, mw, S_HEADER);
                b.put_clip(0, y, &m.title, S_HEADER, mw);
                let lh = bottom.saturating_sub(y + 1);
                for (row, idx) in (m.scroll..m.items.len()).enumerate().take(lh) {
                    let yy = y + 1 + row;
                    let st = if idx == m.sel { S_SELECT } else { S_DEFAULT };
                    if idx == m.sel {
                        b.fill(0, yy, mw, st);
                    }
                    b.put_clip(0, yy, &m.items[idx], st, mw);
                }
            }
            x0 = mw + 1;
        }
        self.list_x0 = x0;
        self.header_y = y;
        if y < bottom {
            self.draw_list_header(&mut b, x0, y, w);
            y += 1;
        }
        self.list_y0 = y;
        self.list_h = bottom.saturating_sub(y);
        self.ensure_visible();
        let focus = !matches!(self.mode, Mode::Menu(_));
        let cols = self.screen().columns.clone();
        let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        for r in 0..self.list_h {
            let idx = self.scroll + r;
            let Some(row) = self.rows.get(idx) else { break };
            let ctx = RowCtx {
                pid_w: self.data.pid_width,
                highlight_mb: self.s.highlight_megabytes,
                highlight_threads: self.s.highlight_threads,
                show_path: self.s.show_program_path,
                tree_prefix: &row.prefix,
                cpu_count: self.data.cpu_count(),
                now_secs,
            };
            let mut segs: Vec<Seg> = Vec::new();
            for f in &cols {
                fields::write_field(&mut segs, *f, &row.p, &ctx);
            }
            let yy = y + r;
            let selected = idx == self.sel;
            let tagged = self.tagged.contains(&row.p.pid);
            let over: Option<Style> = if selected {
                Some(if focus { S_SELECT } else { S_SELECT_UNFOCUS })
            } else if row.kind == RowKind::New {
                Some(S_NEW)
            } else if row.kind == RowKind::Tomb {
                Some(S_TOMB)
            } else if tagged {
                Some(S_TAG)
            } else if self.s.shadow_other_users && !row.p.is_mine {
                Some(S_SHADOW)
            } else {
                None
            };
            if let Some(o) = over {
                if o.bg != Col::Default {
                    b.fill(x0, yy, w, o);
                }
            }
            let mut col = 0usize;
            for (text, st) in segs {
                let st = match over {
                    Some(o) if selected && tagged => Style::new(Col::Yellow, o.bg, true),
                    Some(o) => o,
                    None => st,
                };
                for ch in text.chars() {
                    if col >= self.hscroll {
                        let x = x0 + col - self.hscroll;
                        if x < w {
                            b.set(x, yy, ch, st);
                        }
                    }
                    col += 1;
                }
            }
        }
        if show_fbar {
            self.draw_function_bar(&mut b, h - 1, w);
        }
        b
    }

    fn ensure_visible(&mut self) {
        let lh = self.list_h.max(1);
        if self.sel < self.scroll {
            self.scroll = self.sel;
        }
        if self.sel >= self.scroll + lh {
            self.scroll = self.sel + 1 - lh;
        }
        let max_scroll = self.rows.len().saturating_sub(lh);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    fn draw_list_header(&mut self, b: &mut Buf, x0: usize, y: usize, w: usize) {
        b.fill(x0, y, w, S_HEADER);
        self.col_hits.clear();
        let (key, dir) = self.active_sort();
        let mut col = 0usize;
        let cols = self.screen().columns.clone();
        for f in cols {
            let mut t = fields::header_title(f, self.data.pid_width);
            let st = if f == key { S_HEADER_SORT } else { S_HEADER };
            if f == key {
                t.pop();
                let arrow = match (self.opts.no_unicode, dir > 0) {
                    (true, true) => '^',
                    (true, false) => 'v',
                    (false, true) => '△',
                    (false, false) => '▽',
                };
                t.push(arrow);
            }
            let start = col;
            for ch in t.chars() {
                if col >= self.hscroll {
                    b.set(x0 + col - self.hscroll, y, ch, st);
                }
                col += 1;
            }
            let end = if f == Field::Command { w + self.hscroll } else { col };
            self.col_hits.push((x0 + start.saturating_sub(self.hscroll), x0 + end.saturating_sub(self.hscroll), f));
        }
    }

    fn draw_function_bar(&mut self, b: &mut Buf, y: usize, w: usize) {
        self.fbar_y = y;
        self.fbar_hits.clear();
        let ro = self.opts.readonly;
        let blank = "      ".to_string();
        let items: Vec<(&str, String, KeyCode)> = match &self.mode {
            Mode::Search { .. } => vec![
                ("F3", "Next  ".into(), KeyCode::F(3)),
                ("S-F3", "Prev   ".into(), KeyCode::F(13)),
                ("Esc", "Cancel ".into(), KeyCode::Esc),
                ("  ", " Search: ".into(), KeyCode::Null),
            ],
            Mode::Filter { .. } => vec![
                ("Enter", "Done  ".into(), KeyCode::Enter),
                ("Esc", "Clear ".into(), KeyCode::Esc),
                ("  ", " Filter: ".into(), KeyCode::Null),
            ],
            Mode::Menu(m) => {
                let verb = match m.kind {
                    MenuKind::Sort => "Sort   ",
                    MenuKind::Kill => "Send   ",
                    MenuKind::User => "Show   ",
                };
                vec![("Enter", verb.into(), KeyCode::Enter), ("Esc", "Cancel ".into(), KeyCode::Esc)]
            }
            _ => {
                let tree = self.screen().tree;
                vec![
                    ("F1", "Help  ".into(), KeyCode::F(1)),
                    ("F2", "Setup ".into(), KeyCode::F(2)),
                    ("F3", "Search".into(), KeyCode::F(3)),
                    ("F4", if self.filter.is_empty() { "Filter".into() } else { "FILTER".into() }, KeyCode::F(4)),
                    ("F5", if tree { "List  ".into() } else { "Tree  ".into() }, KeyCode::F(5)),
                    ("F6", "SortBy".into(), KeyCode::F(6)),
                    ("F7", if ro { blank.clone() } else { "Nice -".into() }, KeyCode::F(7)),
                    ("F8", if ro { blank.clone() } else { "Nice +".into() }, KeyCode::F(8)),
                    ("F9", if ro { blank.clone() } else { "Kill  ".into() }, KeyCode::F(9)),
                    ("F10", "Quit".into(), KeyCode::F(10)),
                ]
            }
        };
        let mut x = 0;
        for (k, label, code) in &items {
            let x_start = x;
            x = b.put(x, y, k, S_FKEY);
            x = b.put(x, y, label, S_FLABEL);
            if *code != KeyCode::Null {
                self.fbar_hits.push((x_start, x, *code));
            }
        }
        let fill_st = if matches!(self.mode, Mode::Normal) { S_FLABEL } else { S_DEFAULT };
        b.fill(x, y, w, fill_st);
        match &self.mode {
            Mode::Search { text, failed } => {
                let st = if *failed { S_FAILED_SEARCH } else { S_DEFAULT };
                b.put(x, y, text, st);
            }
            Mode::Filter { text } => {
                b.put(x, y, text, S_DEFAULT);
            }
            _ => {}
        }
        if self.paused && matches!(self.mode, Mode::Normal) {
            let t = " PAUSED ";
            let px = w.saturating_sub(t.len());
            b.put(px, y, t, S_PAUSED);
        }
    }

    /// Draws the meters; returns the first free row.
    fn draw_header(&self, b: &mut Buf, w: usize) -> usize {
        let pad = if self.s.header_margin { 2 } else { 0 };
        let top = if self.s.header_margin { 1 } else { 0 };
        let avail = w.saturating_sub(2 * pad + 1);
        let lw = avail / 2;
        let rw = avail - lw;
        let lx = pad;
        let rx = pad + lw + 1;
        let n = self.data.cpu_count();
        let base = if self.s.cpu_count_from_one { 1 } else { 0 };
        // which CPUs go left/right, how many per row (sub_32480)
        let (left_cpus, right_cpus, per_row): (Vec<usize>, Vec<usize>, usize) = if n <= 4 {
            ((0..n).collect(), vec![], 1)
        } else if n > 128 {
            (vec![usize::MAX], vec![], 1)
        } else {
            let half = n.div_ceil(2);
            let k = if n <= 8 {
                1
            } else if n <= 16 {
                2
            } else if n <= 32 {
                4
            } else {
                8
            };
            ((0..half).collect(), (half..n).collect(), k)
        };
        let mut ly = top;
        let mut ry = top;
        let mono = self.s.monochrome;
        let draw_cpus = |b: &mut Buf, list: &[usize], x: usize, cw: usize, y: &mut usize| {
            for chunk in list.chunks(per_row) {
                let sub = if per_row == 1 { cw } else { (cw + 1) / per_row - 1 };
                for (j, &c) in chunk.iter().enumerate() {
                    let sx = x + j * (sub + 1);
                    let (sample, cap) = if c == usize::MAX {
                        (self.data.avg_cpu, "Avg".to_string())
                    } else {
                        (self.data.cpus.get(c).copied().unwrap_or_default(), format!("{:>3}", c + base))
                    };
                    let segs = [(sample.nice, C_NICE), (sample.user, C_USER), (sample.kernel, C_KERNEL), (sample.virt, C_VIRT)];
                    let text = format!("{:5.1}%", sample.total().min(100.0));
                    bar_meter(b, sx, *y, sub, &cap, &segs, 100.0, &text, mono);
                }
                *y += 1;
            }
        };
        draw_cpus(b, &left_cpus, lx, lw, &mut ly);
        draw_cpus(b, &right_cpus, rx, rw, &mut ry);
        // memory and swap on the left
        let m = self.data.mem;
        // MemoryMeter_updateValues 0x293a0: text = used + shared + compressed / total
        let used_txt = (m.used_kb + m.shared_kb + m.compressed_kb) as f64;
        let mt = format!("{}/{}", fields::human_unit(used_txt), fields::human_unit(m.total_kb as f64));
        let mem_segs = [
            (m.used_kb as f64, C_MEM_USED),
            (m.shared_kb as f64, C_MEM_SHARED),
            (m.compressed_kb as f64, C_MEM_COMPRESSED),
            (m.buffers_kb as f64, C_MEM_BUFFERS),
            (m.cache_kb as f64, C_MEM_CACHE),
        ];
        bar_meter(b, lx, ly, lw, "Mem", &mem_segs, m.total_kb as f64, &mt, mono);
        ly += 1;
        let st = format!("{}/{}", fields::human_unit(m.swap_used_kb as f64), fields::human_unit(m.swap_total_kb as f64));
        let swap_segs = [(m.swap_used_kb as f64, C_SWAP), (m.swap_cache_kb as f64, C_MEM_CACHE), (m.swap_compressed_kb as f64, C_MEM_COMPRESSED)];
        bar_meter(b, lx, ly, lw, "Swp", &swap_segs, m.swap_total_kb as f64, &st, mono);
        ly += 1;
        // text meters on the right
        let tx = rx;
        let lim = rx + rw;
        let mut x = b.put_clip(tx, ry, "Tasks: ", S_METER_CAPTION, lim);
        let thr_st = if self.s.hide_userland_threads { S_SHADOW } else { S_GREEN_BOLD };
        let thr_lbl = if self.s.hide_userland_threads { S_SHADOW } else { S_METER_CAPTION };
        let k_st = if self.s.hide_kernel_threads { S_SHADOW } else { S_GREEN_BOLD };
        let k_lbl = if self.s.hide_kernel_threads { S_SHADOW } else { S_METER_CAPTION };
        x = b.put_clip(x, ry, &self.data.tasks.to_string(), S_METER_VALUE, lim);
        x = b.put_clip(x, ry, ", ", thr_lbl, lim);
        x = b.put_clip(x, ry, &self.data.threads.to_string(), thr_st, lim);
        x = b.put_clip(x, ry, " thr", thr_lbl, lim);
        x = b.put_clip(x, ry, ", ", k_lbl, lim);
        x = b.put_clip(x, ry, &self.data.kthreads.to_string(), k_st, lim);
        x = b.put_clip(x, ry, " kthr", k_lbl, lim);
        x = b.put_clip(x, ry, "; ", S_METER_CAPTION, lim);
        x = b.put_clip(x, ry, &self.data.running.to_string(), S_GREEN_BOLD, lim);
        b.put_clip(x, ry, " running", S_METER_CAPTION, lim);
        ry += 1;
        let (l1, l5, l15) = self.data.load;
        let mut x = b.put_clip(tx, ry, "Load average: ", S_METER_CAPTION, lim);
        x = b.put_clip(x, ry, &format!("{:.2} ", l1), S_BOLD, lim);
        x = b.put_clip(x, ry, &format!("{:.2} ", l5), S_METER_VALUE, lim);
        b.put_clip(x, ry, &format!("{:.2} ", l15), S_METER_CAPTION, lim);
        ry += 1;
        let x = b.put_clip(tx, ry, "Uptime: ", S_METER_CAPTION, lim);
        b.put_clip(x, ry, &uptime_text(self.data.uptime), S_METER_VALUE, lim);
        ry += 1;
        ly.max(ry) + top
    }

    fn draw_help(&self, b: &mut Buf) {
        let w = b.w;
        let mut y = 0;
        b.put(0, y, &format!("{NAME} {VERSION} - a cross-platform process viewer that works like htop 3.3.0."), S_DEFAULT);
        y += 1;
        b.put(0, y, "Runs on Linux, macOS and Windows. Some columns need Linux to show real values.", S_DEFAULT);
        y += 2;
        let legend = |b: &mut Buf, y: usize, cap: &str, parts: &[(&str, Style)], tail: &str| {
            let mut x = b.put(0, y, cap, S_DEFAULT);
            x = b.put(x, y, "[", S_BOLD);
            for (i, (t, st)) in parts.iter().enumerate() {
                if i > 0 {
                    x = b.put(x, y, "/", S_DEFAULT);
                }
                x = b.put(x, y, t, *st);
            }
            x = b.put(x, y, "    ", S_DEFAULT);
            x = b.put(x, y, tail, S_SHADOW);
            b.put(x, y, "]", S_BOLD);
        };
        legend(b, y, "CPU usage bar: ", &[("low-priority", C_NICE), ("normal", C_USER), ("kernel", C_KERNEL), ("virtualized", C_VIRT)], "used%");
        y += 1;
        legend(b, y, "Memory bar:    ", &[("used", C_MEM_USED), ("shared", C_MEM_SHARED), ("buffers", C_MEM_BUFFERS), ("cache", C_MEM_CACHE)], "used/total");
        y += 1;
        legend(b, y, "Swap bar:      ", &[("used", C_SWAP), ("cache", C_MEM_CACHE)], "used/total");
        y += 2;
        b.put(0, y, "Process state: R: running; S: sleeping; T: stopped; Z: zombie; D: disk sleep; I: idle", S_DEFAULT);
        y += 2;
        let keys: &[(&str, &str, &str, &str)] = &[
            ("      Arrows", "scroll process list", "       Space", "tag process"),
            ("      Digits", "incremental PID search", "           c", "tag process and its children"),
            ("        F3 /", "incremental name search", "           U", "untag all processes"),
            ("        F4 \\", "incremental name filtering", "        F9 k", "kill process/tagged processes"),
            ("        F5 t", "tree view", "        F7 ]", "higher priority (root only)"),
            ("           p", "toggle program path", "        F8 [", "lower priority (+ nice)"),
            ("           u", "show processes of a single user", "           e", "show process environment"),
            ("           H", "hide/show user process threads", "           w", "wrap process command in multiple lines"),
            ("           K", "hide/show kernel threads", "           l", "list open files"),
            ("           F", "cursor follows process", "           Z", "pause/resume process updates"),
            ("       + - *", "expand/collapse tree", "    F6 < > .", "select sort column"),
            ("       P M T", "sort by CPU%, MEM% or TIME", "           I", "invert sort order"),
            ("           N", "sort by PID", "        F2 S", "setup"),
            ("         Tab", "switch to next screen", "      F1 h ?", "show this help screen"),
            ("           #", "hide/show header meters", "       F10 q", "quit"),
            ("      Ctrl-L", "refresh", "", ""),
        ];
        let half = w / 2;
        for (k1, d1, k2, d2) in keys {
            if y + 1 >= b.h {
                break;
            }
            let mut x = b.put(0, y, k1, S_METER_VALUE);
            x = b.put(x, y, ": ", S_DEFAULT);
            b.put_clip(x, y, d1, S_DEFAULT, half);
            if !k2.is_empty() {
                let mut x = b.put(half, y, k2, S_METER_VALUE);
                x = b.put(x, y, ": ", S_DEFAULT);
                b.put(x, y, d2, S_DEFAULT);
            }
            y += 1;
        }
        y += 1;
        if y < b.h {
            b.put(0, y, "Press any key to return.", S_BOLD);
        }
    }

    fn setup_items(&self) -> Vec<(&'static str, bool)> {
        let s = &self.s;
        vec![
            ("Tree view", self.screen().tree),
            ("Shadow other users' processes", s.shadow_other_users),
            ("Hide kernel threads", s.hide_kernel_threads),
            ("Hide userland process threads", s.hide_userland_threads),
            ("Display threads in a different color", s.highlight_threads),
            ("Show program path", s.show_program_path),
            ("Highlight large numbers in memory counters", s.highlight_megabytes),
            ("Leave a margin around header", s.header_margin),
            ("Show tabs for screens", s.screen_tabs),
            ("Hide function bar", s.hide_function_bar),
            ("Count CPUs from 1 instead of 0", s.cpu_count_from_one),
            ("Monochrome color scheme", s.monochrome),
            ("Enable the mouse", s.enable_mouse),
        ]
    }

    fn toggle_setup(&mut self, i: usize) {
        if i == 0 {
            let sc = self.screen_mut();
            sc.tree = !sc.tree;
        }
        let s = &mut self.s;
        match i {
            1 => s.shadow_other_users = !s.shadow_other_users,
            2 => s.hide_kernel_threads = !s.hide_kernel_threads,
            3 => s.hide_userland_threads = !s.hide_userland_threads,
            4 => s.highlight_threads = !s.highlight_threads,
            5 => s.show_program_path = !s.show_program_path,
            6 => s.highlight_megabytes = !s.highlight_megabytes,
            7 => s.header_margin = !s.header_margin,
            8 => s.screen_tabs = !s.screen_tabs,
            9 => s.hide_function_bar = !s.hide_function_bar,
            10 => s.cpu_count_from_one = !s.cpu_count_from_one,
            11 => s.monochrome = !s.monochrome,
            12 => s.enable_mouse = !s.enable_mouse,
            _ => {}
        }
        self.s.changed = true;
    }

    fn draw_setup(&mut self, b: &mut Buf, sel: usize) {
        let w = b.w;
        b.fill(0, 0, w, S_HEADER);
        b.put(0, 0, "Setup", S_HEADER);
        b.put(0, 1, "Display options", S_BOLD);
        let items = self.setup_items();
        for (i, (label, on)) in items.iter().enumerate() {
            let y = 2 + i;
            if y + 1 >= b.h {
                break;
            }
            let selected = i == sel;
            let st = if selected { S_SELECT } else { S_DEFAULT };
            if selected {
                b.fill(0, y, w, st);
            }
            let mut x = b.put(0, y, "[", if selected { st } else { S_BOLD });
            x = b.put(x, y, if *on { "x" } else { " " }, if selected { st } else { S_METER_VALUE });
            x = b.put(x, y, "]    ", if selected { st } else { S_BOLD });
            b.put(x, y, label, st);
        }
        let y = 2 + items.len();
        if y + 1 < b.h {
            let st = if sel == items.len() { S_SELECT } else { S_DEFAULT };
            if sel == items.len() {
                b.fill(0, y, w, st);
            }
            b.put(0, y, &format!("[{:>3}]  Update interval (tenths of a second; - and + change it)", self.s.delay), st);
        }
        let yb = b.h - 1;
        let mut x = b.put(0, yb, "Space", S_FKEY);
        x = b.put(x, yb, "Toggle", S_FLABEL);
        x = b.put(x, yb, "Esc", S_FKEY);
        x = b.put(x, yb, "Done  ", S_FLABEL);
        b.fill(x, yb, w, S_FLABEL);
    }

    fn draw_info(&self, b: &mut Buf) {
        let Mode::Info { title, lines, scroll } = &self.mode else { return };
        let w = b.w;
        b.fill(0, 0, w, S_HEADER);
        b.put(0, 0, title, S_HEADER);
        for (i, l) in lines.iter().skip(*scroll).enumerate() {
            let y = 1 + i;
            if y + 1 >= b.h {
                break;
            }
            b.put(0, y, l, S_DEFAULT);
        }
        let yb = b.h - 1;
        let mut x = b.put(0, yb, "Esc", S_FKEY);
        x = b.put(x, yb, "Done  ", S_FLABEL);
        b.fill(x, yb, w, S_FLABEL);
    }

    // --------------------------------------------------------------- input ---

    fn selected(&self) -> Option<&Proc> {
        self.rows.get(self.sel).map(|r| &r.p)
    }

    fn move_sel(&mut self, delta: i64) {
        if self.rows.is_empty() {
            return;
        }
        let n = self.rows.len() as i64;
        self.sel = (self.sel as i64 + delta).clamp(0, n - 1) as usize;
        if self.follow.is_some() {
            self.follow = self.selected().map(|p| p.pid);
        }
    }

    fn open_menu(&mut self, kind: MenuKind) {
        let (title, items, sel) = match kind {
            MenuKind::Sort => {
                let (key, _) = self.active_sort();
                let names: Vec<String> = fields::FIELDS.iter().map(|f| f.name.to_string()).collect();
                let sel = fields::FIELDS.iter().position(|f| f.f == key).unwrap_or(0);
                ("Sort by".to_string(), names, sel)
            }
            MenuKind::Kill => {
                let mut items = vec![" 0 Cancel".to_string()];
                items.extend(self.signals.iter().map(|(t, _)| t.clone()));
                let sel = items.iter().position(|t| t.contains("SIGTERM") || t.contains("Terminate")).unwrap_or(0);
                ("Send signal:".to_string(), items, sel)
            }
            MenuKind::User => {
                let mut items = vec!["All users".to_string()];
                items.extend(self.data.user_names());
                let sel = self.user_filter.as_ref().and_then(|u| items.iter().position(|i| i == u)).unwrap_or(0);
                ("Show processes of:".to_string(), items, sel)
            }
        };
        self.mode = Mode::Menu(Menu { kind, title, items, sel, scroll: 0 });
        self.fix_menu_scroll();
    }

    fn fix_menu_scroll(&mut self) {
        let lh = self.list_h.max(3);
        if let Mode::Menu(m) = &mut self.mode {
            if m.sel < m.scroll {
                m.scroll = m.sel;
            }
            if m.sel >= m.scroll + lh {
                m.scroll = m.sel + 1 - lh;
            }
        }
    }

    fn menu_accept(&mut self) {
        let Mode::Menu(m) = std::mem::replace(&mut self.mode, Mode::Normal) else { return };
        match m.kind {
            MenuKind::Sort => {
                if let Some(f) = fields::FIELDS.get(m.sel) {
                    let (key, _) = self.active_sort();
                    if key != f.f {
                        self.set_sort(f.f);
                    }
                }
            }
            MenuKind::Kill => {
                if m.sel > 0 {
                    if let Some((_, sig)) = self.signals.get(m.sel - 1).cloned() {
                        let targets: Vec<u32> = if self.tagged.is_empty() {
                            self.selected().map(|p| vec![p.pid]).unwrap_or_default()
                        } else {
                            self.tagged.iter().copied().collect()
                        };
                        for pid in targets {
                            self.data.kill(pid, sig);
                        }
                        self.tagged.clear();
                    }
                }
            }
            MenuKind::User => {
                self.user_filter = if m.sel == 0 { None } else { m.items.get(m.sel).cloned() };
            }
        }
        self.rebuild();
    }

    fn search(&mut self, text: &str, from: usize, forward: bool) -> bool {
        let n = self.rows.len();
        if n == 0 || text.is_empty() {
            return true;
        }
        let t = text.to_lowercase();
        for k in 0..n {
            let i = if forward { (from + k) % n } else { (from % n + n - k) % n };
            if fields::command_text(&self.rows[i].p, true).to_lowercase().contains(&t) {
                self.sel = i;
                return true;
            }
        }
        false
    }

    fn tag_children(&mut self, pid: u32) {
        let mut stack = vec![pid];
        while let Some(p) = stack.pop() {
            self.tagged.insert(p);
            for r in &self.data.procs {
                if r.ppid == p && r.pid != p && !self.tagged.contains(&r.pid) {
                    stack.push(r.pid);
                }
            }
        }
    }

    fn info_screen(&mut self, title: String, lines: Vec<String>) {
        self.mode = Mode::Info { title, lines, scroll: 0 };
    }

    fn set_search_result(&mut self, ok: bool) {
        if let Mode::Search { failed, .. } = &mut self.mode {
            *failed = !ok;
        }
    }

    pub fn key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        let page = self.list_h.max(1) as i64;
        match &mut self.mode {
            Mode::Help => {
                self.mode = Mode::Normal;
                return;
            }
            Mode::Info { scroll, lines, .. } => {
                let last = lines.len().saturating_sub(1);
                match k.code {
                    KeyCode::Down => *scroll = (*scroll + 1).min(last),
                    KeyCode::Up => *scroll = scroll.saturating_sub(1),
                    KeyCode::PageDown => *scroll = (*scroll + page as usize).min(last),
                    KeyCode::PageUp => *scroll = scroll.saturating_sub(page as usize),
                    _ => self.mode = Mode::Normal,
                }
                return;
            }
            Mode::Setup { sel } => {
                let cur = *sel;
                match k.code {
                    KeyCode::Up => *sel = cur.saturating_sub(1),
                    KeyCode::Down => *sel = (cur + 1).min(SETUP_TOGGLES),
                    KeyCode::Char(' ') | KeyCode::Enter if cur < SETUP_TOGGLES => self.toggle_setup(cur),
                    KeyCode::Char('+') | KeyCode::Right if cur == SETUP_TOGGLES => {
                        self.s.delay = (self.s.delay + 1).min(100);
                        self.s.changed = true;
                    }
                    KeyCode::Char('-') | KeyCode::Left if cur == SETUP_TOGGLES => {
                        self.s.delay = self.s.delay.saturating_sub(1).max(1);
                        self.s.changed = true;
                    }
                    KeyCode::Esc | KeyCode::F(10) | KeyCode::Char('q') | KeyCode::F(2) => {
                        self.mode = Mode::Normal;
                        self.rebuild();
                    }
                    _ => {}
                }
                return;
            }
            Mode::Search { text, failed } => {
                let shift = k.modifiers.contains(KeyModifiers::SHIFT);
                match k.code {
                    KeyCode::Esc | KeyCode::Enter => self.mode = Mode::Normal,
                    KeyCode::F(3) if shift => {
                        let t = text.clone();
                        let from = self.sel + self.rows.len().max(1) - 1;
                        let ok = self.search(&t, from, false);
                        self.set_search_result(ok);
                    }
                    KeyCode::F(13) => {
                        let t = text.clone();
                        let from = self.sel + self.rows.len().max(1) - 1;
                        let ok = self.search(&t, from, false);
                        self.set_search_result(ok);
                    }
                    KeyCode::F(3) => {
                        let t = text.clone();
                        let from = self.sel + 1;
                        let ok = self.search(&t, from, true);
                        self.set_search_result(ok);
                    }
                    KeyCode::Backspace => {
                        text.pop();
                        *failed = false;
                    }
                    KeyCode::Char(c) => {
                        text.push(c);
                        let t = text.clone();
                        let from = self.sel;
                        let ok = self.search(&t, from, true);
                        self.set_search_result(ok);
                    }
                    KeyCode::Up => self.move_sel(-1),
                    KeyCode::Down => self.move_sel(1),
                    _ => {}
                }
                return;
            }
            Mode::Filter { text } => {
                match k.code {
                    KeyCode::Esc => {
                        self.filter.clear();
                        self.mode = Mode::Normal;
                    }
                    KeyCode::Enter => self.mode = Mode::Normal,
                    KeyCode::Backspace => {
                        text.pop();
                        self.filter = text.clone();
                    }
                    KeyCode::Char(c) => {
                        text.push(c);
                        self.filter = text.clone();
                    }
                    KeyCode::Up => self.move_sel(-1),
                    KeyCode::Down => self.move_sel(1),
                    _ => {}
                }
                self.rebuild();
                return;
            }
            Mode::Menu(m) => {
                let n = m.items.len();
                match k.code {
                    KeyCode::Up => m.sel = m.sel.saturating_sub(1),
                    KeyCode::Down => m.sel = (m.sel + 1).min(n.saturating_sub(1)),
                    KeyCode::PageUp => m.sel = m.sel.saturating_sub(page as usize),
                    KeyCode::PageDown => m.sel = (m.sel + page as usize).min(n.saturating_sub(1)),
                    KeyCode::Home => m.sel = 0,
                    KeyCode::End => m.sel = n.saturating_sub(1),
                    KeyCode::Enter => {
                        self.menu_accept();
                        return;
                    }
                    KeyCode::Esc | KeyCode::F(10) | KeyCode::Char('q') => {
                        self.mode = Mode::Normal;
                        return;
                    }
                    KeyCode::Char(c) if c.is_alphanumeric() => {
                        let lc = c.to_ascii_lowercase().to_string();
                        if let Some(i) = m.items.iter().position(|t| t.trim_start().to_lowercase().starts_with(&lc)) {
                            m.sel = i;
                        }
                    }
                    _ => {}
                }
                self.fix_menu_scroll();
                return;
            }
            Mode::Normal => {}
        }
        // ---------- normal mode
        let ro = self.opts.readonly;
        match k.code {
            KeyCode::Up => self.move_sel(-1),
            KeyCode::Down => self.move_sel(1),
            KeyCode::PageUp => self.move_sel(-page),
            KeyCode::PageDown => self.move_sel(page),
            KeyCode::Home => self.move_sel(-(self.rows.len() as i64)),
            KeyCode::End => self.move_sel(self.rows.len() as i64),
            KeyCode::Left => self.hscroll = self.hscroll.saturating_sub(8),
            KeyCode::Right => self.hscroll += 8,
            KeyCode::Char('l') if ctrl => {}
            KeyCode::F(10) | KeyCode::Char('q') => self.quit = true,
            KeyCode::F(1) | KeyCode::Char('h') | KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::F(2) | KeyCode::Char('S') | KeyCode::Char('C') => self.mode = Mode::Setup { sel: 0 },
            KeyCode::F(3) | KeyCode::Char('/') => self.mode = Mode::Search { text: String::new(), failed: false },
            KeyCode::F(4) | KeyCode::Char('\\') => self.mode = Mode::Filter { text: self.filter.clone() },
            KeyCode::F(5) | KeyCode::Char('t') => {
                let sc = self.screen_mut();
                sc.tree = !sc.tree;
                self.s.changed = true;
                self.rebuild();
            }
            KeyCode::F(6) | KeyCode::Char('<') | KeyCode::Char('>') | KeyCode::Char('.') | KeyCode::Char(',') => {
                self.open_menu(MenuKind::Sort)
            }
            KeyCode::F(7) | KeyCode::Char(']') if !ro => self.renice(-1),
            KeyCode::F(8) | KeyCode::Char('[') if !ro => self.renice(1),
            KeyCode::F(9) | KeyCode::Char('k') if !ro => self.open_menu(MenuKind::Kill),
            KeyCode::Char('u') => self.open_menu(MenuKind::User),
            KeyCode::Char('P') => self.sort_shortcut(Field::PercentCpu),
            KeyCode::Char('M') => self.sort_shortcut(Field::PercentMem),
            KeyCode::Char('T') => self.sort_shortcut(Field::Time),
            KeyCode::Char('N') => self.sort_shortcut(Field::Pid),
            KeyCode::Char('I') => {
                self.invert_sort();
                self.rebuild();
            }
            KeyCode::Char('H') => {
                self.s.hide_userland_threads = !self.s.hide_userland_threads;
                self.s.changed = true;
                self.rebuild();
            }
            KeyCode::Char('K') => {
                self.s.hide_kernel_threads = !self.s.hide_kernel_threads;
                self.s.changed = true;
                self.rebuild();
            }
            KeyCode::Char('p') => {
                self.s.show_program_path = !self.s.show_program_path;
                self.s.changed = true;
            }
            KeyCode::Char('F') => {
                self.follow = if self.follow.is_some() { None } else { self.selected().map(|p| p.pid) };
            }
            KeyCode::Char('Z') | KeyCode::Char('z') => self.paused = !self.paused,
            KeyCode::Char('#') => self.hide_meters = !self.hide_meters,
            KeyCode::Tab | KeyCode::BackTab => {
                let n = self.s.screens.len();
                self.tab = if k.code == KeyCode::BackTab { (self.tab + n - 1) % n } else { (self.tab + 1) % n };
                self.rebuild();
            }
            KeyCode::Char(' ') => {
                if let Some(pid) = self.selected().map(|p| p.pid) {
                    if !self.tagged.remove(&pid) {
                        self.tagged.insert(pid);
                    }
                    self.move_sel(1);
                }
            }
            KeyCode::Char('c') => {
                if let Some(pid) = self.selected().map(|p| p.pid) {
                    self.tag_children(pid);
                }
            }
            KeyCode::Char('U') => self.tagged.clear(),
            // + - = (and Shift-F6) toggle the selected branch in tree view (0x13c90, 0x189d0)
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Char('-') | KeyCode::F(18) => {
                if self.screen().tree {
                    if let Some(pid) = self.rows.get(self.sel).map(|r| r.p.pid) {
                        if !self.collapsed.remove(&pid) {
                            self.collapsed.insert(pid);
                        }
                        self.rebuild();
                    }
                } else if k.code == KeyCode::F(18) {
                    self.open_menu(MenuKind::Sort);
                }
            }
            // Backspace in tree view: go to the parent and fold it (0x13ce0)
            KeyCode::Backspace => {
                if self.screen().tree {
                    if let Some(p) = self.selected().cloned() {
                        let parent = if p.ppid == p.pid { p.tgid } else { p.ppid };
                        if let Some(i) = self.rows.iter().position(|r| r.p.pid == parent) {
                            self.collapsed.insert(parent);
                            self.sel = i;
                            self.rebuild();
                        }
                    }
                }
            }
            // * in tree view flips "all branches collapsed" (0x19b50, Table_collapseAllBranches 0x34550:
            // every non-root row with a PID above 1 is folded)
            KeyCode::Char('*') => {
                if self.screen().tree {
                    self.all_collapsed = !self.all_collapsed;
                    if self.all_collapsed {
                        let fold: Vec<u32> = self.rows.iter().filter(|r| !r.prefix.is_empty() && r.p.pid > 1).map(|r| r.p.pid).collect();
                        self.collapsed.extend(fold);
                    } else {
                        self.collapsed.clear();
                    }
                    self.rebuild();
                }
            }
            KeyCode::Char('e') => {
                if let Some(p) = self.selected().cloned() {
                    let env = self.data.environ(p.pid).unwrap_or_default();
                    let lines = if env.is_empty() { vec!["(the environment of this process can't be read)".to_string()] } else { env };
                    self.info_screen(format!("Environment of process {} - {}", p.pid, fields::command_text(&p, true)), lines);
                }
            }
            KeyCode::Char('w') => {
                if let Some(p) = self.selected().cloned() {
                    let txt = fields::command_text(&p, true);
                    let chars: Vec<char> = txt.chars().collect();
                    let lines = chars.chunks(100).map(|c| c.iter().collect()).collect();
                    self.info_screen(format!("Command of process {}", p.pid), lines);
                }
            }
            KeyCode::Char('l') => {
                if let Some(p) = self.selected().cloned() {
                    let lines = list_open_files(p.pid);
                    self.info_screen(format!("Open files of process {} - {}", p.pid, fields::command_text(&p, true)), lines);
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let now = Instant::now();
                if now.duration_since(self.pid_search.1) > Duration::from_secs(1) {
                    self.pid_search.0.clear();
                }
                self.pid_search.0.push(c);
                self.pid_search.1 = now;
                let want = self.pid_search.0.clone();
                if let Some(i) = self.rows.iter().position(|r| r.p.pid.to_string().starts_with(&want)) {
                    self.sel = i;
                }
            }
            _ => {}
        }
    }

    fn sort_shortcut(&mut self, f: Field) {
        let (key, _) = self.active_sort();
        if key != f {
            self.set_sort(f);
        }
        self.rebuild();
    }

    fn renice(&mut self, delta: i64) {
        let targets: Vec<(u32, Option<i64>)> = if self.tagged.is_empty() {
            self.selected().map(|p| vec![(p.pid, p.nice)]).unwrap_or_default()
        } else {
            self.data.procs.iter().filter(|p| self.tagged.contains(&p.pid)).map(|p| (p.pid, p.nice)).collect()
        };
        for (pid, nice) in targets {
            data::renice(pid, nice, delta);
        }
    }

    pub fn mouse(&mut self, m: MouseEvent) {
        if !self.s.enable_mouse {
            return;
        }
        let (x, y) = (m.column as usize, m.row as usize);
        match m.kind {
            MouseEventKind::ScrollUp => {
                if let Mode::Menu(_) = self.mode {
                    self.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                } else {
                    self.move_sel(-3);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Mode::Menu(_) = self.mode {
                    self.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                } else {
                    self.move_sel(3);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if y == self.fbar_y {
                    if let Some(&(_, _, code)) = self.fbar_hits.iter().find(|(a, b, _)| x >= *a && x < *b) {
                        self.key(KeyEvent::new(code, KeyModifiers::NONE));
                    }
                    return;
                }
                if !matches!(self.mode, Mode::Normal | Mode::Menu(_)) {
                    return;
                }
                let (header_y, list_x0) = (self.header_y, self.list_x0);
                if let Mode::Menu(menu) = &mut self.mode {
                    if x < list_x0 {
                        if y > header_y {
                            let i = menu.scroll + (y - header_y - 1);
                            if i < menu.items.len() {
                                menu.sel = i;
                            }
                        }
                        return;
                    }
                }
                if Some(y) == self.tabs_y {
                    if let Some(&(_, _, i)) = self.tab_hits.iter().find(|(a, b, _)| x >= *a && x < *b) {
                        self.tab = i;
                        self.rebuild();
                    }
                } else if y == self.header_y {
                    if let Some(&(_, _, f)) = self.col_hits.iter().find(|(a, b, _)| x >= *a && x < *b) {
                        self.set_sort(f);
                        self.rebuild();
                    }
                } else if y >= self.list_y0 && y < self.list_y0 + self.list_h {
                    let i = self.scroll + (y - self.list_y0);
                    if i < self.rows.len() {
                        self.sel = i;
                        if self.follow.is_some() {
                            self.follow = Some(self.rows[i].p.pid);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn list_open_files(pid: u32) -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(rd) = std::fs::read_dir(format!("/proc/{pid}/fd")) {
            let mut v: Vec<(u32, String)> = rd
                .flatten()
                .filter_map(|e| {
                    let n: u32 = e.file_name().to_string_lossy().parse().ok()?;
                    let target = std::fs::read_link(e.path()).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
                    Some((n, target))
                })
                .collect();
            v.sort();
            let mut out = vec!["   FD  NAME".to_string()];
            out.extend(v.into_iter().map(|(n, t)| format!("{:5}  {}", n, t)));
            return out;
        }
    }
    match std::process::Command::new("lsof").args(["-P", "-n", "-p", &pid.to_string()]).output() {
        Ok(o) if !o.stdout.is_empty() => String::from_utf8_lossy(&o.stdout).lines().map(|s| s.to_string()).collect(),
        _ => vec!["Open files can't be listed here (this needs permission, or the lsof tool).".to_string()],
    }
}

/// Uptime meter text (sub_2fec0 @ 0x2fec0).
pub fn uptime_text(secs: u64) -> String {
    if secs == 0 {
        return "(unknown)".into();
    }
    let days = secs / 86400;
    let prefix = if secs > 8726399 {
        format!("{days} days(!)")
    } else if secs >= 172800 {
        format!("{days} days, ")
    } else if days == 1 {
        "1 day, ".to_string()
    } else {
        String::new()
    };
    format!("{}{:02}:{:02}:{:02}", prefix, (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}

/// A bar meter: caption, `[`, bar with text right-aligned inside, `]`.
#[allow(clippy::too_many_arguments)]
fn bar_meter(b: &mut Buf, x: usize, y: usize, w: usize, caption: &str, segs: &[(f64, Style)], total: f64, text: &str, mono: bool) {
    let cap: String = caption.chars().take(w).collect();
    let cap_len = cap.chars().count();
    let mut cx = b.put(x, y, &cap, S_METER_CAPTION);
    if w < cap_len + 2 {
        return;
    }
    let inner = w - cap_len - 2;
    cx = b.put(cx, y, "[", S_BOLD);
    let tchars: Vec<char> = text.chars().collect();
    let mut cells: Vec<char> = vec![' '; inner];
    let tl = tchars.len().min(inner);
    for i in 0..tl {
        cells[inner - tl + i] = tchars[tchars.len() - tl + i];
    }
    let mut styles: Vec<Style> = vec![S_SHADOW; inner];
    let mono_chars = ['|', '#', '*', '@', '$', '%', '&', '.'];
    let mut off = 0usize;
    for (i, (v, st)) in segs.iter().enumerate() {
        let n = if *v > 0.0 && total > 0.0 {
            let v = v.min(total);
            ((v / total * inner as f64).ceil() as usize).min(inner - off)
        } else {
            0
        };
        for j in off..off + n {
            if cells[j] == ' ' {
                cells[j] = if mono { mono_chars[i.min(7)] } else { '|' };
            }
            styles[j] = *st;
        }
        off += n;
    }
    for i in 0..inner {
        b.set(cx + i, y, cells[i], styles[i]);
    }
    b.put(cx + inner, y, "]", S_BOLD);
}
