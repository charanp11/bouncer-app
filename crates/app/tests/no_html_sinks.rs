//! Agent text must only ever reach the page through `textContent` (security
//! rule 4). Fails if the frontend uses any API that parses a string as HTML.

use std::path::Path;

const SINKS: &[&str] = &[
    "innerHTML",
    "outerHTML",
    "insertAdjacentHTML",
    "document.write",
    "createContextualFragment",
    "DOMParser",
    "srcdoc",
    "eval(",
    "new Function",
];

#[test]
fn frontend_never_parses_strings_as_html() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src");
    let mut checked = 0;
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "ts") {
            let code = std::fs::read_to_string(&path).unwrap();
            for sink in SINKS {
                assert!(!code.contains(sink), "{} uses {sink}", path.display());
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no frontend files found in {}", src.display());
    let html = std::fs::read_to_string(src.join("../index.html")).unwrap();
    assert!(!html.contains("<script>"), "inline script in index.html");
}
