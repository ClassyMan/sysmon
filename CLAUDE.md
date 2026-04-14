# sysmon

Cargo workspace with 7 real-time Linux system monitors (Rust + ratatui).

## Crates
- `shared/` — line_chart, ring_buffer, sticky_max (shared library)
- `cpu/` — per-core CPU utilization
- `gpu/` — NVIDIA GPU (nvidia-smi)
- `ram/` — memory, swap, page faults, PSI
- `dio/` — disk IOPS and latency
- `net/` — network throughput + matrix rain mode
- `audio/` — FFT spectrum analyzer (PipeWire)

## Build & install
```bash
cargo build --release                    # all
cargo build --release -p <crate>         # one
cp target/release/<name> ~/.local/bin/   # install
```

## Test
```bash
cargo test --workspace                       # all tests
cargo llvm-cov --workspace --summary-only    # coverage
```

## Test patterns
- **UI tests**: `TestBackend::new(120, 40)` + `Terminal::new(backend)`, assert no panic and check `buffer_to_string` for key content
- **App tests**: `App::with_capacity(n)` constructor in `#[cfg(test)]` bypasses I/O (no `/proc`, no `nvidia-smi`)
- **Collector tests**: Parsers are extracted from I/O — `parse_cpuinfo()`, `parse_loadavg()`, `filter_and_sort_interfaces()`, `parse_wpctl_inspect()`, etc. Test the parser, not the file read.
- **No DI**: Collectors are thin I/O wrappers around parsers. Don't add traits/generics to mock file reads — extract the parser instead.

## Release
All binaries released together from the monorepo:
```bash
gh release create vX.Y.Z target/release/cpu target/release/gpu target/release/ram target/release/dio target/release/net target/release/audio
```

## Chart rendering rules (hard-won)
1. **1:1 column mapping only** — never downsample. Show last `width` points when data_len > width.
2. **Sticky Y-axis** — ratchets up instantly, decays after 60s (`StickyMax`).
3. **Capacity >= terminal width** — `max(time_based, term_width)`.

## Visual design
- Art over science — animated, alive, not tables
- Color by meaning (green=low, red=high), not arbitrary rainbows
- Human labels (Download/Upload not RX/TX)
- No IP addresses in headers (doxxing risk)
- Split overlapping data into side-by-side charts

## Keybindings (all tools)
- `q` / `Esc` / `Ctrl+C` — quit
- `f` — toggle fast mode (25ms/3s)
- `d` / `D` — cycle devices/interfaces (dio, net)
- `v` — toggle view mode (net: charts ↔ rain)
