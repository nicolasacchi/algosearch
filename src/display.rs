use crate::search::SearchResponse;
use colored::Colorize;

pub fn format_search_results(resp: &SearchResponse) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "{} {} — {} results\n",
        format!("[{}]", resp.site).cyan().bold(),
        resp.query,
        resp.total_hits
    ));

    for hit in &resp.results {
        out.push('\n');

        if !hit.hierarchy.is_empty() {
            let breadcrumb = hit.hierarchy.join(" > ");
            out.push_str(&format!("  {}\n", breadcrumb.bold()));
        }

        if let Some(snippet) = &hit.snippet {
            let clean = strip_highlight_tags(snippet);
            if !clean.is_empty() {
                out.push_str(&format!("  {}\n", clean.dimmed()));
            }
        }

        if let Some(url) = &hit.url {
            out.push_str(&format!("  {} {}\n", "→".green(), url));
        }
    }

    out
}

fn strip_highlight_tags(s: &str) -> String {
    s.replace("<em>", "")
        .replace("</em>", "")
        .replace("<mark>", "")
        .replace("</mark>", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_highlight_tags() {
        assert_eq!(
            strip_highlight_tags("Use <em>useState</em> hook"),
            "Use useState hook"
        );
        assert_eq!(
            strip_highlight_tags("<mark>React</mark> hooks"),
            "React hooks"
        );
        assert_eq!(strip_highlight_tags("no tags"), "no tags");
    }
}
