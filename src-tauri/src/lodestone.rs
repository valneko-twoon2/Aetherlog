//! Addresses for the Lodestone.  The app never contacts the site itself: it only hands an
//! address to the system browser, and the host is fixed.

const SEARCH: &str = "https://jp.finalfantasyxiv.com/lodestone/character/";

/// Percent-encodes everything but unreserved characters, so a name cannot alter the address.
fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

pub fn search_url(name: &str, world: &str) -> String {
    let mut url = format!("{SEARCH}?q={}", url_encode(name));
    if !world.is_empty() {
        url.push_str(&format!("&worldname={}", url_encode(world)));
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_encoded() {
        assert_eq!(search_url("A&b c", ""), format!("{SEARCH}?q=A%26b%20c"));
        assert_eq!(search_url("Alice", "Tiamat"), format!("{SEARCH}?q=Alice&worldname=Tiamat"));
    }
}
