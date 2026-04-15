use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Deserialize;

const APOD_BASE_URL: &str = "https://api.nasa.gov/planetary/apod";
const MAX_IMAGE_DIM: u32 = 800;

fn cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".cache").join("sysmon").join("astro")
}

fn ensure_cache_dir() -> PathBuf {
    let dir = cache_dir();
    let _ = fs::create_dir_all(&dir);
    dir
}

fn cache_json_path(date: &str) -> PathBuf {
    cache_dir().join(format!("{date}.json"))
}

fn cache_image_path(date: &str) -> PathBuf {
    cache_dir().join(format!("{date}.img"))
}

fn load_cached_entry(date: &str) -> Option<ApodEntry> {
    let json_path = cache_json_path(date);
    let json = fs::read_to_string(&json_path).ok()?;
    let resp: ApodResponse = serde_json::from_str(&json).ok()?;

    let (image, ascii_art) = load_cached_image(date);

    Some(ApodEntry {
        title: resp.title,
        explanation: resp.explanation,
        date: resp.date,
        copyright: resp.copyright,
        media_type: resp.media_type,
        image,
        ascii_art,
    })
}

fn load_cached_image(date: &str) -> (Option<DecodedImage>, Option<String>) {
    let img_path = cache_image_path(date);
    let bytes = match fs::read(&img_path) {
        Ok(b) => b,
        Err(_) => return (None, None),
    };
    let img = match image::load_from_memory(&bytes) {
        Ok(i) => i,
        Err(_) => return (None, None),
    };
    let img = if img.width() > MAX_IMAGE_DIM || img.height() > MAX_IMAGE_DIM {
        img.thumbnail(MAX_IMAGE_DIM, MAX_IMAGE_DIM)
    } else {
        img
    };
    let ascii_art = generate_ascii_art(&img);
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels: Vec<[u8; 3]> = rgb.pixels().map(|p| p.0).collect();
    let decoded = DecodedImage { width: w, height: h, pixels };
    (Some(decoded), Some(ascii_art))
}

fn save_cached_json(date: &str, resp: &ApodResponse) {
    let dir = ensure_cache_dir();
    let path = dir.join(format!("{date}.json"));
    if let Ok(json) = serde_json::to_string(resp) {
        let _ = fs::write(path, json);
    }
}

fn save_cached_image(date: &str, bytes: &[u8]) {
    let dir = ensure_cache_dir();
    let path = dir.join(format!("{date}.img"));
    let _ = fs::write(path, bytes);
}

#[derive(Debug, Deserialize, serde::Serialize, Clone)]
pub struct ApodResponse {
    pub title: String,
    pub explanation: String,
    pub url: String,
    #[serde(default)]
    pub hdurl: Option<String>,
    pub date: String,
    pub media_type: String,
    #[serde(default)]
    pub copyright: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 3]>,
}

#[derive(Debug, Clone)]
pub struct ApodEntry {
    pub title: String,
    pub explanation: String,
    pub date: String,
    pub copyright: Option<String>,
    pub media_type: String,
    pub image: Option<DecodedImage>,
    pub ascii_art: Option<String>,
}

pub struct FetchState {
    pub entries: Option<Vec<ApodEntry>>,
    pub error: Option<String>,
    pub entries_updated: bool,
    pub refresh_ms: u64,
    pub should_stop: bool,
    pub api_key: String,
}

impl FetchState {
    pub fn new(refresh_ms: u64, api_key: String) -> Self {
        Self {
            entries: None,
            error: None,
            entries_updated: false,
            refresh_ms,
            should_stop: false,
            api_key,
        }
    }
}

pub fn build_apod_url(api_key: &str, days: u32) -> String {
    let (start, end) = date_range(days);
    format!(
        "{APOD_BASE_URL}?api_key={api_key}&start_date={start}&end_date={end}"
    )
}

fn date_range(days: u32) -> (String, String) {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let end = format_date(now_secs);
    let start_secs = now_secs.saturating_sub((days.saturating_sub(1) as u64) * 86400);
    let start = format_date(start_secs);
    (start, end)
}

fn format_date(epoch_secs: u64) -> String {
    let days_since_epoch = epoch_secs / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02}")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Civil days from epoch algorithm
    days += 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

pub fn parse_apod_response(json: &str) -> Result<Vec<ApodResponse>> {
    // Check for API error responses (rate limit, bad key, etc.)
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json) {
        if let Some(err) = val.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown API error");
            anyhow::bail!("{msg}");
        }
    }

    // APOD returns a single object for one date or an array for date ranges
    if json.trim_start().starts_with('[') {
        Ok(serde_json::from_str(json)?)
    } else {
        let single: ApodResponse = serde_json::from_str(json)?;
        Ok(vec![single])
    }
}

struct FetchedImage {
    decoded: DecodedImage,
    dynamic: image::DynamicImage,
    raw_bytes: Vec<u8>,
}

fn fetch_image(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Option<FetchedImage> {
    let raw_bytes = client.get(url).send().ok()?.bytes().ok()?.to_vec();
    let img = image::load_from_memory(&raw_bytes).ok()?;

    let img = if img.width() > MAX_IMAGE_DIM || img.height() > MAX_IMAGE_DIM {
        img.thumbnail(MAX_IMAGE_DIM, MAX_IMAGE_DIM)
    } else {
        img
    };

    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels: Vec<[u8; 3]> = rgb.pixels().map(|p| p.0).collect();
    Some(FetchedImage {
        decoded: DecodedImage { width: w, height: h, pixels },
        dynamic: img,
        raw_bytes,
    })
}

fn generate_ascii_art(img: &image::DynamicImage) -> String {
    let config = artem::config::ConfigBuilder::new()
        .target(artem::config::TargetType::File)
        .build();
    artem::convert(img.clone(), &config)
}

fn date_strings(days: u32) -> Vec<String> {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    (0..days)
        .map(|i| {
            let secs = now_secs.saturating_sub(i as u64 * 86400);
            format_date(secs)
        })
        .rev()
        .collect()
}

fn do_fetch(
    client: &reqwest::blocking::Client,
    shared: &Arc<Mutex<FetchState>>,
    api_key: &str,
) {
    let url = build_apod_url(api_key, 7);
    match client.get(&url).send().and_then(|r| r.text()) {
        Ok(body) => match parse_apod_response(&body) {
            Ok(responses) => {
                for resp in &responses {
                    save_cached_json(&resp.date, resp);
                }

                let mut entries: Vec<ApodEntry> = responses
                    .iter()
                    .map(|r| {
                        let (image, ascii_art) = load_cached_image(&r.date);
                        ApodEntry {
                            title: r.title.clone(),
                            explanation: r.explanation.clone(),
                            date: r.date.clone(),
                            copyright: r.copyright.clone(),
                            media_type: r.media_type.clone(),
                            image,
                            ascii_art,
                        }
                    })
                    .collect();
                {
                    let mut state = shared.lock().unwrap();
                    state.entries = Some(entries.clone());
                    state.entries_updated = true;
                    state.error = None;
                }

                for (i, resp) in responses.iter().enumerate() {
                    if shared.lock().unwrap().should_stop {
                        break;
                    }
                    if resp.media_type != "image" || entries[i].image.is_some() {
                        continue;
                    }
                    if let Some(f) = fetch_image(client, &resp.url) {
                        save_cached_image(&resp.date, &f.raw_bytes);
                        entries[i].ascii_art = Some(generate_ascii_art(&f.dynamic));
                        entries[i].image = Some(f.decoded);
                        let mut state = shared.lock().unwrap();
                        state.entries = Some(entries.clone());
                        state.entries_updated = true;
                    }
                }
            }
            Err(e) => {
                let mut state = shared.lock().unwrap();
                state.error = Some(format!("Parse: {e}"));
            }
        },
        Err(e) => {
            let mut state = shared.lock().unwrap();
            if state.entries.is_none() {
                // Show full error chain for diagnosis
                state.error = Some(format!("Fetch: {e:#}"));
            }
        }
    }
}

pub fn spawn_fetcher(shared: Arc<Mutex<FetchState>>) -> thread::JoinHandle<()> {
    // Load cache immediately
    let dates = date_strings(7);
    let cached: Vec<ApodEntry> = dates
        .iter()
        .filter_map(|d| load_cached_entry(d))
        .collect();
    if !cached.is_empty() {
        let mut state = shared.lock().unwrap();
        state.entries = Some(cached);
        state.entries_updated = true;
    }

    // Build client and do initial fetch on the main thread where networking works.
    // reqwest::blocking creates an internal tokio runtime whose DNS resolver
    // can fail when the client is constructed inside thread::spawn.
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("failed to build HTTP client");

    let api_key = shared.lock().unwrap().api_key.clone();
    do_fetch(&client, &shared, &api_key);

    thread::spawn(move || {
        let mut last_fetch = Instant::now();

        loop {
            let (refresh_ms, should_stop, api_key) = {
                let state = shared.lock().unwrap();
                (state.refresh_ms, state.should_stop, state.api_key.clone())
            };

            if should_stop {
                break;
            }

            if last_fetch.elapsed() >= Duration::from_millis(refresh_ms) {
                do_fetch(&client, &shared, &api_key);
                last_fetch = Instant::now();
            }

            thread::sleep(Duration::from_millis(500));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APOD_SINGLE: &str = r#"{
        "title": "The Horsehead Nebula",
        "explanation": "A dark molecular cloud shaped like a horse head.",
        "url": "https://apod.nasa.gov/apod/image/horsehead_small.jpg",
        "hdurl": "https://apod.nasa.gov/apod/image/horsehead.jpg",
        "date": "2026-04-15",
        "media_type": "image",
        "copyright": "NASA/ESA"
    }"#;

    const APOD_ARRAY: &str = r#"[
        {
            "title": "The Horsehead Nebula",
            "explanation": "A dark molecular cloud.",
            "url": "https://example.com/img1.jpg",
            "date": "2026-04-14",
            "media_type": "image"
        },
        {
            "title": "Mars Opposition",
            "explanation": "Mars at its closest approach.",
            "url": "https://example.com/vid1.mp4",
            "date": "2026-04-15",
            "media_type": "video"
        }
    ]"#;

    #[test]
    fn test_parse_single_entry() {
        let entries = parse_apod_response(APOD_SINGLE).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "The Horsehead Nebula");
        assert_eq!(entries[0].media_type, "image");
        assert_eq!(entries[0].copyright, Some("NASA/ESA".to_string()));
    }

    #[test]
    fn test_parse_array() {
        let entries = parse_apod_response(APOD_ARRAY).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "The Horsehead Nebula");
        assert_eq!(entries[1].title, "Mars Opposition");
        assert_eq!(entries[1].media_type, "video");
    }

    #[test]
    fn test_parse_empty_array() {
        let entries = parse_apod_response("[]").unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_rate_limit_error() {
        let json = r#"{"error":{"code":"OVER_RATE_LIMIT","message":"You have exceeded your rate limit."}}"#;
        let err = parse_apod_response(json).unwrap_err();
        assert!(err.to_string().contains("rate limit"));
    }

    #[test]
    fn test_parse_malformed() {
        assert!(parse_apod_response("not json").is_err());
    }

    #[test]
    fn test_parse_missing_optional_fields() {
        let json = r#"{
            "title": "Test",
            "explanation": "Test explanation",
            "url": "https://example.com/img.jpg",
            "date": "2026-04-15",
            "media_type": "image"
        }"#;
        let entries = parse_apod_response(json).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].hdurl.is_none());
        assert!(entries[0].copyright.is_none());
    }

    #[test]
    fn test_build_apod_url_contains_key() {
        let url = build_apod_url("DEMO_KEY", 7);
        assert!(url.contains("api_key=DEMO_KEY"));
        assert!(url.contains("start_date="));
        assert!(url.contains("end_date="));
    }

    #[test]
    fn test_build_apod_url_single_day() {
        let url = build_apod_url("DEMO_KEY", 1);
        // start_date and end_date should be the same
        let start = url.split("start_date=").nth(1).unwrap().split('&').next().unwrap();
        let end = url.split("end_date=").nth(1).unwrap().split('&').next().unwrap();
        assert_eq!(start, end);
    }

    #[test]
    fn test_format_date() {
        // 2026-01-01 00:00:00 UTC = 1767225600
        assert_eq!(format_date(1767225600), "2026-01-01");
    }

    #[test]
    fn test_format_date_epoch() {
        assert_eq!(format_date(0), "1970-01-01");
    }

    #[test]
    fn test_decoded_image_pixel_count() {
        let img = DecodedImage {
            width: 4,
            height: 3,
            pixels: vec![[255, 0, 0]; 12],
        };
        assert_eq!(img.pixels.len(), (img.width * img.height) as usize);
    }
}
