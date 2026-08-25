//! Fetching pages from someone else's server, politely.
//!
//! Two obligations meet here. The first is ZANPOS's own: a URL the AI or a
//! source adapter hands us must not be able to reach the shop's LAN, the hub, or
//! a metadata endpoint. That work is already done and lives in
//! [`crate::ai::tools_web`] — HTTPS only, no credentials in the URL, no host
//! that resolves to a private address, bounded response body. It is reused here
//! rather than reimplemented, because a second SSRF filter is a second one to
//! get wrong.
//!
//! The second is the source's. These are small sites, and one shop wanting
//! better margins is not a reason to make their afternoon worse. Requests go out
//! one at a time per host with a floor between them, that floor honours whatever
//! the site's own `Crawl-delay` asks for, and a source that starts refusing gets
//! backed off rather than retried harder.
//!
//! The client also says who it is. A shop comparing prices for its own shelf is
//! a reasonable thing to be doing, and a site operator who wants to say
//! otherwise should be able to find us in their logs and do so.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::ai::tools_web::{read_bounded_text, resolve_public_host, validate_public_url};
use crate::errors::{AppError, AppResult};

/// Identifies the client and what it is for. Deliberately not a browser string:
/// pretending to be Chrome is how you end up defeating a block someone chose to
/// put up, which is a different activity from reading a public page.
const USER_AGENT: &str = concat!(
    "ZANPOS/",
    env!("CARGO_PKG_VERSION"),
    " (+retail price comparison for a single Bahrain shop)"
);

/// Product pages run large — the captured Akelny page is 330 KB — but a
/// megabyte means we asked for the wrong thing.
const MAX_BYTES: usize = 2 * 1024 * 1024;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

/// Floor between two requests to the same host when the site asks for nothing
/// specific. Slower than any site would demand, which is the point: this runs
/// unattended and nobody is waiting on the hundredth page.
const DEFAULT_GAP: Duration = Duration::from_millis(1_500);

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_default()
    })
}

/// Last request per host, so the gap is enforced across every adapter rather
/// than per adapter — two sources on one domain still queue behind each other.
fn last_seen() -> &'static Mutex<HashMap<String, Instant>> {
    static SEEN: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Wait out the remaining gap for this host, then mark it as just used.
async fn pace(host: &str, gap: Duration) {
    let wait = {
        let mut seen = last_seen().lock().await;
        let now = Instant::now();
        let wait = match seen.get(host) {
            Some(previous) => gap.checked_sub(now.duration_since(*previous)),
            None => None,
        };
        // Recorded before the sleep, not after, so several callers arriving at
        // once space themselves out instead of all waking to the same slot.
        seen.insert(host.to_string(), now + wait.unwrap_or_default());
        wait
    };
    if let Some(wait) = wait {
        tokio::time::sleep(wait).await;
    }
}

/// Fetch a page as text, subject to both obligations above.
///
/// `crawl_delay` is the source's declared `Crawl-delay` where it publishes one;
/// the larger of that and the default floor is used, so a site asking for more
/// room gets it and a site asking for less does not shrink ours.
pub async fn fetch_text(url: &str, crawl_delay: Option<Duration>) -> AppResult<String> {
    let parsed = validate_public_url(url)?;
    resolve_public_host(&parsed).await?;
    let host = parsed.host_str().unwrap_or_default().to_string();

    pace(&host, crawl_delay.unwrap_or(DEFAULT_GAP).max(DEFAULT_GAP)).await;

    let response = client()
        .get(parsed.clone())
        .header("accept", "text/html,application/xhtml+xml,application/xml;q=0.9")
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("{host} could not be reached: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        // Named rather than swallowed. 403 is how LuLu says "not this client",
        // and a source that starts answering that way needs to show up as a
        // status an operator can read, not as a product nobody sells.
        return Err(AppError::Internal(format!(
            "{host} answered {} for {}",
            status.as_u16(),
            parsed.path()
        )));
    }

    read_bounded_text(response, MAX_BYTES, &format!("{host} response")).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reused guards are the only SSRF filter in the product. A regression
    /// here would let a source adapter reach the hub on the shop's own LAN.
    #[tokio::test]
    async fn private_and_non_https_targets_are_refused_before_any_request() {
        for url in [
            "http://127.0.0.1/admin",
            "https://localhost/x",
            "http://192.168.1.10/",
            "https://user:pass@example.com/",
            "ftp://example.com/",
        ] {
            assert!(fetch_text(url, None).await.is_err(), "accepted {url}");
        }
    }

    /// A site asking for more room than our floor gets it; one asking for less
    /// does not shrink ours.
    #[test]
    fn the_declared_crawl_delay_can_only_widen_the_gap() {
        let widened = Duration::from_secs(5).max(DEFAULT_GAP);
        let narrowed = Duration::from_millis(100).max(DEFAULT_GAP);

        assert_eq!(widened, Duration::from_secs(5));
        assert_eq!(narrowed, DEFAULT_GAP);
    }

    /// Recording the slot before sleeping is what makes concurrent callers
    /// queue. Marking it afterwards would let them all wake into the same one.
    /// A short gap so the test spends milliseconds rather than seconds; the
    /// behaviour under test is the queueing, not the size of the floor.
    #[tokio::test]
    async fn two_requests_to_one_host_do_not_share_a_slot() {
        let gap = Duration::from_millis(60);
        let started = std::time::Instant::now();
        pace("queued.test", gap).await;
        pace("queued.test", gap).await;

        assert!(started.elapsed() >= gap, "{:?}", started.elapsed());
    }

    #[tokio::test]
    async fn a_different_host_is_not_made_to_wait() {
        let gap = Duration::from_millis(400);
        pace("first.test", gap).await;
        let started = std::time::Instant::now();
        pace("second.test", gap).await;

        assert!(started.elapsed() < gap, "{:?}", started.elapsed());
    }

    /// A client that pretends to be a browser is defeating a block rather than
    /// reading a public page, and the difference should stay visible in a log.
    #[test]
    fn the_client_identifies_itself_rather_than_impersonating_a_browser() {
        assert!(USER_AGENT.starts_with("ZANPOS/"));
        for browser in ["Mozilla", "Chrome", "Safari", "AppleWebKit"] {
            assert!(!USER_AGENT.contains(browser), "{USER_AGENT}");
        }
    }
}
