use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

const SITE: &str = "https://www.graphicaudio.net";

/// The spec's schema; empty fields are left out of the answer.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookMetadata {
    pub media_type: String,
    pub provider_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub narrators: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub publisher: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub published_year: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cover: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub isbn13: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<SeriesEntry>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub language_code: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub duration: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub published_date: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct SeriesEntry {
    pub series: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sequence: String,
}

fn is_zero(value: &i64) -> bool {
    *value == 0
}

macro_rules! re {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($pattern).expect("valid pattern"));
    };
}

re!(SERIES, r#"(?s)class="series-name".*?<span[^>]*>(.*?)</span>"#);
re!(EPISODE, r#"(?s)class="episode-name">\s*<span[^>]*>.*?</span>(.*?)</div>"#);
re!(AUTHOR, r#"itemprop="author">([^<]*)<"#);
re!(RELEASED, r"Release Date:\s*([A-Za-z]{3,9})\.?\s+(\d{1,2}),\s*(\d{4})");
re!(RUNNING, r"Running Time:\s*(?:(\d+)(?:\.(\d+))?)(?:&nbsp;|\s)*Hours?");
re!(ISBN, r"ISBN #:\s*(\d{13})");
re!(GENRE, r"Genre:\s*([^<\n]+)");
re!(DESCRIPTION, r#"(?s)class="product-description">(.*?)</div>"#);
re!(BREAK, r"(?i)<br\s*/?>");
re!(TAG, r"<[^>]+>");
re!(STARRING, r#"(?s)attribute-label">Starring</div>\s*<div class="attribute-value">(.*?)</div>"#);
re!(CREDIT, r#"class="credit"[^>]*>([^<]+)<"#);
re!(COVER, r#"https://www\.graphicaudio\.net/media/catalog/product/cache/[0-9a-f]+/[^"'\s]+\.(?:jpg|jpeg|png)"#);
re!(
    PRODUCT_LINK,
    r#"href="https://www\.graphicaudio\.net/([a-z0-9][a-z0-9-]*)\.html""#
);
re!(NUMBERED_TITLE, r"^(?:(\d+(?:\.\d+)?)\s*:\s*)?(.*?)(?:\s*\((\d+)\s+of\s+(\d+)\))?$");

/// Pages that are not products: the site's own sections that happen to sit at the top level.
const SECTIONS: [&str; 6] = [
    "our-productions",
    "get-started-now",
    "access",
    "access-app",
    "customer",
    "checkout",
];

/// A product's id: its page's path, which is stable and what the site links by.
pub fn product_id(url: &str) -> Option<String> {
    let id = url
        .strip_prefix(SITE)?
        .trim_start_matches('/')
        .strip_suffix(".html")?;
    (!id.is_empty() && !id.contains('/')).then(|| id.to_string())
}

pub fn product_url(id: &str) -> String {
    format!("{SITE}/{id}.html")
}

/// Product page ids linked from a search or series page, in page order. Boxed "series set"
/// downloads are left out: they are bundles, not one recording.
pub fn product_links(html: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    PRODUCT_LINK
        .captures_iter(html)
        .map(|caps| caps[1].to_string())
        .filter(|id| !SECTIONS.contains(&id.as_str()) && !id.ends_with("series-set"))
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn text(fragment: &str) -> String {
    let plain = TAG.replace_all(fragment, " ");
    let plain = plain
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The numbering in an episode heading such as `4 : Rhythm of War (5 of 6)`.
#[derive(Debug, PartialEq)]
pub struct Episode {
    pub number: Option<String>,
    pub title: String,
    pub part: Option<(u32, u32)>,
}

pub fn parse_episode(heading: &str) -> Episode {
    let heading = text(heading);
    let caps = NUMBERED_TITLE
        .captures(&heading)
        .expect("the pattern matches any text");
    Episode {
        number: caps.get(1).map(|m| m.as_str().to_string()),
        title: caps[2].trim().to_string(),
        part: caps
            .get(3)
            .zip(caps.get(4))
            .and_then(|(part, total)| Some((part.as_str().parse().ok()?, total.as_str().parse().ok()?))),
    }
}

/// `1.1` for part 1 of book 1, `4.5` for part 5 of book 4: parts sort between books. Past nine
/// parts the fraction takes two digits so `1.10` does not sort below `1.2`.
pub fn sequence(episode: &Episode) -> String {
    let Some(number) = &episode.number else {
        return String::new();
    };
    match episode.part {
        Some((part, total)) if total > 1 && !number.contains('.') => {
            if total > 9 {
                format!("{number}.{part:02}")
            } else {
                format!("{number}.{part}")
            }
        }
        _ => number.clone(),
    }
}

fn month(name: &str) -> Option<u32> {
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let name = name.to_lowercase();
    months
        .iter()
        .position(|m| name.starts_with(m))
        .map(|index| index as u32 + 1)
}

/// A product page as an answer. `None` when the page is not a recording (no episode heading).
pub fn map_product(html: &str, id: &str) -> Option<BookMetadata> {
    let heading = EPISODE.captures(html)?[1].to_string();
    let episode = parse_episode(&heading);
    if episode.title.is_empty() {
        return None;
    }
    let series = SERIES
        .captures(html)
        .map(|caps| text(&caps[1]))
        .filter(|name| !name.is_empty());
    let title = match episode.part {
        Some((part, total)) if total > 1 => format!("{} ({part} of {total})", episode.title),
        _ => episode.title.clone(),
    };

    let (published_date, published_year) = RELEASED
        .captures(html)
        .and_then(|caps| {
            let month = month(&caps[1])?;
            let day: u32 = caps[2].parse().ok()?;
            Some((format!("{}-{month:02}-{day:02}", &caps[3]), caps[3].to_string()))
        })
        .unwrap_or_default();

    let duration = RUNNING
        .captures(html)
        .map(|caps| {
            let hours: f64 = format!("{}.{}", &caps[1], caps.get(2).map_or("0", |m| m.as_str()))
                .parse()
                .unwrap_or(0.0);
            (hours * 3600.0).round() as i64
        })
        .unwrap_or(0);

    let isbn13 = ISBN
        .captures(html)
        .map(|caps| caps[1].to_string())
        .unwrap_or_default();

    let narrators = STARRING
        .captures(html)
        .map(|caps| {
            CREDIT
                .captures_iter(&caps[1])
                .map(|c| text(&c[1]))
                .filter(|name| !name.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let cover = COVER
        .find_iter(html)
        .map(|m| m.as_str())
        .find(|url| !isbn13.is_empty() && url.contains(&isbn13))
        .or_else(|| COVER.find(html).map(|m| m.as_str()))
        .unwrap_or_default()
        .to_string();

    Some(BookMetadata {
        media_type: "audiobook".into(),
        provider_id: id.to_string(),
        title,
        subtitle: "GraphicAudio Dramatized Adaptation".into(),
        authors: AUTHOR
            .captures(html)
            .map(|caps| text(&caps[1]))
            .filter(|name| !name.is_empty())
            .into_iter()
            .collect(),
        narrators,
        publisher: "GraphicAudio".into(),
        published_year,
        description: description(html),
        cover,
        isbn13,
        genres: GENRE
            .captures(html)
            .map(|caps| {
                text(&caps[1])
                    .split(',')
                    .map(|g| g.trim().to_string())
                    .filter(|g| !g.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        series: series
            .map(|series| {
                vec![SeriesEntry {
                    series,
                    sequence: sequence(&episode),
                }]
            })
            .unwrap_or_default(),
        language_code: "en".into(),
        duration,
        published_date,
        url: product_url(id),
    })
}

/// The blurb's paragraphs, without the closing copyright and production notice.
fn description(html: &str) -> String {
    let Some(caps) = DESCRIPTION.captures(html) else {
        return String::new();
    };
    BREAK
        .split(&caps[1])
        .map(text)
        .filter(|paragraph| !paragraph.is_empty() && !paragraph.starts_with('©'))
        .map(|paragraph| match paragraph.split_once(" © ") {
            Some((blurb, _)) => blurb.to_string(),
            None => paragraph,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn words(value: &str) -> BTreeSet<String> {
    value
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1 && !matches!(*w, "the" | "of" | "and" | "an" | "in" | "to"))
        .map(str::to_string)
        .collect()
}

/// What a query asks for, once the file name's trimmings are gone: `(2 of 3)`, `Part 2`,
/// `GraphicAudio`, `[Dramatized Adaptation]`.
pub fn clean_query(query: &str) -> String {
    static NOISE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\[[^\]]*\]|graphic\s*audio|dramati[sz]ed( adaptation)?|\(?\s*(?:part\s*)?\d+\s+of\s+\d+\s*\)?|\(\s*part\s*\d+\s*\)",
        )
        .expect("valid pattern")
    });
    let stripped = NOISE.replace_all(query, " ");
    // File titles often read "Iron Flame (...): The Empyrean, Book 2". The slug carries the book's
    // own title, so what follows the colon (series and number) would only rule every product out.
    let title = match stripped.split_once(':') {
        Some((head, _)) if !head.trim().is_empty() => head,
        _ => &stripped,
    };
    title
        .replace(['(', ')'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The part a query names (`1 of 3`, `Part 2`), if any.
pub fn wanted_part(query: &str) -> Option<u32> {
    static PART: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\(?\b(?:(\d+)\s+of\s+\d+|(?:part|pt\.?)\s*(\d+))\b").expect("valid pattern")
    });
    let caps = PART.captures(query)?;
    caps.get(1).or_else(|| caps.get(2))?.as_str().parse().ok()
}

/// Whether a product's slug could be what was asked for: it shares every significant word of the
/// query, so a series search that returns 35 products only fetches the few that can match.
pub fn slug_matches(id: &str, query: &str) -> bool {
    let wanted = words(query);
    let slug = words(&id.replace('-', " "));
    !wanted.is_empty() && wanted.is_subset(&slug)
}

/// Best first: right part, then the author, then the shorter (closer) title.
pub fn rank(mut found: Vec<BookMetadata>, query: &str, author: &str, part: Option<u32>) -> Vec<BookMetadata> {
    let author_words = words(author);
    let query_words = words(query);
    found.sort_by_key(|book| {
        let part_matches = part.is_some_and(|part| book.title.contains(&format!("({part} of ")));
        let author_shared = words(&book.authors.join(" ")).intersection(&author_words).count();
        let extra = words(&book.title).difference(&query_words).count();
        (
            std::cmp::Reverse(part_matches),
            std::cmp::Reverse(author_shared),
            extra,
        )
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRODUCT: &str = include_str!("../testdata/mistborn-1-1of3.html");
    const SERIES_PAGE: &str = include_str!("../testdata/mistborn-series.html");

    #[test]
    fn episode_headings() {
        let e = parse_episode("4 :   Rhythm of War (5 of 6) ");
        assert_eq!(
            e,
            Episode { number: Some("4".into()), title: "Rhythm of War".into(), part: Some((5, 6)) }
        );
        assert_eq!(sequence(&e), "4.5");
        let single = parse_episode("5 : Shadows of Self");
        assert_eq!((single.part, sequence(&single)), (None, "5".into()));
        let unnumbered = parse_episode("Dune (1 of 2)");
        assert_eq!(sequence(&unnumbered), "");
        let long = Episode { number: Some("1".into()), title: String::new(), part: Some((10, 12)) };
        assert_eq!(sequence(&long), "1.10");
    }

    #[test]
    fn maps_a_product_page() {
        let book = map_product(PRODUCT, "mistborn-1-the-final-empire-1-of-3").expect("a recording");
        assert_eq!(book.title, "The Final Empire (1 of 3)");
        assert_eq!(book.authors, vec!["Brandon Sanderson"]);
        assert_eq!(book.series, vec![SeriesEntry { series: "Mistborn".into(), sequence: "1.1".into() }]);
        assert_eq!(book.published_date, "2013-12-01");
        assert_eq!(book.duration, 6 * 3600);
        assert_eq!(book.isbn13, "9781648812989");
        assert_eq!(book.genres, vec!["Fantasy"]);
        assert_eq!(book.narrators.first().map(String::as_str), Some("Terence Aselford"));
        assert!(book.narrators.len() > 5, "a full cast, got {:?}", book.narrators);
        assert!(book.cover.contains("9781648812989"));
        assert!(book.description.starts_with("Brandon Sanderson, fantasy"));
        assert!(!book.description.contains('©'));
        assert_eq!(book.media_type, "audiobook");
    }

    #[test]
    fn series_page_links_products_only() {
        let links = product_links(SERIES_PAGE);
        assert!(links.contains(&"mistborn-1-the-final-empire-1-of-3".to_string()));
        assert!(links.contains(&"mistborn-4-the-alloy-of-law".to_string()));
        assert!(links.iter().all(|id| id != "our-productions" && !id.ends_with("series-set")));
    }

    #[test]
    fn narrows_candidates_by_slug() {
        assert!(slug_matches("mistborn-1-the-final-empire-2-of-3", "The Final Empire"));
        assert!(!slug_matches("mistborn-2-the-well-of-ascension-1-of-3", "The Final Empire"));
        assert_eq!(wanted_part("The Final Empire (2 of 3)"), Some(2));
        assert_eq!(clean_query("The Final Empire (2 of 3) [GraphicAudio]"), "The Final Empire");
        let sent = "Iron Flame (Part 2 of 2) (Dramatized Adaptation): The Empyrean, Book 2";
        assert_eq!(clean_query(sent), "Iron Flame");
        assert_eq!(wanted_part(sent), Some(2));
        assert_eq!(clean_query("Iron Flame (Part 1 of 2)"), "Iron Flame");
        assert_eq!(clean_query("Iron Flame"), "Iron Flame");
        assert!(slug_matches("the-empyrean-2-iron-flame-2-of-2", &clean_query(sent)));
    }

    #[test]
    fn ids_round_trip() {
        let url = product_url("mistborn-4-the-alloy-of-law");
        assert_eq!(product_id(&url).as_deref(), Some("mistborn-4-the-alloy-of-law"));
        assert_eq!(product_id("https://www.graphicaudio.net/our-productions/series.html"), None);
    }
}
