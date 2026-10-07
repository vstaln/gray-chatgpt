use super::*;

#[test]
fn success_page_carries_the_mark_and_the_outcome() {
    let page = signed_in();
    assert!(page.starts_with("<!doctype html>"));
    assert!(page.contains(r#"aria-label="Gray""#), "inline mark");
    assert!(page.contains("Signed in with ChatGPT"));
    assert!(page.contains(r#"<main class="ok">"#));
}

#[test]
fn failure_text_is_escaped() {
    let page = failed("<script>alert('x')</script> & \"quoted\"");
    assert!(!page.contains("<script>"), "{page}");
    assert!(
        page.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; &quot;quoted&quot;")
    );
    assert!(page.contains(r#"<main class="error">"#));
}

#[test]
fn recovery_hint_is_said_once() {
    let page = failed("Permission was not granted. Run /connect again and allow plan usage.");
    assert_eq!(page.matches("/connect").count(), 1, "{page}");
    let page = failed("ChatGPT sign-in failed.");
    assert!(page.contains("ChatGPT sign-in failed. Run /connect in Gray to try again."));
}

#[test]
fn page_loads_nothing_from_the_network() {
    for page in [signed_in(), failed("x")] {
        for needle in ["http://", "https://", "<script", "<link", "@import", "url("] {
            // The SVG namespace is an identifier, not a fetch.
            let stripped = page.replace("http://www.w3.org/2000/svg", "");
            assert!(!stripped.contains(needle), "found {needle}");
        }
    }
    assert!(CSP.starts_with("default-src 'none'"));
}
