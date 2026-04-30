//! Tiny User-Agent parser used to derive `os` / `browser` tags on ingest.
//!
//! Sentry cloud does this server-side; errex must too or the SPA renders no
//! environment tags. We deliberately don't pull `uap-rs` or `woothee` — both
//! ship megabyte-scale regex databases and errex's "lightweight first"
//! principle treats RAM as a budget, not an afterthought. The browsers and
//! operating systems that account for >99% of real traffic are a short
//! list, and the cost of missing an oddball UA is one missing tag — not a
//! lost event.
//!
//! Order matters in `detect_browser`: Edge / Opera / Brave all advertise
//! themselves as "Chrome" too, so we have to check the marker for those
//! shells before falling through to the plain Chrome match.
//!
//! Returned shape mirrors what Sentry SDKs emit when they DO send tags
//! (e.g. mobile SDKs): both the bare key (`browser`, `os`) carrying
//! `"<Name> <Version>"` and the `.name` variant carrying just the family.
//! `eventDetail.ts::dedupTags` collapses the redundancy at render time.
use errex_proto::Event;
use serde_json::{Map, Value};

/// Augments `event.tags` in place with `os.*` / `browser.*` derived from
/// the User-Agent header in `event.request`. No-op if the request or UA is
/// missing. Existing tag keys win — never overwrite an SDK-supplied value.
pub fn augment_tags_from_user_agent(event: &mut Event) {
    let Some(ua) = extract_user_agent(event) else {
        return;
    };
    let derived = parse_user_agent(&ua);
    if derived.is_empty() {
        return;
    }

    let tags = ensure_tags_object(&mut event.tags);
    for (k, v) in derived {
        tags.entry(k.to_string()).or_insert(Value::String(v));
    }
}

fn extract_user_agent(event: &Event) -> Option<String> {
    let headers = event.request.as_ref()?.get("headers")?.as_object()?;
    // Header names are case-insensitive — Sentry browser SDK emits
    // `"User-Agent"` but Node ones occasionally lowercase the key.
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("user-agent") {
            return value.as_str().map(|s| s.to_string());
        }
    }
    None
}

fn ensure_tags_object(tags: &mut Option<Value>) -> &mut Map<String, Value> {
    if !matches!(tags, Some(Value::Object(_))) {
        // Tags arrived missing, or as the legacy array-of-pairs shape (or
        // something weirder). Replace with an object — eventDetail.ts
        // already normalizes both shapes for display, but we need a
        // writable map here. Preserving an unrecognized shape would
        // require more code than the edge case justifies.
        *tags = Some(Value::Object(Map::new()));
    }
    tags.as_mut().unwrap().as_object_mut().unwrap()
}

fn parse_user_agent(ua: &str) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some((name, version)) = detect_os(ua) {
        out.push(("os.name", name.to_string()));
        out.push((
            "os",
            match version {
                Some(v) => format!("{name} {v}"),
                None => name.to_string(),
            },
        ));
    }
    if let Some((name, version)) = detect_browser(ua) {
        out.push(("browser.name", name.to_string()));
        out.push((
            "browser",
            match version {
                Some(v) => format!("{name} {v}"),
                None => name.to_string(),
            },
        ));
    }
    out
}

fn detect_os(ua: &str) -> Option<(&'static str, Option<String>)> {
    if let Some(rest) = ua.split_once("Windows NT ").map(|(_, r)| r) {
        let version = rest.split([';', ')']).next().map(parse_windows_nt);
        return Some(("Windows", version.flatten()));
    }
    if ua.contains("Android ") {
        let version = after_token(ua, "Android ").map(token_until_semi_or_paren);
        return Some(("Android", version));
    }
    if ua.contains("iPhone") || ua.contains("iPad") || ua.contains("iPod") {
        let version = after_token(ua, "OS ")
            .map(token_until_space_or_paren)
            .map(|v| v.replace('_', "."));
        return Some(("iOS", version));
    }
    if ua.contains("Mac OS X") {
        let version = after_token(ua, "Mac OS X ")
            .map(token_until_space_or_paren)
            .map(|v| v.replace('_', "."));
        return Some(("macOS", version));
    }
    if ua.contains("CrOS") {
        return Some(("Chrome OS", None));
    }
    if ua.contains("Linux") {
        return Some(("Linux", None));
    }
    None
}

fn detect_browser(ua: &str) -> Option<(&'static str, Option<String>)> {
    // Order: shells that masquerade as Chrome first.
    if let Some(v) = after_token(ua, "Edg/") {
        return Some(("Edge", Some(token_until_space_or_paren(v))));
    }
    if let Some(v) = after_token(ua, "OPR/") {
        return Some(("Opera", Some(token_until_space_or_paren(v))));
    }
    // Firefox identifies itself uniquely.
    if let Some(v) = after_token(ua, "Firefox/") {
        return Some(("Firefox", Some(token_until_space_or_paren(v))));
    }
    // Mobile Safari and Safari both have "Safari/<webkit>" but Chrome does
    // too. The disambiguator is "Chrome/" or "CriOS/" / "FxiOS/" which
    // means a non-Safari shell on iOS.
    if let Some(v) = after_token(ua, "CriOS/") {
        return Some(("Chrome", Some(token_until_space_or_paren(v))));
    }
    if let Some(v) = after_token(ua, "FxiOS/") {
        return Some(("Firefox", Some(token_until_space_or_paren(v))));
    }
    if let Some(v) = after_token(ua, "Chrome/") {
        return Some(("Chrome", Some(token_until_space_or_paren(v))));
    }
    if ua.contains("Safari/") {
        // Safari's user-visible version lives in `Version/`.
        let version = after_token(ua, "Version/").map(token_until_space_or_paren);
        return Some(("Safari", version));
    }
    None
}

fn after_token<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    haystack.split_once(needle).map(|(_, rest)| rest)
}

fn token_until_space_or_paren(s: &str) -> String {
    s.chars()
        .take_while(|c| !c.is_whitespace() && *c != ')' && *c != ';')
        .collect()
}

fn token_until_semi_or_paren(s: &str) -> String {
    s.chars()
        .take_while(|c| *c != ';' && *c != ')')
        .collect::<String>()
        .trim()
        .to_string()
}

/// Map "10.0" → "10", "6.1" → "7", "6.3" → "8.1" etc. Windows NT versions
/// don't match marketing names; we surface what users will recognize.
fn parse_windows_nt(version: &str) -> Option<String> {
    let v = version.trim();
    Some(
        match v {
            "10.0" => "10",
            "6.3" => "8.1",
            "6.2" => "8",
            "6.1" => "7",
            "6.0" => "Vista",
            "5.2" => "XP",
            "5.1" => "XP",
            "5.0" => "2000",
            "" => return None,
            other => other,
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev_with_ua(ua: &str) -> Event {
        let raw = json!({
            "timestamp": "2026-01-01T00:00:00Z",
            "request": { "headers": { "User-Agent": ua } }
        });
        serde_json::from_value(raw).expect("test fixture must parse")
    }

    fn tag(event: &Event, key: &str) -> Option<String> {
        event
            .tags
            .as_ref()?
            .get(key)?
            .as_str()
            .map(|s| s.to_string())
    }

    #[test]
    fn linux_chrome_136() {
        let ua = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";
        let mut ev = ev_with_ua(ua);
        augment_tags_from_user_agent(&mut ev);
        assert_eq!(tag(&ev, "os.name").as_deref(), Some("Linux"));
        assert_eq!(tag(&ev, "os").as_deref(), Some("Linux"));
        assert_eq!(tag(&ev, "browser.name").as_deref(), Some("Chrome"));
        assert_eq!(tag(&ev, "browser").as_deref(), Some("Chrome 136.0.0.0"));
    }

    #[test]
    fn windows_10_edge() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0";
        let mut ev = ev_with_ua(ua);
        augment_tags_from_user_agent(&mut ev);
        assert_eq!(tag(&ev, "os.name").as_deref(), Some("Windows"));
        assert_eq!(tag(&ev, "os").as_deref(), Some("Windows 10"));
        assert_eq!(tag(&ev, "browser.name").as_deref(), Some("Edge"));
        assert_eq!(tag(&ev, "browser").as_deref(), Some("Edge 120.0.0.0"));
    }

    #[test]
    fn macos_safari() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15";
        let mut ev = ev_with_ua(ua);
        augment_tags_from_user_agent(&mut ev);
        assert_eq!(tag(&ev, "os.name").as_deref(), Some("macOS"));
        assert_eq!(tag(&ev, "os").as_deref(), Some("macOS 14.5.0"));
        assert_eq!(tag(&ev, "browser.name").as_deref(), Some("Safari"));
        assert_eq!(tag(&ev, "browser").as_deref(), Some("Safari 17.5"));
    }

    #[test]
    fn android_firefox() {
        let ua = "Mozilla/5.0 (Android 14; Mobile; rv:122.0) Gecko/122.0 Firefox/122.0";
        let mut ev = ev_with_ua(ua);
        augment_tags_from_user_agent(&mut ev);
        assert_eq!(tag(&ev, "os.name").as_deref(), Some("Android"));
        assert_eq!(tag(&ev, "os").as_deref(), Some("Android 14"));
        assert_eq!(tag(&ev, "browser.name").as_deref(), Some("Firefox"));
        assert_eq!(tag(&ev, "browser").as_deref(), Some("Firefox 122.0"));
    }

    #[test]
    fn iphone_safari() {
        let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
        let mut ev = ev_with_ua(ua);
        augment_tags_from_user_agent(&mut ev);
        assert_eq!(tag(&ev, "os.name").as_deref(), Some("iOS"));
        assert_eq!(tag(&ev, "os").as_deref(), Some("iOS 17.5.1"));
        assert_eq!(tag(&ev, "browser.name").as_deref(), Some("Safari"));
        assert_eq!(tag(&ev, "browser").as_deref(), Some("Safari 17.5"));
    }

    #[test]
    fn missing_user_agent_is_a_noop() {
        let raw = json!({
            "timestamp": "2026-01-01T00:00:00Z",
            "request": { "headers": {} }
        });
        let mut ev: Event = serde_json::from_value(raw).unwrap();
        augment_tags_from_user_agent(&mut ev);
        assert!(ev.tags.is_none(), "should not synthesize empty tags");
    }

    #[test]
    fn lowercase_header_name_is_accepted() {
        let raw = json!({
            "timestamp": "2026-01-01T00:00:00Z",
            "request": { "headers": { "user-agent": "Mozilla/5.0 (X11; Linux x86_64) Firefox/115.0" } }
        });
        let mut ev: Event = serde_json::from_value(raw).unwrap();
        augment_tags_from_user_agent(&mut ev);
        assert_eq!(tag(&ev, "browser.name").as_deref(), Some("Firefox"));
    }

    #[test]
    fn pre_existing_tags_are_not_overwritten() {
        let raw = json!({
            "timestamp": "2026-01-01T00:00:00Z",
            "request": { "headers": { "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) Chrome/120.0.0.0 Safari/537.36" } },
            "tags": { "os": "Custom OS Forever", "browser.name": "Mine" }
        });
        let mut ev: Event = serde_json::from_value(raw).unwrap();
        augment_tags_from_user_agent(&mut ev);
        assert_eq!(tag(&ev, "os").as_deref(), Some("Custom OS Forever"));
        assert_eq!(tag(&ev, "browser.name").as_deref(), Some("Mine"));
        // Keys we didn't set get filled in.
        assert_eq!(tag(&ev, "os.name").as_deref(), Some("Linux"));
    }
}
