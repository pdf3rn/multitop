# multitop — rules taken from the original (htop 3.3.0, /usr/bin/htop)

Every value below was read from the original's code with Ghidra (native_function_pseudocode) and the
built-in lifter, or observed by running it in a pseudo-terminal. "estimated" means it was looked for and not found in code.

## Command line (CommandLine_run @ 0x1d310, getopt string "hVMCs:td:n:u::Up:F:H:")
| option | rule |
|---|---|
| -d/--delay N | tenths of a second; parsed like `sscanf("%d")`; clamped to 1..100; non-number → `Error: invalid delay value "%s".` exit 1 (string @0x4b078) |
| -n/--max-iterations N | `<= 0` → `Error: maximum iteration count must be positive.` (0x4b0a0); not a number → `Error: invalid maximum iteration count "%s".` (0x4b0d8); exit 1 |
| -H[N] | highlight new/old processes for N seconds (default 5, from htoprc `highlight_changes_delay_secs=5`); bad → `Error: invalid highlight delay value "%s".` (0x4b108) |
| -u[USER] | no value → $USER; unknown → `Error: invalid user "%s".` (0x47551) exit 1 |
| -s COL | `help` lists columns and exits 0; unknown → `Error: invalid column "%s".` (0x47585) exit 1 |
| -p a,b,c | comma list, atoi per item (non-numbers are not an error — observed) |
| extra words | `Error: unsupported non-option ARGV-elements:` + ` word`… + newline, exit 1 (0x4b138) |
| unknown option | getopt message `prog: unrecognized option '--x'`, exit 1 (observed) |
| -h / -V | help / version to stdout, exit 0 (observed) |

## Start-up
* Two scans 75 ms apart before the first frame (timespec @0x4d3f0 = 0x047868c0 ns = 75 000 000 ns).
* Default refresh delay 15 tenths (htoprc `delay=15`).

## Header (sub_32480 @ 0x32480 = default meters; Header_draw @ 0x20230)
* Two columns 50/50 (`header_layout=two_50_50`), margin 2 cells each side, 1 cell gap, one blank row above and below (`header_margin=1`).
* CPU meters by CPU count: ≤4 AllCPUs (left only); 5–8 LeftCPUs/RightCPUs; 9–16 two per row; 17–32 four; 33–128 eight; >128 one average "CPU" meter.
* Left: CPUs (bar), Memory (bar), Swap (bar). Right: CPUs (bar), Tasks (text), LoadAverage (text), Uptime (text). Meter mode ids: 1 = bar, 2 = text.
* Bar: caption (3 chars, cyan) `[` bar `]`; text right-aligned inside the bar; bar cells `|` coloured per segment, uncovered text grey.
* CPU segments: nice (blue), user (green), kernel = system+irq+softirq (red), steal+guest (cyan). Text `%5.1f%%`.
* Memory segments: used (green), shared (magenta), buffers (bold blue), cache (yellow); text `used/total` with Meter_humanUnit.
* Swap: used (red), cache (yellow).
* Meter_humanUnit @0x28690: value in KiB; divide by 1024 while ≥1024 (units K M G T P E Z Y R Q); 2 decimals when ≤9.99, 1 when ≤99.9, else 0 (constants 0x4d310=99.9, 0x4d318=9.99, 0x4d2f8=1024).
* Tasks (sub_318d0 @0x318d0): `Tasks: N, T thr, K kthr; R running` — thr/kthr parts grey when those threads are hidden.
* Load average: `Load average: a b c` with a bold white, b bold cyan, c cyan (`%.2f `).
* Uptime (sub_2fec0 @0x2fec0): `HH:MM:SS`, prefix `1 day, ` / `%d days, ` (≥ 172800 s) / `%d days(!)` (> 8 726 399 s, no comma).

## Process list
* Default columns (string @0x4c948): `PID USER PRIORITY NICE M_VIRT M_RESIDENT M_SHARE STATE PERCENT_CPU PERCENT_MEM TIME Command`.
* Second tab "I/O" (0x4c9a8): `PID USER IO_PRIORITY IO_RATE IO_READ_RATE IO_WRITE_RATE PERCENT_SWAP_DELAY PERCENT_IO_DELAY Command`.
* Default sort PERCENT_CPU descending; header arrow ▽ for descending, △ for ascending (observed).
* Row_printPercentage @0x2e760: grey below 0.05 (0x47014), highlight colour at ≥ 99.9 (0x47010); width-4 column shows `100` above 99.9; negative → `N/A`.
* Row_printKBytes @0x30b70: `<1000` → `%5u `; `<100000` → thousands in cyan + `%03u`; else hundredths of MiB, divided by 1024 while > 999999; `≥100` units → `%4u%c `, `≥1000` → `%1u%03u%c`; `<10` → `%1u.%02u%c`; `<100` → `%2u.%1u%c`. Colours M cyan, G green, T+ red.
* Row_printTime @0x31250 (centiseconds): <360000 `%2u:%02u.%02u `; <8640000 `%2uh` (cyan) `%02u:%02u `; <86400000 `%1ud%02uh%02um `; ≤ 365 days `%4ud%02uh `; years `%3uy%03ud `; then `%7luy `; else `eternity `.
* Row_printRate @0x31650: <0 blank; <0.005 `%7.2f B/s ` grey; <1024 B/s; <1 MiB K/s; <1 GiB M/s (cyan); then G/s, T/s, P/s (red).
* Nice: negative red, zero grey, positive green. State R green, D bold red, others grey. Other users' names grey. Threads green. Tree lines cyan.
* Function bar (strings 0x48b7d…): `Help Setup Search Filter Tree/List SortBy Nice - Nice + Kill Quit`, keys default colour, labels black on cyan; F5 says `List` in tree view, F4 says `FILTER` while a filter is on.

## Keys (help screen of the original)
Arrows/PgUp/PgDn/Home/End move; Space tag; c tag with children; U untag; F1 h ? help; F2 S setup; F3 / search; F4 \ filter;
F5 t tree; F6 < > . sort menu; P M T N sort by CPU/MEM/TIME/PID; I invert; F7 ] nice −; F8 [ nice +; F9 k kill; F10 q quit;
u user menu; H hide threads; K hide kernel threads; p program path; F follow; + - * expand/collapse; Tab next screen;
e environment; w wrap command; l open files; Z pause; # hide meters; Ctrl-L redraw.

## Not available on every system (estimated)
* PRI/NI/SHR/processor/I/O priority come from /proc on Linux; on macOS NI comes from getpriority and PRI = 20+NI; on Windows they show `-`.
* CPU segment split and memory buffers/shared come from /proc on Linux; elsewhere CPU is one green segment and cache = available − free.
* Swap-delay and I/O-delay percentages need Linux delay accounting and show `N/A` everywhere (as htop does without root).

## Confirmed with Ghidra (second pass) and what changed in the build

### Process_writeField @ 0x2c3e0 (column cells)
* USER: other users' names grey (`htopUserId != uid`); processes with raised privileges magenta (Linux-only flag, not shown here: estimated off).
* PRIORITY: `" RT "` when ≤ −100, else `"%3ld "`. **Fixed** (was `"RT  "`).
* NICE: < 0 high-priority colour (red), 0 grey, > 0 low-priority colour (green). Unchanged.
* STATE: letters from table @0x4db18 = `?URQWDBPTtZXIS`. Colour masks: 0x40c → green (U, R),
  0x1ac0 → bold red (D, B, P, t, Z, X), 0x6030 → grey (Q, W, I, S), others default (T, ?). **Fixed** (Z/t/X were default).
* NLWP grey when 1; TGID grey when it equals the PID (**added**).
* EXE/COMM without a value: `KTHREAD` for kernel threads, else `N/A`, grey (**added**).
* Command: thread colour for userland *and* kernel threads when highlight_threads is on (**fixed**).
  Tree prefix: per level `"   "` or `"│  "`, then `├`/`└` + `─` (open) or `+` (folded) + space, tree colour.

### Row_printKBytes @ 0x30b70 / Row_printTime @ 0x31250
* Unknown size prints `"  N/A "` (6 wide, grey). **Fixed** (was 7 wide).
* Above 1000 units the leading digit uses the *next* unit's colour and the rest this unit's colour. **Fixed**.
* Loop divides by 1024 while the value (hundredths of MiB) is ≥ 1000000.
* Time thresholds confirmed: 360000, 0x83d600 = 8640000, 86400000, 0xbbf81e00 = 3153600000 cs,
  0x2de41353000 (1000 years), 0x7009d32da2ffff, then `eternity `.

### Memory and swap (Machine_scan @ 0x41bc0, Platform_setMemoryValues @ 0x3ba40, MemoryMeter @ 0x293a0, SwapMeter @ 0x2f890, Platform_setSwapValues @ 0x3bca0)
* used = MemTotal − (MemFree + Buffers + Cached + SReclaimable) (or MemTotal − MemFree when that would be negative);
  shared = Shmem; buffers = Buffers; cache = Cached + SReclaimable − Shmem; available = min(MemAvailable, MemTotal).
  **Fixed**: the build subtracted Shmem from "used" a second time.
* Zswap: subtracted from used and drawn as a "compressed" segment. Bar order used, shared, compressed, buffers, cache.
* Text = humanUnit(used + shared + compressed) "/" humanUnit(total).
* Swap used = SwapTotal − SwapFree − SwapCached (− Zswapped, shown as its own segment); text used/total.
* (ZFS ARC and hugepages corrections exist in the original; not applied — Linux-only, estimated as zero.)

### CPU (Platform_setCPUValues @ 0x3b5e0)
* Non-detailed mode, 4 values as % of the period: nice, user, system+irq+softirq, steal+guest; total = sum of positive values, capped at 100.

### Header (Header_draw @ 0x20230) and list header (Table_printHeader @ 0x31d50)
* Columns: width = COLS − 2·pad − (columns − 1), each column = percent·width/100 with the fraction carried to the next; x advances by width + 1.
* Sort column title in the selection colour; its trailing space becomes `△` (ascending, direction 1) or `▽` (descending).

### Command line (CommandLine_run @ 0x1d310)
* `-s` compares names exactly (strcmp). **Fixed** (was case-insensitive).
* `-u NAME`: unknown name made only of digits is used as a numeric user id. **Added**.
* `-H N`: values below 1 become 1; no upper limit. **Fixed** (was capped at one day).
* `-d`: clamped 1..100. `-n` ≤ 0 → error. Help and version go to stdout, exit 0; errors exit 1.

### Keys (Action_setBindings @ 0x15c90, MainPanel_new @ 0x29250)
* `C` also opens Setup (**added**). `+`, `-`, `=` all *toggle* the selected branch, tree view only (0x13c90) (**fixed**).
* Shift-F6 toggles the branch in tree view, otherwise opens Sort (0x189d0) (**added**).
* Backspace in tree view selects the parent and folds it (0x13ce0) (**added**).
* `*` in tree view flips "all branches collapsed": every non-root row with PID > 1 folds (0x19b50, Table_collapseAllBranches @0x34550) (**fixed**).
* Linux-only keys not in this version: `a` CPU affinity, `i` I/O priority, `s` strace, `x` file locks, `Y` scheduling policy (0x19cb0), `O` hide container processes, `m` merged command, `{` `}`.
