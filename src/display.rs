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

        // Line 1: hierarchy breadcrumbs or title
        if !hit.hierarchy.is_empty() {
            let breadcrumb = hit.hierarchy.join(" > ");
            out.push_str(&format!("  {}\n", breadcrumb.bold()));
        } else if let Some(title) = &hit.title {
            out.push_str(&format!("  {}\n", title.bold()));
        }

        // Line 2: metadata (brand, price, category) — only if any are present
        let mut meta_parts: Vec<String> = Vec::new();
        if let Some(brand) = &hit.brand {
            meta_parts.push(brand.clone());
        }
        if let Some(price) = &hit.price {
            meta_parts.push(format!("€{}", price));
        }
        if let Some(category) = &hit.category {
            meta_parts.push(category.clone());
        }
        if !meta_parts.is_empty() {
            out.push_str(&format!("  {}\n", meta_parts.join(" | ").dimmed()));
        }

        // Line 3: snippet/description
        if let Some(snippet) = &hit.snippet {
            let clean = strip_highlight_tags(snippet);
            if !clean.is_empty() {
                out.push_str(&format!("  {}\n", clean.dimmed()));
            }
        }

        // Line 4: URL
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
