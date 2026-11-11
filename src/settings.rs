//! Preferences, kept in the same "key=value" format as the original's htoprc.
//! Read from our own file; if that doesn't exist yet, the original's file is imported.

use crate::fields::{self, Field};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Screen {
    pub name: String,
    pub columns: Vec<Field>,
    pub sort_key: Field,
    pub sort_dir: i32,
    pub tree_sort_key: Field,
    pub tree_sort_dir: i32,
    pub tree: bool,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub delay: u32,
    pub hide_kernel_threads: bool,
    pub hide_userland_threads: bool,
    pub shadow_other_users: bool,
    pub show_program_path: bool,
    pub highlight_megabytes: bool,
    pub highlight_threads: bool,
    pub header_margin: bool,
    pub screen_tabs: bool,
    pub hide_function_bar: bool,
    pub cpu_count_from_one: bool,
    pub monochrome: bool,
    pub enable_mouse: bool,
    pub screens: Vec<Screen>,
    pub changed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            delay: 15,
            hide_kernel_threads: true,
            hide_userland_threads: false,
            shadow_other_users: false,
            show_program_path: true,
            highlight_megabytes: true,
            highlight_threads: true,
            header_margin: true,
            screen_tabs: true,
            hide_function_bar: false,
            cpu_count_from_one: false,
            monochrome: false,
            enable_mouse: true,
            screens: vec![
                Screen {
                    name: "Main".into(),
                    columns: fields::MAIN_COLUMNS.to_vec(),
                    sort_key: Field::PercentCpu,
                    sort_dir: -1,
                    tree_sort_key: Field::Pid,
                    tree_sort_dir: 1,
                    tree: false,
                },
                Screen {
                    name: "I/O".into(),
                    columns: fields::IO_COLUMNS.to_vec(),
                    sort_key: Field::IoRate,
                    sort_dir: -1,
                    tree_sort_key: Field::Pid,
                    tree_sort_dir: 1,
                    tree: false,
                },
            ],
            changed: false,
        }
    }
}

fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if let Some(x) = std::env::var_os("XDG_CONFIG_HOME").filter(|x| !x.is_empty()) {
        Some(PathBuf::from(x))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
    }
}

pub fn own_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MULTITOPRC") {
        return Some(PathBuf::from(p));
    }
    config_dir().map(|d| d.join("multitop").join("multitoprc"))
}

fn original_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("HTOPRC") {
        return Some(PathBuf::from(p));
    }
    config_dir().map(|d| d.join("htop").join("htoprc"))
}

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        let text = own_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .or_else(|| original_path().and_then(|p| std::fs::read_to_string(p).ok()));
        if let Some(t) = text {
            s.parse(&t);
        }
        s.changed = false;
        s
    }

    fn parse(&mut self, text: &str) {
        let b = |v: &str| v.trim() == "1";
        let mut read_screens: Vec<Screen> = Vec::new();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            if let Some(name) = k.strip_prefix("screen:") {
                let cols: Vec<Field> = v.split_whitespace().filter_map(fields::by_name).collect();
                if cols.is_empty() {
                    continue;
                }
                let base = Settings::default().screens.into_iter().find(|s| s.name == name);
                read_screens.push(Screen {
                    name: name.to_string(),
                    columns: cols,
                    sort_key: base.as_ref().map(|b| b.sort_key).unwrap_or(Field::PercentCpu),
                    sort_dir: -1,
                    tree_sort_key: Field::Pid,
                    tree_sort_dir: 1,
                    tree: false,
                });
                continue;
            }
            if let Some(sub) = k.strip_prefix('.') {
                if let Some(sc) = read_screens.last_mut() {
                    match sub {
                        "sort_key" => sc.sort_key = fields::by_name(v.trim()).unwrap_or(sc.sort_key),
                        "tree_sort_key" => sc.tree_sort_key = fields::by_name(v.trim()).unwrap_or(sc.tree_sort_key),
                        "sort_direction" => sc.sort_dir = if v.trim() == "1" { 1 } else { -1 },
                        "tree_sort_direction" => sc.tree_sort_dir = if v.trim() == "-1" { -1 } else { 1 },
                        "tree_view" => sc.tree = b(v),
                        _ => {}
                    }
                }
                continue;
            }
            match k {
                "delay" => self.delay = v.trim().parse::<u32>().unwrap_or(15).clamp(1, 100),
                "hide_kernel_threads" => self.hide_kernel_threads = b(v),
                "hide_userland_threads" => self.hide_userland_threads = b(v),
                "shadow_other_users" => self.shadow_other_users = b(v),
                "show_program_path" => self.show_program_path = b(v),
                "highlight_megabytes" => self.highlight_megabytes = b(v),
                "highlight_threads" => self.highlight_threads = b(v),
                "header_margin" => self.header_margin = b(v),
                "screen_tabs" => self.screen_tabs = b(v),
                "hide_function_bar" => self.hide_function_bar = v.trim() != "0",
                "cpu_count_from_one" => self.cpu_count_from_one = b(v),
                "color_scheme" => self.monochrome = v.trim() == "1",
                "enable_mouse" => self.enable_mouse = b(v),
                _ => {}
            }
        }
        if !read_screens.is_empty() {
            self.screens = read_screens;
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = own_path() else { return Ok(()) };
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let i = |v: bool| if v { 1 } else { 0 };
        let mut t = String::new();
        t.push_str("# multitop settings (same format as htoprc). Rewritten when settings change.\n");
        t.push_str(&format!("delay={}\n", self.delay));
        t.push_str(&format!("hide_kernel_threads={}\n", i(self.hide_kernel_threads)));
        t.push_str(&format!("hide_userland_threads={}\n", i(self.hide_userland_threads)));
        t.push_str(&format!("shadow_other_users={}\n", i(self.shadow_other_users)));
        t.push_str(&format!("show_program_path={}\n", i(self.show_program_path)));
        t.push_str(&format!("highlight_megabytes={}\n", i(self.highlight_megabytes)));
        t.push_str(&format!("highlight_threads={}\n", i(self.highlight_threads)));
        t.push_str(&format!("header_margin={}\n", i(self.header_margin)));
        t.push_str(&format!("screen_tabs={}\n", i(self.screen_tabs)));
        t.push_str(&format!("hide_function_bar={}\n", i(self.hide_function_bar)));
        t.push_str(&format!("cpu_count_from_one={}\n", i(self.cpu_count_from_one)));
        t.push_str(&format!("color_scheme={}\n", i(self.monochrome)));
        t.push_str(&format!("enable_mouse={}\n", i(self.enable_mouse)));
        for s in &self.screens {
            let cols: Vec<&str> = s.columns.iter().map(|c| fields::info(*c).name).collect();
            t.push_str(&format!("screen:{}={}\n", s.name, cols.join(" ")));
            t.push_str(&format!(".sort_key={}\n", fields::info(s.sort_key).name));
            t.push_str(&format!(".tree_sort_key={}\n", fields::info(s.tree_sort_key).name));
            t.push_str(&format!(".tree_view={}\n", i(s.tree)));
            t.push_str(&format!(".sort_direction={}\n", s.sort_dir));
            t.push_str(&format!(".tree_sort_direction={}\n", s.tree_sort_dir));
        }
        std::fs::write(&path, t).map_err(|e| format!("Can not save configuration to {}: {e}", path.display()))
    }
}
