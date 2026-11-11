//! Command-line options, parsed the way GNU getopt_long does for the original
//! (option string "hVMCs:td:n:u::Up:F:H:", see MECHANICS.md).

use crate::fields;
use std::io::Write;

/// println! that ignores a closed pipe (e.g. `multitop -s help | head`).
macro_rules! out {
    () => { let _ = writeln!(std::io::stdout()); };
    ($($t:tt)*) => { let _ = writeln!(std::io::stdout(), $($t)*); };
}

pub const NAME: &str = "multitop";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Default, Debug, Clone)]
pub struct Opts {
    pub no_color: bool,
    pub delay: Option<u32>,
    pub filter: Option<String>,
    pub highlight: Option<u32>,
    pub no_mouse: bool,
    pub max_iter: Option<u64>,
    pub pids: Option<Vec<u32>>,
    pub readonly: bool,
    pub sort_key: Option<fields::Field>,
    pub tree: bool,
    pub user: Option<String>,
    pub no_unicode: bool,
    pub screenshot: Option<std::path::PathBuf>,
}

pub enum Outcome {
    Run(Opts),
    Exit(i32),
}

#[derive(PartialEq, Clone, Copy)]
enum Arg {
    No,
    Required,
    Optional,
}

const LONG: &[(&str, char, Arg)] = &[
    ("no-color", 'C', Arg::No),
    ("no-colour", 'C', Arg::No),
    ("delay", 'd', Arg::Required),
    ("filter", 'F', Arg::Required),
    ("help", 'h', Arg::No),
    ("highlight-changes", 'H', Arg::Optional),
    ("no-mouse", 'M', Arg::No),
    ("max-iterations", 'n', Arg::Required),
    ("pid", 'p', Arg::Required),
    ("readonly", '\u{80}', Arg::No),
    ("sort-key", 's', Arg::Required),
    ("tree", 't', Arg::No),
    ("user", 'u', Arg::Optional),
    ("no-unicode", 'U', Arg::No),
    ("version", 'V', Arg::No),
    ("screenshot", '\u{81}', Arg::Required),
];

fn short_kind(c: char) -> Option<Arg> {
    match c {
        'h' | 'V' | 'M' | 'C' | 't' | 'U' => Some(Arg::No),
        's' | 'd' | 'n' | 'p' | 'F' => Some(Arg::Required),
        'u' | 'H' => Some(Arg::Optional),
        _ => None,
    }
}

/// Like C's sscanf("%d"): leading blanks, optional sign, then digits; the rest is ignored.
fn scan_int(s: &str) -> Option<i64> {
    let t = s.trim_start();
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let n: String = digits.chars().take_while(|c| c.is_ascii_digit()).collect();
    if n.is_empty() {
        return None;
    }
    let v: i64 = n.chars().take(16).collect::<String>().parse().ok()?;
    Some(if neg { -v } else { v })
}

fn atoi(s: &str) -> u32 {
    scan_int(s).map(|v| v.max(0) as u32).unwrap_or(0)
}

pub fn print_help() {
    out!("{NAME} {VERSION} - a cross-platform interactive process viewer");
    out!("(rebuilt to work like htop 3.3.0 on Linux, macOS and Windows)");
    out!();
    out!("-C --no-color                   Use a monochrome color scheme");
    out!("-d --delay=DELAY                Set the delay between updates, in tenths of seconds");
    out!("-F --filter=FILTER              Show only the commands matching the given filter");
    out!("-h --help                       Print this help screen");
    out!("-H --highlight-changes[=DELAY]  Highlight new and old processes");
    out!("-M --no-mouse                   Disable the mouse");
    out!("-n --max-iterations=NUMBER      Exit after NUMBER iterations/frame updates");
    out!("-p --pid=PID[,PID,PID...]       Show only the given PIDs");
    out!("   --readonly                   Disable all system and process changing features");
    out!("-s --sort-key=COLUMN            Sort by COLUMN in list view (try --sort-key=help for a list)");
    out!("-t --tree                       Show the tree view (can be combined with -s)");
    out!("-u --user[=USERNAME]            Show only processes for a given user (or $USER)");
    out!("-U --no-unicode                 Do not use unicode but plain ASCII");
    out!("-V --version                    Print version info");
    out!("   --screenshot=FILE            Draw the screen and save it as a PNG picture, then exit");
    out!();
    out!("Press F1 inside {NAME} for online help.");
}

fn print_sort_help() {
    for f in fields::FIELDS {
        out!("{:>19} {}", f.name, f.desc);
    }
}

pub fn parse(args: &[String], user_exists: &dyn Fn(&str) -> bool) -> Outcome {
    let prog = args
        .first()
        .map(|a| fields::basename(a).trim_end_matches(".exe").to_string())
        .unwrap_or_else(|| NAME.to_string());
    let mut o = Opts::default();
    let mut extra: Vec<String> = Vec::new();
    let mut i = 1;
    let fail = |msg: String| {
        eprint!("{msg}");
        Outcome::Exit(1)
    };
    while i < args.len() {
        let a = &args[i];
        i += 1;
        if a == "--" {
            extra.extend(args[i..].iter().cloned());
            break;
        }
        let mut items: Vec<(char, Option<String>)> = Vec::new();
        if let Some(long) = a.strip_prefix("--") {
            let (name, val) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            let exact = LONG.iter().find(|(n, _, _)| *n == name);
            let matches: Vec<_> = LONG.iter().filter(|(n, _, _)| n.starts_with(name)).collect();
            let opt = match exact {
                Some(e) => e,
                None if matches.len() == 1 => matches[0],
                None if matches.len() > 1 && matches.iter().all(|m| m.1 == matches[0].1) => matches[0],
                None if matches.len() > 1 => {
                    return fail(format!("{prog}: option '--{name}' is ambiguous\n"));
                }
                None => return fail(format!("{prog}: unrecognized option '--{name}'\n")),
            };
            let (full, c, kind) = *opt;
            let val = match kind {
                Arg::No => {
                    if val.is_some() {
                        return fail(format!("{prog}: option '--{full}' doesn't allow an argument\n"));
                    }
                    None
                }
                Arg::Optional => match val {
                    Some(v) => Some(v),
                    None if i < args.len() && !args[i].is_empty() && !args[i].starts_with('-') => {
                        i += 1;
                        Some(args[i - 1].clone())
                    }
                    None => None,
                },
                Arg::Required => match val {
                    Some(v) => Some(v),
                    None if i < args.len() => {
                        i += 1;
                        Some(args[i - 1].clone())
                    }
                    None => return fail(format!("{prog}: option '--{full}' requires an argument\n")),
                },
            };
            items.push((c, val));
        } else if a.starts_with('-') && a.len() > 1 {
            let chars: Vec<char> = a[1..].chars().collect();
            let mut j = 0;
            while j < chars.len() {
                let c = chars[j];
                j += 1;
                match short_kind(c) {
                    None => return fail(format!("{prog}: invalid option -- '{c}'\n")),
                    Some(Arg::No) => items.push((c, None)),
                    Some(kind) => {
                        let rest: String = chars[j..].iter().collect();
                        j = chars.len();
                        if !rest.is_empty() {
                            items.push((c, Some(rest)));
                        } else if kind == Arg::Required {
                            if i < args.len() {
                                i += 1;
                                items.push((c, Some(args[i - 1].clone())));
                            } else {
                                return fail(format!("{prog}: option requires an argument -- '{c}'\n"));
                            }
                        } else if i < args.len() && !args[i].is_empty() && !args[i].starts_with('-') {
                            // like the original: an optional value may also be the next word
                            i += 1;
                            items.push((c, Some(args[i - 1].clone())));
                        } else {
                            items.push((c, None));
                        }
                    }
                }
            }
        } else {
            extra.push(a.clone());
            continue;
        }
        for (c, val) in items {
            let v = val.clone().unwrap_or_default();
            match c {
                'h' => {
                    print_help();
                    return Outcome::Exit(0);
                }
                'V' => {
                    out!("{NAME} {VERSION}");
                    return Outcome::Exit(0);
                }
                'M' => o.no_mouse = true,
                'C' => o.no_color = true,
                'U' => o.no_unicode = true,
                't' => o.tree = true,
                '\u{80}' => o.readonly = true,
                '\u{81}' => o.screenshot = Some(std::path::PathBuf::from(v)),
                's' => {
                    if v == "help" {
                        print_sort_help();
                        return Outcome::Exit(0);
                    }
                    match fields::by_exact_name(&v) {
                        Some(f) => o.sort_key = Some(f),
                        None => return fail(format!("Error: invalid column \"{v}\".\n")),
                    }
                }
                'd' => match scan_int(&v) {
                    Some(d) => o.delay = Some(d.clamp(1, 100) as u32),
                    None => return fail(format!("Error: invalid delay value \"{v}\".\n")),
                },
                'n' => match scan_int(&v) {
                    Some(n) if n <= 0 => return fail("Error: maximum iteration count must be positive.\n".into()),
                    Some(n) => o.max_iter = Some(n as u64),
                    None => return fail(format!("Error: invalid maximum iteration count \"{v}\".\n")),
                },
                'u' => {
                    let name = match val {
                        Some(n) => n,
                        None => std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default(),
                    };
                    // CommandLine_run 0x1d310: an unknown name made only of digits is taken as a user id
                    let numeric = !name.is_empty() && name.chars().all(|c| c.is_ascii_digit());
                    if !numeric && !user_exists(&name) {
                        return fail(format!("Error: invalid user \"{name}\".\n"));
                    }
                    o.user = Some(name);
                }
                'p' => {
                    let list: Vec<u32> = v.split(',').filter(|s| !s.is_empty()).map(atoi).collect();
                    o.pids.get_or_insert_with(Vec::new).extend(list);
                }
                'F' => o.filter = Some(v),
                'H' => match val {
                    None => o.highlight = Some(5),
                    Some(s) => match scan_int(&s) {
                        Some(d) => o.highlight = Some(d.max(1).min(u32::MAX as i64) as u32),
                        None => return fail(format!("Error: invalid highlight delay value \"{s}\".\n")),
                    },
                },
                _ => {}
            }
        }
    }
    if !extra.is_empty() {
        let mut msg = String::from("Error: unsupported non-option ARGV-elements:");
        for e in &extra {
            msg.push(' ');
            msg.push_str(e);
        }
        msg.push('\n');
        return fail(msg);
    }
    Outcome::Run(o)
}
