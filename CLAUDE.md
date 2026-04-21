# sysmon

Cargo workspace with 8 real-time monitors plus a compositor (Rust + ratatui).

## Crates
- `shared/` — line_chart, ring_buffer, sticky_max (shared library)
- `cpu/` — per-core CPU utilization
- `gpu/` — NVIDIA GPU (nvml-wrapper, not nvidia-smi — works in strict snap confinement)
- `ram/` — memory, swap, page faults, PSI
- `dio/` — disk IOPS and latency
- `net/` — network throughput + matrix rain mode
- `audio/` — FFT spectrum analyzer (libpipewire direct)
- `poly/` — Polymarket prediction market dashboard (Gamma + CLOB APIs)
- `astro/` — NASA Astronomy Picture of the Day TUI viewer
- `sysmon/` — compositor binary: `sysmon --cpu --gpu --audio ...` tiles multiple tools in one window

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
gh release create vX.Y.Z target/release/cpu target/release/gpu target/release/ram target/release/dio target/release/net target/release/audio target/release/poly
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
- `j`/`k` / `↓`/`↑` — scroll markets (poly)

## poly-specific notes
- Fetches from Polymarket Gamma API (events/metadata) and CLOB API (price history)
- No auth needed — read-only public endpoints
- Background thread handles HTTP; main thread never blocks on network
- Default 30s refresh, fast mode 5s — Gamma rate limit is 500/10s so both are safe
- Uses `reqwest` blocking + `serde`

## audio-specific notes
- Uses libpipewire directly via `pipewire` + `libspa` crates (not `pw-record` — subprocess binary isn't in snap)
- Build deps: `clang` + `libclang-dev` for bindgen; snap stage-packages include `libpipewire-0.3-0` + `libspa-0.2-modules`
- Snap env vars: `SPA_PLUGIN_DIR=$SNAP/usr/lib/$CRAFT_ARCH_TRIPLET/spa-0.2`, `PIPEWIRE_MODULE_DIR=$SNAP/usr/lib/$CRAFT_ARCH_TRIPLET/pipewire-0.3`
- Capture props: `MEDIA_CATEGORY=Capture` + `STREAM_CAPTURE_SINK=true` + `AUTOCONNECT` flag. **Do NOT set `MEDIA_ROLE`** — it silently breaks WirePlumber auto-routing to the default sink monitor
- In `.process()` callback, always slice by `data.chunk().size()` and `.offset()`, NEVER `data.data().len()` — the mmap'd buffer is much larger than the valid chunk, reading the full slice feeds the FFT zeroed memory
- Audio crate is lib+bin (like other tools) so `sysmon --audio` composites it alongside cpu/gpu/etc

## Debugging audio in snap
- `pw-link -l | grep sysmon-audio` — should show `<sink>:monitor_FL -> sysmon-audio:input_FL`. Empty = routing broken
- `pw-cli ls Node | grep -B1 -A25 sysmon-audio` — confirms `stream.capture.sink=true` reached the server
- UI header shows `<state> | <rate>Hz | buf=N peak=X.XXXX` for live diagnosis — peak=0 with active audio means we're linked to nothing / dummy
- TUI rule: thread errors go into `Arc<Mutex<Option<String>>>` and render in the header — `eprintln!` is invisible under the alternate screen

## astro-specific notes
- Fetches from NASA APOD API (public, key-authenticated)
- Uses `reqwest` blocking + `rustls-tls` + `serde`
- Background thread handles HTTP; client constructed on main thread (tokio DNS resolver fix)
- Cache in `XDG_CACHE_HOME/sysmon/astro/` (snap-aware, falls back to `SNAP_USER_COMMON/.cache`)
- Theme detection reads kitty config via `XDG_CONFIG_HOME` (snap-aware)

## Terminal theme detection
- `shared/src/terminal_theme.rs` resolves a 16-color palette at startup via `init()`
- Detection order: ghostty config (`~/.config/ghostty/config` + theme file) → kitty config (with include chain) → OSC 11/10/4 query → Catppuccin Mocha default
- Env-based terminal detection: `TERM_PROGRAM=ghostty` / `GHOSTTY_RESOURCES_DIR` for ghostty; `KITTY_WINDOW_ID` / `TERM=xterm-kitty` for kitty
- OSC 4 is unreliable in ghostty (partial slot responses leak defaults) — config parse is mandatory
- `Palette.colors` is 16 slots — `.green()` returns slot 2 (muted), `.bright_green()` returns slot 10 (vibrant). Same split for all ANSI colors
- Chart lines and accent UI use the **bright** variants (`bright_green`, `bright_yellow`, `bright_red`, `bright_cyan`) so they pop on dark backgrounds. `surface()`/`label()` stay muted for structural borders and axis labels
- **Theme-fit caveat**: in Alien Blood, `bright_magenta` = `#0058e0` and `bright_blue` = `#00aae0` — both blue. Avoid them if you want to stay inside the green/teal/yellow/orange palette; route through `bright_cyan` (teal) instead
- All tool UIs read via `palette().bright_green()`, `.bright_red()`, etc. — NO hardcoded `Color::White / Yellow / Black / DarkGray` anywhere except `#[cfg(test)]` fixtures and one-off vivid colors that palette can't provide (e.g. audio peak marker uses `Color::Rgb(0xff, 0x55, 0x55)` because Alien Blood's reds are all muted/orange)
- The `sysmon` compositor embeds every tool as a library, so rebuilding standalone tools isn't enough — always rebuild+reinstall `sysmon` too
- **Alien Blood lock-in tests (v5.1.0)**: `shared/src/terminal_theme.rs` tests freeze exact RGBs for all 16 slot accessors + `muted_label()`, `lerp(11,9,0.5)`, and three `mix_with_bg` call sites. `ALIEN_BLOOD_KITTY_CONF` fixture drives parser tests; `alien_blood_palette()` drives derived-helper tests; `test_alien_blood_fixture_matches_parsed` keeps them in sync. Any palette-parsing or blending change must leave those tests green — they ARE the regression gate
- **Brightness helpers**: `rgb_luminance(rgb)` (Rec.709 linear), `Palette::bg_luminance()`, `Palette::is_dark()` (< 0.5), `Palette::contrast_ratio(fg, bg)` (WCAG, 1.0–21.0). Not wired into any consumer yet; reserved for future contrast fixes and for the synthesis branch below
- **Missing-brights synthesis**: `parse_kitty_config` and `parse_ghostty_theme` zero brights at init then call `synthesize_brights()`. Per-slot: any bright slot still `[0,0,0]` gets filled by lightening the basic (dark bg, t=0.35 toward white) or darkening (light bg, t=0.20 toward black); slot 8 leans toward `fg` at t=0.30. No-op on fully-populated palettes (Alien Blood, Catppuccin default). `parse_responses` (OSC path) intentionally unchanged

## Snap packaging
```bash
./snap-dev.sh              # build + install + run astro
./snap-dev.sh <app>        # build + install + run specific app
./snap-dev.sh --install    # skip cargo, just re-snap + install
```
- Uses `snapcraft pack --destructive-mode` (avoids LXD issues)
- Strict confinement: `home` plug can't access dotfiles — use `XDG_*` / `SNAP_*` env vars
- HTTP crates must use `http1_only()` — HTTP/2 ALPN negotiation fails in snap sandbox
- Always set a `user_agent()` — NASA API rejects default UA from snap
