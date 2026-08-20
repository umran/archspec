//! Assembly of the self-contained HTML document.
//!
//! Everything the page needs — style, script, and data — is inlined at
//! build time, so the output opens from disk with no server and makes
//! no external requests.

use archspec::spec::Model;
use serde::Serialize;

use crate::graph;
use crate::report::ProverReport;

const TEMPLATE: &str = include_str!("assets/template.html");
const STYLE: &str = include_str!("assets/style.css");
const SCRIPT: &str = include_str!("assets/app.js");

#[derive(Serialize)]
struct PageData<'a> {
    title: &'a str,
    model: &'a Model,
    graph: graph::Graph,
    report: Option<&'a ProverReport>,
}

pub fn render(
    model: &Model,
    report: Option<&ProverReport>,
    title: &str,
) -> Result<String, String> {
    let data = PageData {
        title,
        model,
        graph: graph::extract(model),
        report,
    };

    let json = serde_json::to_string(&data)
        .map_err(|error| format!("cannot serialize page data: {error}"))?;

    // `<` only occurs inside JSON string literals, where the escape
    // sequence `\u003c` is equivalent; this keeps `</script>`,
    // `<script`, and `<!--` (which would shift the parser into the
    // script-data escaped states) inert in the inline data block.
    let json = json.replace('<', "\\u003c");

    Ok(TEMPLATE
        .replace("__TITLE__", &escape_html(title))
        .replace("/*__STYLE__*/", STYLE)
        .replace("/*__DATA__*/", &format!("window.ARCHSPEC = {json};"))
        .replace("/*__SCRIPT__*/", SCRIPT))
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_self_contained_html() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/flash_checkout.yaml");

        let model = archspec::parser::yaml::parse(
            &std::fs::read_to_string(path).expect("fixture readable"),
        )
        .expect("fixture parses");

        let html =
            render(&model, None, "smoke </script> test").expect("renders");

        // All placeholders substituted.
        assert!(!html.contains("__TITLE__"));
        assert!(!html.contains("/*__STYLE__*/"));
        assert!(!html.contains("/*__DATA__*/"));
        assert!(!html.contains("/*__SCRIPT__*/"));

        assert!(html.contains("window.ARCHSPEC"));
        assert!(html.contains("operation.create_order"));

        // No unescaped close tag may survive inside the data block:
        // exactly the template's own two script closers remain (the
        // title's is HTML-escaped).
        assert_eq!(html.matches("</script>").count(), 2);
    }

    #[test]
    fn neutralizes_script_data_escape_sequences() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/minimal.yaml");

        let mut model = archspec::parser::yaml::parse(
            &std::fs::read_to_string(path).expect("fixture readable"),
        )
        .expect("fixture parses");

        // A `<!--` followed by `<script` inside script data would put
        // the HTML parser into the double-escaped state, where the
        // template's real close tag no longer ends the element.
        if let Some(archspec::spec::Schema::Canonical(schema)) =
            model.schemas.values_mut().next()
        {
            schema.description =
                Some("Beware <!-- of <script> tricks".to_string());
        } else {
            panic!("fixture has a canonical schema");
        }

        let html = render(&model, None, "hostile").expect("renders");

        assert!(!html.contains("<!--"));
        assert!(!html.contains("of <script>"));
        assert!(html.contains("Beware \\u003c!-- of \\u003cscript"));
        assert_eq!(html.matches("</script>").count(), 2);
    }
}
