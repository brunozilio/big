use serde::{Deserialize, Serialize};

pub const MAX_BYTES: u64 = 16 * 1024 * 1024;
pub const META: &str = "meta.json";
pub const FOLDER: &str = "artifacts";
const MARKER: &str = "Artifact id: ";
const TITLE_SCAN: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Page,
    Markdown,
    Pdf,
    Image,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub title: String,
    pub name: String,
    pub kind: Kind,
    pub session: String,
    pub source: String,
    pub published_at: i64,
    pub size: u64,
}

const TYPES: [(&str, Kind, &str); 13] = [
    ("html", Kind::Page, "text/html; charset=utf-8"),
    ("htm", Kind::Page, "text/html; charset=utf-8"),
    ("md", Kind::Markdown, "text/markdown; charset=utf-8"),
    ("pdf", Kind::Pdf, "application/pdf"),
    ("png", Kind::Image, "image/png"),
    ("jpg", Kind::Image, "image/jpeg"),
    ("jpeg", Kind::Image, "image/jpeg"),
    ("gif", Kind::Image, "image/gif"),
    ("webp", Kind::Image, "image/webp"),
    ("svg", Kind::Image, "image/svg+xml"),
    ("csv", Kind::Text, "text/csv; charset=utf-8"),
    ("txt", Kind::Text, "text/plain; charset=utf-8"),
    ("json", Kind::Text, "application/json"),
];

fn extension(name: &str) -> String {
    name.rsplit_once('.').map(|(_, found)| found.to_ascii_lowercase()).unwrap_or_default()
}

pub fn kind_of(name: &str) -> Option<Kind> {
    let found = extension(name);
    TYPES.iter().find(|(known, _, _)| *known == found).map(|(_, kind, _)| *kind)
}

pub fn media_type(name: &str) -> &'static str {
    let found = extension(name);
    TYPES.iter().find(|(known, _, _)| *known == found).map_or("application/octet-stream", |(_, _, media)| media)
}

pub fn accepted() -> String {
    TYPES.iter().map(|(known, _, _)| format!(".{known}")).collect::<Vec<_>>().join(", ")
}

fn head(text: &str) -> &str {
    let end = (0..=TITLE_SCAN.min(text.len())).rev().find(|at| text.is_char_boundary(*at)).unwrap_or(0);
    &text[..end]
}

fn html_title(text: &str) -> Option<String> {
    let scanned = head(text);
    let lower = scanned.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    Some(scanned[start..end].split_whitespace().collect::<Vec<_>>().join(" "))
}

fn markdown_title(text: &str) -> Option<String> {
    head(text).lines().find_map(|line| line.strip_prefix("# ")).map(|title| title.trim().to_owned())
}

fn stem(name: &str) -> String {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_owned()
}

pub fn title_of(given: Option<&str>, kind: Kind, name: &str, text: &str) -> String {
    let found = match kind {
        Kind::Page => html_title(text),
        Kind::Markdown => markdown_title(text),
        Kind::Pdf | Kind::Image | Kind::Text => None,
    };
    let chosen = given.map(str::trim).filter(|title| !title.is_empty()).map(str::to_owned);
    chosen.or(found).filter(|title| !title.is_empty()).unwrap_or_else(|| stem(name))
}

pub fn whole_document(text: &str) -> bool {
    let start = text.trim_start().get(..64).unwrap_or(text.trim_start()).to_ascii_lowercase();
    start.starts_with("<!doctype") || start.starts_with("<html")
}

pub fn wrapped(text: &str) -> String {
    if whole_document(text) {
        return text.to_owned();
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\"><style>:root{{color-scheme:light}}body{{margin:0;font:14px/1.5 system-ui,-apple-system,sans-serif}}img{{max-width:100%}}[hidden]{{display:none!important}}</style></head><body>\n{text}\n</body></html>"
    )
}

pub fn marked(id: &str) -> String {
    format!("{MARKER}{id}")
}

pub fn id_in(output: &str) -> Option<&str> {
    let (_, rest) = output.rsplit_once(MARKER)?;
    let id = rest.split_whitespace().next()?;
    id.chars().all(|letter| letter.is_ascii_hexdigit()).then_some(id).filter(|id| !id.is_empty())
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|letter| letter.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_s_kind_and_media_type_come_from_its_extension() {
        assert_eq!(kind_of("Relatório.HTML"), Some(Kind::Page));
        assert_eq!(kind_of("notes.md"), Some(Kind::Markdown));
        assert_eq!(kind_of("data.csv"), Some(Kind::Text));
        assert_eq!(kind_of("app.exe"), None);
        assert_eq!(media_type("a.pdf"), "application/pdf");
        assert_eq!(media_type("a"), "application/octet-stream");
        assert!(accepted().contains(".html") && accepted().contains(".pdf"));
    }

    #[test]
    fn the_title_is_the_one_given_then_the_document_s_own_then_the_file_name() {
        let page = "<style></style><TITLE>\n Vendas  do trimestre </TITLE>";
        assert_eq!(title_of(None, Kind::Page, "q3.html", page), "Vendas do trimestre");
        assert_eq!(title_of(Some(" Q3 "), Kind::Page, "q3.html", page), "Q3");
        assert_eq!(title_of(None, Kind::Markdown, "n.md", "intro\n# Riscos\n"), "Riscos");
        assert_eq!(title_of(None, Kind::Pdf, "contrato.final.pdf", ""), "contrato.final");
        assert_eq!(title_of(Some(""), Kind::Page, "x.html", "<title></title>"), "x");
    }

    #[test]
    fn a_fragment_gets_the_skeleton_and_a_whole_document_stays_as_it_is() {
        let page = wrapped("<title>A</title><p>oi</p>");
        assert!(
            page.starts_with("<!doctype html>") && page.contains("<p>oi</p>") && page.contains("viewport-fit=cover")
        );
        assert_eq!(wrapped("  <!DOCTYPE html><p>x</p>"), "  <!DOCTYPE html><p>x</p>");
        assert_eq!(wrapped("<html><body></body></html>"), "<html><body></body></html>");
    }

    #[test]
    fn the_id_is_read_back_from_the_tool_s_answer() {
        let said = format!("Published \"Q3\".\n{}", marked("ab12"));
        assert_eq!(id_in(&said), Some("ab12"));
        assert_eq!(id_in("Published nothing"), None);
        assert_eq!(id_in("Artifact id: ../x"), None);
        assert!(valid_id("ab12") && !valid_id("") && !valid_id("a/b"));
    }
}
