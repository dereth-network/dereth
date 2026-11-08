//! A plain Markdown to HTML conversion, enough for the specification's own pages: headings,
//! paragraphs, lists (nested by indentation), fenced code, tables, block quotes, rules, and inline
//! code, emphasis and links. Anything else is shown as its text. Every character of the source is
//! escaped before it is marked up, so a page can hold no markup of its own.

/// `text` as a whole HTML page, its title its first heading, each link's target passed through
/// `link`.
#[must_use]
pub fn page(text: &str, link: &dyn Fn(&str) -> String) -> String {
    let title = text
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .map_or_else(|| "Dereth".to_owned(), |t| escape(strip_inline(t).trim()));
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" \
         content=\"width=device-width, initial-scale=1\">\n<title>{title}</title>\n<style>{STYLE}</style>\n\
         </head>\n<body>\n<main>\n{}</main>\n</body>\n</html>\n",
        body(text, link)
    )
}

const STYLE: &str = "body{margin:0;background:#fbfaf7;color:#1f1d1a;font:16px/1.55 system-ui,sans-serif}\
main{max-width:52rem;margin:0 auto;padding:1.5rem 1rem 4rem}\
h1,h2,h3,h4{line-height:1.25}code{font:.9em ui-monospace,monospace;background:#efece5;padding:.1em .3em;border-radius:3px}\
pre{background:#efece5;padding:.8rem;overflow-x:auto;border-radius:4px}pre code{background:none;padding:0}\
table{border-collapse:collapse;display:block;overflow-x:auto}th,td{border:1px solid #d8d3c8;padding:.3rem .5rem;text-align:left;vertical-align:top}\
blockquote{margin:0;padding-left:1rem;border-left:3px solid #d8d3c8;color:#55514a}a{color:#2b5d9b}\
@media (prefers-color-scheme:dark){body{background:#1a1917;color:#e8e4dc}code,pre{background:#2a2824}\
th,td{border-color:#3b3833}blockquote{border-color:#3b3833;color:#b5afa4}a{color:#8db4e8}}";

/// HTML-escaped text.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// A heading's text with its inline marks taken off, for the page's title.
fn strip_inline(s: &str) -> String {
    s.replace(['`', '*'], "")
}

/// An anchor for a heading, as code hosts make them: lowercase, its letters, digits, `-` and `_`,
/// spaces as `-`.
fn anchor(text: &str) -> String {
    strip_inline(text)
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// One line's inline marks: code spans, links, strong and emphasis.
fn inline(s: &str, link: &dyn Fn(&str) -> String) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let find = |from: usize, c: char| chars[from..].iter().position(|x| *x == c).map(|p| p + from);
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            if let Some(end) = find(i + 1, '`') {
                let code: String = chars[i + 1..end].iter().collect();
                out.push_str(&format!("<code>{}</code>", escape(&code)));
                i = end + 1;
                continue;
            }
        }
        if c == '[' {
            if let Some(close) = find(i + 1, ']') {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = find(close + 2, ')') {
                        let label: String = chars[i + 1..close].iter().collect();
                        let target: String = chars[close + 2..end].iter().collect();
                        out.push_str(&format!(
                            "<a href=\"{}\">{}</a>",
                            escape(&link(target.trim())),
                            inline(&label, link)
                        ));
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(end) = rest.find("**").filter(|e| *e > 0) {
                out.push_str(&format!("<strong>{}</strong>", inline(&rest[..end], link)));
                i += 2 + rest[..end].chars().count() + 2;
                continue;
            }
        }
        if c == '*' && chars.get(i + 1).is_some_and(|n| !n.is_whitespace()) {
            if let Some(end) = find(i + 1, '*') {
                let inner: String = chars[i + 1..end].iter().collect();
                out.push_str(&format!("<em>{}</em>", inline(&inner, link)));
                i = end + 1;
                continue;
            }
        }
        out.push_str(&escape(&c.to_string()));
        i += 1;
    }
    out
}

/// A table row's cells.
fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_owned()).collect()
}

fn is_rule_row(line: &str) -> bool {
    let c = cells(line);
    !c.is_empty()
        && c.iter()
            .all(|c| !c.is_empty() && c.chars().all(|x| matches!(x, '-' | ':' | ' ')))
}

/// A list item's indentation, whether it is numbered, and its text, when `line` is one.
fn item(line: &str) -> Option<(usize, bool, &str)> {
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
        return Some((indent, false, rest));
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        if let Some(rest) = t[digits..].strip_prefix(". ") {
            return Some((indent, true, rest));
        }
    }
    None
}

/// The page's body.
fn body(text: &str, link: &dyn Fn(&str) -> String) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, out: &mut String| {
        if !para.is_empty() {
            let joined = para.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ");
            out.push_str(&format!("<p>{}</p>\n", inline(&joined, link)));
            para.clear();
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        if t.is_empty() {
            flush(&mut para, &mut out);
            i += 1;
            continue;
        }
        if let Some(fence) = t.strip_prefix("```") {
            flush(&mut para, &mut out);
            let lang = fence.trim();
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1;
            let class = if lang.is_empty() {
                String::new()
            } else {
                format!(" class=\"language-{}\"", escape(lang))
            };
            out.push_str(&format!(
                "<pre><code{class}>{}</code></pre>\n",
                escape(&code.join("\n"))
            ));
            continue;
        }
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            flush(&mut para, &mut out);
            let raw = t[hashes..].trim();
            let (text, id) = match raw.rsplit_once(" {#") {
                Some((h, a)) if a.ends_with('}') => (h, a.trim_end_matches('}').to_owned()),
                _ => (raw, anchor(raw)),
            };
            out.push_str(&format!(
                "<h{hashes} id=\"{}\">{}</h{hashes}>\n",
                escape(&id),
                inline(text, link)
            ));
            i += 1;
            continue;
        }
        if t == "---" || t == "***" {
            flush(&mut para, &mut out);
            out.push_str("<hr>\n");
            i += 1;
            continue;
        }
        if t.starts_with('|') && lines.get(i + 1).is_some_and(|n| is_rule_row(n)) {
            flush(&mut para, &mut out);
            out.push_str("<table>\n<thead><tr>");
            for c in cells(t) {
                out.push_str(&format!("<th>{}</th>", inline(&c, link)));
            }
            out.push_str("</tr></thead>\n<tbody>\n");
            i += 2;
            while i < lines.len() && lines[i].trim().starts_with('|') {
                out.push_str("<tr>");
                for c in cells(lines[i]) {
                    out.push_str(&format!("<td>{}</td>", inline(&c, link)));
                }
                out.push_str("</tr>\n");
                i += 1;
            }
            out.push_str("</tbody>\n</table>\n");
            continue;
        }
        if let Some(q) = t.strip_prefix('>') {
            flush(&mut para, &mut out);
            let mut quoted = vec![q.trim_start()];
            i += 1;
            while i < lines.len() {
                match lines[i].trim().strip_prefix('>') {
                    Some(q) => quoted.push(q.trim_start()),
                    None => break,
                }
                i += 1;
            }
            out.push_str(&format!(
                "<blockquote>\n{}</blockquote>\n",
                body(&quoted.join("\n"), link)
            ));
            continue;
        }
        if item(line).is_some() && para.is_empty() {
            i = list(&lines, i, &mut out, link);
            continue;
        }
        para.push(line);
        i += 1;
    }
    flush(&mut para, &mut out);
    out
}

/// The list starting at line `start`, and the items nested in it: the line after it.
fn list(lines: &[&str], start: usize, out: &mut String, link: &dyn Fn(&str) -> String) -> usize {
    let Some((indent, numbered, _)) = item(lines[start]) else {
        return start + 1;
    };
    let tag = if numbered { "ol" } else { "ul" };
    out.push_str(&format!("<{tag}>\n"));
    let mut i = start;
    while i < lines.len() {
        let Some((at, kind, text)) = item(lines[i]) else {
            break;
        };
        if at < indent || (at == indent && kind != numbered) {
            break;
        }
        if at > indent {
            i = list(lines, i, out, link);
            continue;
        }
        // The item's text, with the lines that carry it on.
        let mut words = vec![text.trim()];
        i += 1;
        while i < lines.len() {
            let l = lines[i];
            let lead = l.len() - l.trim_start().len();
            if l.trim().is_empty() || item(l).is_some() || lead <= indent {
                break;
            }
            words.push(l.trim());
            i += 1;
        }
        out.push_str(&format!("<li>{}", inline(&words.join(" "), link)));
        if i < lines.len() && item(lines[i]).is_some_and(|(a, _, _)| a > indent) {
            out.push('\n');
            i = list(lines, i, out, link);
        }
        out.push_str("</li>\n");
        // A blank line between items keeps the list going.
        if i + 1 < lines.len()
            && lines[i].trim().is_empty()
            && item(lines[i + 1]).is_some_and(|(a, k, _)| a == indent && k == numbered)
        {
            i += 1;
        }
    }
    out.push_str(&format!("</{tag}>\n"));
    i
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the guide's pages written as HTML for the art pack source's host).
    use super::*;

    fn same(target: &str) -> String {
        target.to_owned()
    }

    #[test]
    fn a_page_keeps_its_headings_paragraphs_lists_code_and_tables() {
        let md = "# Making a pack\n\nA pack is a *folder* with **one** file, `manifest.toml`.\nIt goes on.\n\n\
                  ## The manifest {#manifest}\n\n- one\n- two\n  - two and a half\n- three\n\n1. first\n2. second\n\n\
                  ```toml\n[pack]\nname = \"<x>\"\n```\n\n| key | what |\n|---|---|\n| `id` | its [id](sources.md#ids) |\n\n\
                  > Note: a quote.\n\n---\n";
        let html = page(md, &same);
        assert!(html.contains("<title>Making a pack</title>"));
        assert!(html.contains("<h1 id=\"making-a-pack\">Making a pack</h1>"));
        assert!(html.contains(
            "<p>A pack is a <em>folder</em> with <strong>one</strong> file, \
             <code>manifest.toml</code>. It goes on.</p>"
        ));
        assert!(html.contains("<h2 id=\"manifest\">The manifest</h2>"));
        assert!(html.contains(
            "<ul>\n<li>one</li>\n<li>two\n<ul>\n<li>two and a half</li>\n</ul>\n</li>\n<li>three</li>\n</ul>"
        ));
        assert!(html.contains("<ol>\n<li>first</li>\n<li>second</li>\n</ol>"));
        assert!(html.contains(
            "<pre><code class=\"language-toml\">[pack]\nname = &quot;&lt;x&gt;&quot;</code></pre>"
        ));
        assert!(html.contains("<th>key</th><th>what</th>"));
        assert!(
            html.contains("<td><code>id</code></td><td>its <a href=\"sources.md#ids\">id</a></td>")
        );
        assert!(html.contains("<blockquote>\n<p>Note: a quote.</p>\n</blockquote>"));
        assert!(html.contains("<hr>"));
    }

    #[test]
    fn markup_in_the_text_is_shown_as_text_and_links_go_through_the_mapping() {
        let html = page(
            "Some <script>alert(1)</script> & [a page](other.md).\n",
            &|t: &str| format!("mapped:{t}"),
        );
        assert!(html.contains("Some &lt;script&gt;alert(1)&lt;/script&gt; &amp; "));
        assert!(html.contains("<a href=\"mapped:other.md\">a page</a>"));
        assert!(!html.contains("<script>"));
    }
}
