use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Deserialize;

const EVENTS_BASE_URL: &str = "https://gamma-api.polymarket.com/events";
const HISTORY_BASE_URL: &str = "https://clob.polymarket.com/prices-history";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    All,
    Politics,
    Sports,
    Crypto,
    Geopolitics,
    Esports,
    Elections,
    Science,
    Ai,
    Business,
}

impl Topic {
    pub const ALL: &[Topic] = &[
        Topic::All,
        Topic::Politics,
        Topic::Sports,
        Topic::Crypto,
        Topic::Geopolitics,
        Topic::Esports,
        Topic::Elections,
        Topic::Science,
        Topic::Ai,
        Topic::Business,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Topic::All => "All",
            Topic::Politics => "Politics",
            Topic::Sports => "Sports",
            Topic::Crypto => "Crypto",
            Topic::Geopolitics => "Geopolitics",
            Topic::Esports => "Esports",
            Topic::Elections => "Elections",
            Topic::Science => "Science",
            Topic::Ai => "AI",
            Topic::Business => "Business",
        }
    }

    fn tag_slug(self) -> Option<&'static str> {
        match self {
            Topic::All => None,
            Topic::Politics => Some("politics"),
            Topic::Sports => Some("sports"),
            Topic::Crypto => Some("crypto"),
            Topic::Geopolitics => Some("geopolitics"),
            Topic::Esports => Some("esports"),
            Topic::Elections => Some("elections"),
            Topic::Science => Some("science"),
            Topic::Ai => Some("ai"),
            Topic::Business => Some("business"),
        }
    }

    pub fn next(self) -> Topic {
        let all = Topic::ALL;
        let idx = all.iter().position(|&t| t == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    pub fn prev(self) -> Topic {
        let all = Topic::ALL;
        let idx = all.iter().position(|&t| t == self).unwrap_or(0);
        if idx == 0 { all[all.len() - 1] } else { all[idx - 1] }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    MonitoringTheSituation,
    Volume24h,
    Volume,
    Liquidity,
    Newest,
    Competitive,
}

impl SortOrder {
    pub const ALL: &[SortOrder] = &[
        SortOrder::MonitoringTheSituation,
        SortOrder::Volume24h,
        SortOrder::Volume,
        SortOrder::Liquidity,
        SortOrder::Newest,
        SortOrder::Competitive,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SortOrder::MonitoringTheSituation => "Monitoring the Situation",
            SortOrder::Volume24h => "24h Volume",
            SortOrder::Volume => "All-time Vol",
            SortOrder::Liquidity => "Liquidity",
            SortOrder::Newest => "Newest",
            SortOrder::Competitive => "Competitive",
        }
    }

    fn api_param(self) -> &'static str {
        match self {
            SortOrder::MonitoringTheSituation => "volume24hr",
            SortOrder::Volume24h => "volume24hr",
            SortOrder::Volume => "volume",
            SortOrder::Liquidity => "liquidityClob",
            SortOrder::Newest => "startDate",
            SortOrder::Competitive => "competitive",
        }
    }

    fn api_limit(self) -> u32 {
        match self {
            SortOrder::MonitoringTheSituation => 50,
            _ => 20,
        }
    }

    pub fn next(self) -> SortOrder {
        let all = SortOrder::ALL;
        let idx = all.iter().position(|&s| s == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }
}

pub fn build_events_url(topic: Topic, sort: SortOrder) -> String {
    let mut url = format!(
        "{EVENTS_BASE_URL}?active=true&closed=false&order={}&ascending=false&limit={}",
        sort.api_param(),
        sort.api_limit()
    );
    if let Some(slug) = topic.tag_slug() {
        url.push_str("&tag_slug=");
        url.push_str(slug);
    }
    url
}

#[derive(Debug, Deserialize)]
pub struct GammaEvent {
    #[allow(dead_code)]
    pub title: String,
    #[serde(default)]
    pub markets: Vec<GammaMarket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GammaMarket {
    pub question: String,
    #[serde(default)]
    pub outcome_prices: String,
    #[serde(default)]
    pub clob_token_ids: String,
    #[serde(default)]
    pub volume24hr: f64,
}

#[derive(Debug, Deserialize)]
pub struct PriceHistoryResponse {
    pub history: Vec<PricePoint>,
}

#[derive(Debug, Deserialize)]
pub struct PricePoint {
    #[allow(dead_code)]
    pub t: i64,
    pub p: f64,
}

#[derive(Debug, Clone)]
pub struct SubMarket {
    pub question: String,
    pub yes_price: f64,
    pub volume_24h: f64,
    pub yes_token_id: String,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub title: String,
    pub markets: Vec<SubMarket>,
    pub total_volume_24h: f64,
}

impl Event {
    pub fn lead_market(&self) -> Option<&SubMarket> {
        self.markets.iter().max_by(|a, b| {
            a.volume_24h.partial_cmp(&b.volume_24h).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    pub fn market_count(&self) -> usize {
        self.markets.len()
    }
}

pub struct FetchState {
    pub events: Option<Vec<Event>>,
    pub price_history: Option<Vec<(f64, f64)>>,
    pub requested_token_id: Option<String>,
    pub error: Option<String>,
    pub events_updated: bool,
    pub history_updated: bool,
    pub refresh_ms: u64,
    pub should_stop: bool,
    pub topic: Topic,
    pub sort_order: SortOrder,
    pub filter_changed: bool,
}

impl FetchState {
    pub fn new(refresh_ms: u64) -> Self {
        Self {
            events: None,
            price_history: None,
            requested_token_id: None,
            error: None,
            events_updated: false,
            history_updated: false,
            refresh_ms,
            should_stop: false,
            topic: Topic::Geopolitics,
            sort_order: SortOrder::MonitoringTheSituation,
            filter_changed: false,
        }
    }
}

pub fn parse_yes_price(outcome_prices: &str) -> Option<f64> {
    let prices: Vec<String> = serde_json::from_str(outcome_prices).ok()?;
    let yes_price: f64 = prices.first()?.parse().ok()?;
    Some(yes_price * 100.0)
}

pub fn parse_yes_token_id(clob_token_ids: &str) -> Option<String> {
    let ids: Vec<String> = serde_json::from_str(clob_token_ids).ok()?;
    ids.into_iter().next()
}

pub fn parse_events_response(json: &str) -> Result<Vec<Event>> {
    let raw_events: Vec<GammaEvent> = serde_json::from_str(json)?;
    let mut events = Vec::new();
    for raw in raw_events {
        let sub_markets: Vec<SubMarket> = raw
            .markets
            .iter()
            .filter_map(|gm| {
                let yes_price = parse_yes_price(&gm.outcome_prices).unwrap_or(0.0);
                let token_id = parse_yes_token_id(&gm.clob_token_ids)?;
                if token_id.is_empty() {
                    return None;
                }
                Some(SubMarket {
                    question: gm.question.clone(),
                    yes_price,
                    volume_24h: gm.volume24hr,
                    yes_token_id: token_id,
                })
            })
            .collect();

        if sub_markets.is_empty() {
            continue;
        }

        let total_volume_24h = sub_markets.iter().map(|m| m.volume_24h).sum();
        events.push(Event {
            title: raw.title,
            markets: sub_markets,
            total_volume_24h,
        });
    }
    Ok(events)
}

pub fn parse_price_history(json: &str) -> Result<Vec<(f64, f64)>> {
    let resp: PriceHistoryResponse = serde_json::from_str(json)?;
    let data: Vec<(f64, f64)> = resp
        .history
        .iter()
        .enumerate()
        .map(|(idx, point)| (idx as f64, point.p * 100.0))
        .collect();
    Ok(data)
}

pub fn human_volume(volume: f64) -> String {
    if volume >= 1_000_000.0 {
        format!("${:.1}M", volume / 1_000_000.0)
    } else if volume >= 1_000.0 {
        format!("${:.0}K", volume / 1_000.0)
    } else {
        format!("${:.0}", volume)
    }
}

fn clickbait_score(event: &Event) -> f64 {
    let lead_price = event
        .lead_market()
        .map(|m| m.yes_price)
        .unwrap_or(50.0);
    let competitiveness = 1.0 - (lead_price - 50.0).abs() / 50.0;
    competitiveness * (1.0 + event.total_volume_24h).ln()
}

fn fetch_events(
    client: &reqwest::blocking::Client,
    topic: Topic,
    sort_order: SortOrder,
) -> Result<Vec<Event>> {
    let url = build_events_url(topic, sort_order);
    let body = client.get(&url).send()?.text()?;
    let mut events = parse_events_response(&body)?;
    if sort_order == SortOrder::MonitoringTheSituation {
        events.sort_by(|a, b| clickbait_score(b).partial_cmp(&clickbait_score(a)).unwrap_or(std::cmp::Ordering::Equal));
        events.truncate(20);
    }
    Ok(events)
}

fn fetch_price_history(
    client: &reqwest::blocking::Client,
    token_id: &str,
) -> Result<Vec<(f64, f64)>> {
    let url = format!("{HISTORY_BASE_URL}?market={token_id}&interval=1w&fidelity=60");
    let body = client.get(&url).send()?.text()?;
    parse_price_history(&body)
}

pub fn spawn_fetcher(shared: Arc<Mutex<FetchState>>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");

        let mut last_events_fetch = Instant::now() - Duration::from_secs(999);
        let mut last_token_id: Option<String> = None;

        loop {
            let (refresh_ms, should_stop, requested_token, topic, sort_order, filter_changed) = {
                let mut state = shared.lock().unwrap();
                let changed = state.filter_changed;
                if changed {
                    state.filter_changed = false;
                }
                (
                    state.refresh_ms,
                    state.should_stop,
                    state.requested_token_id.clone(),
                    state.topic,
                    state.sort_order,
                    changed,
                )
            };

            if should_stop {
                break;
            }

            let should_fetch_events = filter_changed
                || last_events_fetch.elapsed() >= Duration::from_millis(refresh_ms);

            if should_fetch_events {
                match fetch_events(&client, topic, sort_order) {
                    Ok(events) => {
                        let mut state = shared.lock().unwrap();
                        state.events = Some(events);
                        state.events_updated = true;
                        state.error = None;
                    }
                    Err(err) => {
                        let mut state = shared.lock().unwrap();
                        state.error = Some(format!("Events: {err}"));
                    }
                }
                last_events_fetch = Instant::now();
            }

            if requested_token != last_token_id {
                if let Some(ref token_id) = requested_token {
                    match fetch_price_history(&client, token_id) {
                        Ok(history) => {
                            let mut state = shared.lock().unwrap();
                            state.price_history = Some(history);
                            state.history_updated = true;
                        }
                        Err(err) => {
                            let mut state = shared.lock().unwrap();
                            state.error = Some(format!("History: {err}"));
                        }
                    }
                }
                last_token_id = requested_token;
            }

            thread::sleep(Duration::from_millis(500));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_yes_price_normal() {
        let result = parse_yes_price(r#"["0.55","0.45"]"#);
        assert!((result.unwrap() - 55.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_yes_price_high() {
        let result = parse_yes_price(r#"["0.92","0.08"]"#);
        assert!((result.unwrap() - 92.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_yes_price_empty() {
        assert!(parse_yes_price("").is_none());
    }

    #[test]
    fn test_parse_yes_price_empty_array() {
        assert!(parse_yes_price("[]").is_none());
    }

    #[test]
    fn test_parse_yes_price_garbage() {
        assert!(parse_yes_price("not json").is_none());
    }

    #[test]
    fn test_parse_yes_token_id_normal() {
        let result = parse_yes_token_id(r#"["token_yes","token_no"]"#);
        assert_eq!(result.unwrap(), "token_yes");
    }

    #[test]
    fn test_parse_yes_token_id_empty() {
        assert!(parse_yes_token_id("").is_none());
    }

    #[test]
    fn test_parse_yes_token_id_empty_array() {
        assert!(parse_yes_token_id("[]").is_none());
    }

    const EVENTS_JSON: &str = r#"[
        {
            "title": "US Election",
            "markets": [
                {
                    "question": "Will candidate A win?",
                    "outcomePrices": "[\"0.65\",\"0.35\"]",
                    "clobTokenIds": "[\"tok_yes_1\",\"tok_no_1\"]",
                    "volume24hr": 1500000.0
                },
                {
                    "question": "Will candidate B win?",
                    "outcomePrices": "[\"0.30\",\"0.70\"]",
                    "clobTokenIds": "[\"tok_yes_2\",\"tok_no_2\"]",
                    "volume24hr": 800000.0
                }
            ]
        },
        {
            "title": "Crypto",
            "markets": [
                {
                    "question": "BTC above 100K?",
                    "outcomePrices": "[\"0.42\",\"0.58\"]",
                    "clobTokenIds": "[\"tok_yes_3\",\"tok_no_3\"]",
                    "volume24hr": 250000.0
                }
            ]
        }
    ]"#;

    #[test]
    fn test_parse_events_response() {
        let events = parse_events_response(EVENTS_JSON).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].title, "US Election");
        assert_eq!(events[0].markets.len(), 2);
        assert_eq!(events[0].markets[0].question, "Will candidate A win?");
        assert!((events[0].markets[0].yes_price - 65.0).abs() < 0.01);
        assert_eq!(events[0].markets[0].yes_token_id, "tok_yes_1");
        assert_eq!(events[1].title, "Crypto");
        assert_eq!(events[1].markets.len(), 1);
    }

    #[test]
    fn test_parse_events_total_volume() {
        let events = parse_events_response(EVENTS_JSON).unwrap();
        assert!((events[0].total_volume_24h - 2_300_000.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_events_lead_market() {
        let events = parse_events_response(EVENTS_JSON).unwrap();
        let lead = events[0].lead_market().unwrap();
        assert_eq!(lead.question, "Will candidate A win?");
        assert!((lead.volume_24h - 1_500_000.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_events_response_empty() {
        let events = parse_events_response("[]").unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_parse_events_response_malformed() {
        let result = parse_events_response("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_events_skips_event_with_no_valid_markets() {
        let json = r#"[{
            "title": "Test",
            "markets": [{
                "question": "Q?",
                "outcomePrices": "[\"0.5\",\"0.5\"]",
                "clobTokenIds": "",
                "volume24hr": 100.0
            }]
        }]"#;
        let events = parse_events_response(json).unwrap();
        assert!(events.is_empty());
    }

    const HISTORY_JSON: &str = r#"{
        "history": [
            {"t": 1700000000, "p": 0.45},
            {"t": 1700003600, "p": 0.50},
            {"t": 1700007200, "p": 0.55},
            {"t": 1700010800, "p": 0.52}
        ]
    }"#;

    #[test]
    fn test_parse_price_history() {
        let data = parse_price_history(HISTORY_JSON).unwrap();
        assert_eq!(data.len(), 4);
        assert!((data[0].0 - 0.0).abs() < 0.01);
        assert!((data[0].1 - 45.0).abs() < 0.01);
        assert!((data[2].0 - 2.0).abs() < 0.01);
        assert!((data[2].1 - 55.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_price_history_empty() {
        let data = parse_price_history(r#"{"history": []}"#).unwrap();
        assert!(data.is_empty());
    }

    #[test]
    fn test_parse_price_history_malformed() {
        assert!(parse_price_history("bad json").is_err());
    }

    #[test]
    fn test_human_volume_millions() {
        assert_eq!(human_volume(1_500_000.0), "$1.5M");
    }

    #[test]
    fn test_human_volume_thousands() {
        assert_eq!(human_volume(800_000.0), "$800K");
    }

    #[test]
    fn test_human_volume_small() {
        assert_eq!(human_volume(500.0), "$500");
    }

    #[test]
    fn test_human_volume_exact_million() {
        assert_eq!(human_volume(1_000_000.0), "$1.0M");
    }

    #[test]
    fn test_build_events_url_all_volume24h() {
        let url = build_events_url(Topic::All, SortOrder::Volume24h);
        assert!(url.contains("order=volume24hr"));
        assert!(!url.contains("tag_slug"));
    }

    #[test]
    fn test_build_events_url_sports_liquidity() {
        let url = build_events_url(Topic::Sports, SortOrder::Liquidity);
        assert!(url.contains("order=liquidityClob"));
        assert!(url.contains("tag_slug=sports"));
    }

    #[test]
    fn test_build_events_url_crypto_newest() {
        let url = build_events_url(Topic::Crypto, SortOrder::Newest);
        assert!(url.contains("order=startDate"));
        assert!(url.contains("tag_slug=crypto"));
    }

    #[test]
    fn test_topic_next_cycles() {
        let mut topic = Topic::All;
        for _ in 0..Topic::ALL.len() {
            topic = topic.next();
        }
        assert_eq!(topic, Topic::All);
    }

    #[test]
    fn test_topic_prev_cycles() {
        let mut topic = Topic::All;
        topic = topic.prev();
        assert_eq!(topic, *Topic::ALL.last().unwrap());
        for _ in 0..Topic::ALL.len() - 1 {
            topic = topic.prev();
        }
        assert_eq!(topic, Topic::All);
    }

    #[test]
    fn test_sort_order_next_cycles() {
        let mut sort = SortOrder::Volume24h;
        for _ in 0..SortOrder::ALL.len() {
            sort = sort.next();
        }
        assert_eq!(sort, SortOrder::Volume24h);
    }

    #[test]
    fn test_topic_labels_not_empty() {
        for topic in Topic::ALL {
            assert!(!topic.label().is_empty());
        }
    }

    #[test]
    fn test_sort_order_labels_not_empty() {
        for sort in SortOrder::ALL {
            assert!(!sort.label().is_empty());
        }
    }
}
