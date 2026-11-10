//! Pinned Actual26.10 title vocabulary and character class, not a user pattern.
use serde::Deserialize;
use std::{collections::BTreeSet, sync::LazyLock};
#[derive(Deserialize)]
struct Data {
    character: String,
    lower: BTreeSet<String>,
    specials: Vec<String>,
}
static DATA: LazyLock<Data> = LazyLock::new(|| {
    serde_json::from_str(include_str!("title-data.json")).expect("bundled title data")
});
static WORD: LazyLock<regress::Regex> = LazyLock::new(|| {
    let c = &DATA.character;
    regress::Regex::new(&format!(
        r#"(?:(?:(\s?(?:^|[.\(\)!?;:"-])\s*)({c}))|({c}))({c}*[’']*{c}*)"#
    ))
    .expect("trusted title expression")
});
static SPECIALS: LazyLock<Vec<(regress::Regex, String)>> = LazyLock::new(|| {
    DATA.specials
        .iter()
        .map(|word| {
            (
                regress::Regex::with_flags(&format!(r"\b{word}\b"), "i")
                    .expect("trusted title special"),
                word.clone(),
            )
        })
        .collect()
});
pub(super) fn title(input: &str) -> String {
    let text = input.to_lowercase();
    let mut result = String::new();
    let mut previous = 0;
    for found in WORD.find_iter(&text) {
        result.push_str(&text[previous..found.range.start]);
        let matched = &text[found.range.clone()];
        let first = matched.chars().next().unwrap();
        if matches!(first, '(' | ')') {
            result.push_str(matched);
        } else {
            let lead = found.group(1).map(|range| &text[range]).unwrap_or("");
            let forced = found.group(2).map(|range| &text[range]);
            let lower = found.group(3).map(|range| &text[range]).unwrap_or("");
            let rest = found.group(4).map(|range| &text[range]).unwrap_or("");
            if forced.is_none() && DATA.lower.contains(&format!("{lower}{rest}")) {
                result.push_str(if crate::payees_tags::js_whitespace(first) {
                    &matched[first.len_utf8()..]
                } else {
                    matched
                });
            } else {
                result.push_str(lead);
                result.push_str(
                    &if lower.is_empty() {
                        forced.unwrap_or("")
                    } else {
                        lower
                    }
                    .to_uppercase(),
                );
                result.push_str(rest);
            }
        }
        previous = found.range.end;
    }
    result.push_str(&text[previous..]);
    for (pattern, special) in SPECIALS.iter() {
        let mut next = String::new();
        let mut previous = 0;
        for found in pattern.find_iter(&result) {
            next.push_str(&result[previous..found.range.start]);
            next.push_str(special);
            previous = found.range.end;
        }
        next.push_str(&result[previous..]);
        result = next;
    }
    result
}
