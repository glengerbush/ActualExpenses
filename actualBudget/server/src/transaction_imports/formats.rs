//! Structured financial formats share the retained-byte and review workflow.
use super::*;
use roxmltree::{Document, Node};
fn truth(value: Option<&str>) -> Option<&str> {
    value.filter(|s| !s.is_empty())
}
fn swapped(
    payee: Option<String>,
    memo: Option<String>,
    options: &parse::Options,
    fallback: bool,
) -> (Option<String>, Option<String>, bool) {
    let (payee, memo) = if options.swap_payee_and_memo {
        (memo, payee)
    } else {
        (payee, memo)
    };
    let used = payee.as_deref().is_none_or(str::is_empty) && fallback;
    (
        if used { memo.clone() } else { payee },
        if options.import_notes && !used {
            memo
        } else {
            None
        },
        used,
    )
}
fn checked(rows: Vec<Value>) -> Result<Vec<Value>, Error> {
    if rows.len() > 100000 {
        return Err(Error::invalid("Import at most100000 rows"));
    }
    for row in &rows {
        for value in row.as_object().unwrap().values() {
            if value.as_str().is_some_and(|s| s.chars().count() > 10000) {
                return Err(Error::invalid("A financial field exceeds10000 characters"));
            }
        }
    }
    Ok(rows)
}
pub(super) fn qif(bytes: &[u8], options: &parse::Options) -> Result<Vec<Value>, Error> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.split('\n').filter(|s| !s.is_empty());
    if !lines
        .next()
        .is_some_and(|s| s.trim().starts_with("!Type:") && !s.contains('$'))
    {
        return Err(Error::invalid(
            "File does not appear to be a valid QIF file",
        ));
    }
    let mut records = vec![];
    let mut current = BTreeMap::new();
    for line in lines {
        let line = line.trim_matches(crate::payees_tags::js_whitespace);
        if line == "^" {
            records.push(std::mem::take(&mut current));
            continue;
        }
        let Some(code) = line.chars().next() else {
            return Err(Error::invalid("Unknown QIF detail code"));
        };
        if !"DTNMAPLCSE$".contains(code) {
            return Err(Error::invalid("Unknown QIF detail code"));
        }
        current.insert(code, line[code.len_utf8()..].to_owned());
    }
    if !current.is_empty() {
        records.push(current);
    }
    let mut rows = vec![];
    for record in records {
        let (Some(date), Some(amount)) = (record.get(&'D'), record.get(&'T')) else {
            continue;
        };
        let payee = record.get(&'P').map(|s| s.replace("&amp;", "&"));
        let memo = record.get(&'M').cloned();
        let (payee, notes, used) = swapped(payee, memo, options, options.swap_payee_and_memo);
        let category = record
            .get(&'L')
            .map(|s| {
                let mut parts = s.split(':');
                let base = parts.next().unwrap_or("");
                parts
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(base)
                    .to_owned()
            })
            .filter(|s| !s.is_empty());
        let mut row = json!({"amount":parse::loose(Some(amount)),"date":date,"payee_name":payee,"imported_payee":payee,"category":category,"notes":notes});
        if used && row["payee_name"].is_null() {
            row.as_object_mut().unwrap().remove("payee_name");
            row.as_object_mut().unwrap().remove("imported_payee");
        }
        rows.push(row);
    }
    checked(rows)
}
fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}
fn path<'a, 'i>(mut node: Node<'a, 'i>, names: &[&str]) -> Option<Node<'a, 'i>> {
    for name in names {
        node = child(node, name)?;
    }
    Some(node)
}
fn content(node: Option<Node<'_, '_>>) -> Option<String> {
    node.and_then(|n| n.text()).map(str::to_owned)
}
fn text(node: Node<'_, '_>, name: &str) -> Option<String> {
    content(child(node, name))
}
fn decode(bytes: &[u8], xml: bool) -> String {
    let header = String::from_utf8_lossy(&bytes[..bytes.len().min(if xml { 500 } else { 2048 })]);
    let encoding = if xml {
        let lower = header.to_ascii_lowercase();
        lower.find("encoding").and_then(|at| {
            let s = header[at + 8..].trim_start();
            let s = s.strip_prefix('=')?.trim_start();
            let quote = s.chars().next()?;
            if !matches!(quote, '\'' | '"') {
                return None;
            }
            Some(s[1..].split(quote).next()?.to_owned())
        })
    } else {
        let lookup = |key: &str| {
            header
                .lines()
                .find_map(|line| line.strip_prefix(key).map(|s| s.trim().to_owned()))
        };
        if lookup("ENCODING:").as_deref() == Some("UTF-8") {
            Some("utf-8".into())
        } else {
            lookup("CHARSET:").map(|s| match s.as_str() {
                "8859-1" => "iso-8859-1".into(),
                "1252" => "windows-1252".into(),
                _ => "utf-8".into(),
            })
        }
    };
    let codec = encoding
        .as_deref()
        .and_then(|s| encoding_rs::Encoding::for_label(s.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    codec.decode(bytes).0.into_owned()
}
fn document(text: &str) -> Result<Document<'_>, Error> {
    Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            nodes_limit: 500000,
            ..Default::default()
        },
    )
    .map_err(|_| Error::invalid("Malformed financial XML file"))
}
fn plain(text: String) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&#038;", "&")
}
// Source SGML leaves have optional closing tags. Normalize only leaf closure;
// the XML library still validates the aggregate structure and entity limits.
fn sgml(input: &str) -> Result<String, Error> {
    let mut output = String::new();
    let mut position = 0;
    let mut skipped = None;
    let mut depth = 0usize;
    while let Some(offset) = input[position..].find('<') {
        let start = position + offset;
        let end = start
            + input[start..]
                .find('>')
                .ok_or_else(|| Error::invalid("Malformed OFX tag"))?;
        let tag = input[start + 1..end].trim().replace('.', "");
        let next = end + 1;
        let closing = tag.starts_with('/');
        let name = tag.trim_start_matches('/');
        if closing && skipped.as_deref() == Some(name) {
            skipped = None;
            position = next;
            continue;
        }
        skipped = None;
        if closing {
            depth = depth.saturating_sub(1);
        } else {
            depth += 1;
            if depth > 128 {
                return Err(Error::invalid("OFX nesting exceeds128 levels"));
            }
        }
        output.push('<');
        output.push_str(&tag);
        output.push('>');
        let stop = input[next..]
            .find('<')
            .map(|n| next + n)
            .unwrap_or(input.len());
        let value = input[next..stop].trim();
        if !closing && !value.is_empty() {
            output.push_str(&value.replace('&', "&amp;"));
            output.push_str("</");
            output.push_str(name);
            output.push('>');
            skipped = Some(name.to_owned());
            depth = depth.saturating_sub(1);
        }
        position = stop;
        if position == input.len() {
            break;
        }
    }
    Ok(output)
}
pub(super) fn ofx(bytes: &[u8], options: &parse::Options) -> Result<Vec<Value>, Error> {
    let decoded = decode(bytes, false);
    let Some(start) = decoded.find("<OFX>").or_else(|| decoded.find("<OFX >")) else {
        return Ok(vec![]);
    };
    let original = &decoded[start..];
    let normalized;
    let doc = match document(original) {
        Ok(doc) => doc,
        Err(_) => {
            normalized = sgml(original)?;
            document(&normalized)?
        }
    };
    let mut rows = vec![];
    for entry in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "STMTTRN")
    {
        let raw = text(entry, "TRNAMT").unwrap_or_default();
        let raw = raw.trim();
        let raw = if raw.starts_with('(') && raw.ends_with(')') {
            format!("-{}", &raw[1..raw.len() - 1])
        } else {
            raw.into()
        };
        let mut dot = false;
        let cleaned = raw
            .chars()
            .filter(|c| {
                if *c == '.' {
                    let keep = !dot;
                    dot = true;
                    keep
                } else {
                    c.is_ascii_digit() || *c == '-'
                }
            })
            .collect::<String>();
        let amount = parse::float_prefix(&cleaned)
            .filter(|n| n.is_finite())
            .ok_or_else(|| Error::invalid("Invalid OFX amount"))?;
        let raw_date =
            text(entry, "DTPOSTED").ok_or_else(|| Error::invalid("Missing OFX posting date"))?;
        let digits = raw_date
            .get(..8)
            .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| Error::invalid("Invalid OFX posting date"))?;
        let y = digits[..4].parse::<i32>().map_err(Error::internal)?;
        let m = digits[4..6].parse::<u32>().map_err(Error::internal)?;
        let d = digits[6..8].parse::<u64>().map_err(Error::internal)?;
        let base = chrono::NaiveDate::from_ymd_opt(y, 1, 1)
            .and_then(|date| date.checked_add_months(chrono::Months::new(m.saturating_sub(1))))
            .and_then(|date| date.checked_add_days(chrono::Days::new(d.saturating_sub(1))))
            .ok_or_else(|| Error::invalid("Invalid OFX date"))?;
        let (payee, notes, used) = swapped(
            text(entry, "NAME").map(|s| plain(s.trim().to_owned())),
            text(entry, "MEMO").map(|s| plain(s.trim().to_owned())),
            options,
            options.fallback_missing_payee_to_memo,
        );
        let mut row = json!({"amount":amount,"date":base.to_string(),"payee_name":payee,"imported_payee":payee,"notes":notes});
        if let Some(id) = text(entry, "FITID") {
            row["imported_id"] = json!(id.trim());
        }
        if used && row["payee_name"].is_null() {
            row.as_object_mut().unwrap().remove("payee_name");
            row.as_object_mut().unwrap().remove("imported_payee");
        }
        rows.push(row);
    }
    checked(rows)
}
fn descendant_text(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .and_then(|n| n.text())
        .map(str::to_owned)
}
fn detail(detail: Option<Node<'_, '_>>, debit: bool) -> (Option<String>, Option<String>) {
    let payee = detail
        .and_then(|n| path(n, &["RltdPties", if debit { "Cdtr" } else { "Dbtr" }]))
        .and_then(|n| descendant_text(n, "Nm"));
    let notes = detail
        .and_then(|n| child(n, "RmtInf"))
        .map(|r| {
            r.children()
                .filter(|n| n.is_element() && n.tag_name().name() == "Ustrd")
                .filter_map(|n| n.text())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.is_empty());
    (payee, notes)
}
pub(super) fn camt(bytes: &[u8], options: &parse::Options) -> Result<Vec<Value>, Error> {
    let decoded = decode(bytes, true);
    let doc = document(&decoded)?;
    let mut rows = vec![];
    for entry in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "Ntry")
    {
        let debit = text(entry, "CdtDbtInd").as_deref() == Some("DBIT");
        let date = ["ValDt", "BookgDt"].iter().find_map(|name| {
            child(entry, name).and_then(|n| {
                text(n, "DtTm")
                    .map(|s| s.chars().take(10).collect())
                    .or_else(|| text(n, "Dt"))
            })
        });
        let Some(date) = date else {
            continue;
        };
        let details = child(entry, "NtryDtls")
            .map(|n| {
                n.children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "TxDtls")
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let multiple = details.len() > 1;
        for detail_node in if multiple {
            details.into_iter().map(Some).collect()
        } else {
            vec![details.first().copied()]
        } {
            let amount = if multiple {
                detail_node.and_then(|n| descendant_text(n, "Amt"))
            } else {
                text(entry, "Amt")
            }
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
            if !amount.is_finite() {
                continue;
            }
            let (mut payee, mut notes) = detail(detail_node, debit);
            if !multiple {
                let additional = text(entry, "AddtlNtryInf");
                if payee.as_deref().and_then(|s| truth(Some(s))).is_none() {
                    payee = additional.clone();
                }
                if notes.as_deref().and_then(|s| truth(Some(s))).is_none() && additional != payee {
                    notes = additional;
                }
                if payee.is_none() && notes.is_none() {
                    notes = text(entry, "NtryRef");
                }
                if payee
                    .as_ref()
                    .zip(notes.as_ref())
                    .is_some_and(|(p, n)| p.contains(n))
                {
                    notes = None;
                }
            }
            let (payee, notes, _) = swapped(payee, notes, options, options.swap_payee_and_memo);
            let mut row = json!({"amount":if debit{-amount}else{amount},"date":date,"payee_name":payee,"imported_payee":payee,"notes":notes});
            if !multiple && let Some(id) = text(entry, "AcctSvcrRef").filter(|s| !s.is_empty()) {
                row["imported_id"] = json!(id);
            }
            rows.push(row);
        }
    }
    checked(rows)
}
