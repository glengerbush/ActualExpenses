//! File syntax and field mapping are independent of ledger mutation.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Options {
    pub has_header_row: bool,
    pub delimiter: String,
    pub encoding: String,
    pub skip_start_lines: i64,
    pub skip_end_lines: i64,
    pub date_format: String,
    pub split_mode: bool,
    pub in_out_mode: bool,
    pub out_value: String,
    pub flip_amount: bool,
    pub multiplier: String,
    pub import_notes: bool,
    pub clear_on_import: bool,
    pub reconcile: bool,
    pub reimport_deleted: bool,
    pub fallback_missing_payee_to_memo: bool,
    pub swap_payee_and_memo: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            has_header_row: true,
            delimiter: ",".into(),
            encoding: "auto".into(),
            skip_start_lines: 0,
            skip_end_lines: 0,
            date_format: "yyyy mm dd".into(),
            split_mode: false,
            in_out_mode: false,
            out_value: String::new(),
            flip_amount: false,
            multiplier: String::new(),
            import_notes: true,
            clear_on_import: true,
            reconcile: true,
            reimport_deleted: true,
            fallback_missing_payee_to_memo: false,
            swap_payee_and_memo: false,
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Mapping {
    pub date: Option<String>,
    pub payee: Option<String>,
    pub amount: Option<String>,
    pub notes: Option<String>,
    pub category: Option<String>,
    pub inflow: Option<String>,
    pub outflow: Option<String>,
    pub in_out: Option<String>,
}
// csv-parse trims outside quoted fields and rejects stray/unclosed quotes.
// Normalize only that syntax before the mature CSV decoder; quoted contents
// remain byte-for-byte intact, including whitespace and embedded newlines.
fn csv_syntax(text: &str, delimiter: char) -> Result<String, Error> {
    let mut out = String::with_capacity(text.len());
    let mut spaces = String::new();
    let mut state = 0;
    let mut after_space = false;
    let whitespace = |c: char| matches!(c, ' ' | '\t' | '\x0b' | '\x0c');
    for c in text.chars() {
        match state {
            0 => {
                if c == delimiter || matches!(c, '\r' | '\n') {
                    out.push(c);
                } else if whitespace(c) {
                } else if c == '"' {
                    out.push(c);
                    state = 2;
                } else {
                    out.push(c);
                    state = 1;
                }
            }
            1 => {
                if c == '"' {
                    return Err(Error::invalid(
                        "Unexpected quote inside an unquoted CSV field",
                    ));
                }
                if c == delimiter || matches!(c, '\r' | '\n') {
                    spaces.clear();
                    out.push(c);
                    state = 0;
                } else if whitespace(c) {
                    spaces.push(c);
                } else {
                    out.push_str(&spaces);
                    spaces.clear();
                    out.push(c);
                }
            }
            2 => {
                out.push(c);
                if c == '"' {
                    state = 3;
                    after_space = false;
                }
            }
            _ => {
                if c == '"' && !after_space {
                    out.push(c);
                    state = 2;
                } else if c == delimiter || matches!(c, '\r' | '\n') {
                    out.push(c);
                    state = 0;
                } else if whitespace(c) {
                    after_space = true;
                } else {
                    return Err(Error::invalid(
                        "Unexpected character after a quoted CSV field",
                    ));
                }
            }
        }
    }
    if state == 2 {
        return Err(Error::invalid("The CSV file ends inside a quoted field"));
    }
    Ok(out)
}
pub(super) fn csv(bytes: &[u8], options: &Options) -> Result<Vec<Value>, Error> {
    if options.delimiter.len() != 1 || b"\r\n\"".contains(&options.delimiter.as_bytes()[0]) {
        return Err(Error::invalid("Choose a single CSV delimiter"));
    }
    let encoding = if options.encoding == "auto" {
        if bytes.starts_with(&[0xff, 0xfe]) {
            encoding_rs::UTF_16LE
        } else if bytes.starts_with(&[0xfe, 0xff]) {
            encoding_rs::UTF_16BE
        } else {
            encoding_rs::UTF_8
        }
    } else {
        encoding_rs::Encoding::for_label(options.encoding.as_bytes())
            .ok_or_else(|| Error::invalid("Unknown file encoding"))?
    };
    let (text, _) = encoding.decode_without_bom_handling(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    if text.len() > MAX_FILE {
        return Err(Error::invalid("Decoded file exceeds16 MiB"));
    }
    let skip_start = options.skip_start_lines.max(0) as usize;
    let skip_end = options.skip_end_lines.max(0) as usize;
    let text = if skip_start > 0 || skip_end > 0 {
        let lines = text
            .split('\n')
            .map(|s| s.strip_suffix('\r').unwrap_or(s))
            .collect::<Vec<_>>();
        if skip_start.saturating_add(skip_end) >= lines.len() {
            return Err(Error::invalid(
                "Cannot skip more lines than exist in the file",
            ));
        }
        lines[skip_start..lines.len() - skip_end].join("\r\n")
    } else {
        text.into()
    };
    let text = csv_syntax(&text, options.delimiter.as_bytes()[0] as char)?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .trim(csv::Trim::None)
        .delimiter(options.delimiter.as_bytes()[0])
        .from_reader(text.as_bytes());
    let mut rows = vec![];
    let mut headers: Option<Vec<String>> = None;
    let mut cells = 0usize;
    for record in reader.records() {
        let record = record.map_err(|e| Error::invalid(format!("Unable to parse CSV: {e}")))?;
        if record.len() > 512 {
            return Err(Error::invalid("CSV rows contain at most512 fields"));
        }
        cells = cells.saturating_add(record.len());
        if cells > 2_000_000 || rows.len() >= 100_000 {
            return Err(Error::invalid(
                "File exceeds the bounded row/cell limit. Split it into smaller files.",
            ));
        }
        if options.has_header_row && headers.is_none() {
            headers = Some(record.iter().map(str::to_owned).collect());
            continue;
        }
        let mut row = serde_json::Map::new();
        for (i, cell) in record.iter().enumerate() {
            if cell.chars().count() > 10000 {
                return Err(Error::invalid(
                    "File fields contain at most10000 characters",
                ));
            }
            if let Some(headers) = &headers {
                if let Some(key) = headers.get(i) {
                    row.insert(key.clone(), json!(cell));
                }
            } else {
                row.insert(i.to_string(), json!(cell));
            }
        }
        rows.push(if options.has_header_row {
            Value::Object(row)
        } else {
            json!(record.iter().collect::<Vec<_>>())
        });
    }
    Ok(rows)
}
fn field<'a>(row: &'a Value, mapping: &Option<String>, fallback: &str) -> Option<&'a str> {
    let key = mapping
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback);
    if let Some(row) = row.as_array() {
        row.get(key.parse::<usize>().ok()?)?.as_str()
    } else {
        row[key].as_str()
    }
}
/// JavaScript parseFloat accepts a valid prefix, unlike Rust's full-string parse.
pub(super) fn float_prefix(raw: &str) -> Option<f64> {
    let raw = raw.trim_matches(crate::payees_tags::js_whitespace);
    let bytes = raw.as_bytes();
    let mut i = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mut digits = 0;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
        digits += 1;
    }
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    let mut end = i;
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i > start {
            end = i;
        }
    }
    raw[..end].parse().ok()
}

pub(super) fn loose(raw: Option<&str>) -> f64 {
    let Some(raw) = raw else {
        return 0.0;
    };
    let mut raw = raw
        .trim_matches(crate::payees_tags::js_whitespace)
        .to_owned();
    if raw.starts_with('(') && raw.ends_with(')') {
        raw = raw.replace('\u{2212}', "");
        raw = format!("-{}", &raw[1..raw.len() - 1]);
    } else {
        raw = raw.replace('\u{2212}', "-");
    }
    let places = raw.rfind([',', '.']).filter(|i| {
        let tail = &raw[i + 1..];
        !tail.contains([',', '.']) && matches!(tail.chars().count(), 1 | 2 | 4..=9)
    });
    let digits = |s: &str| {
        s.chars()
            .filter(|c| c.is_ascii_digit() || *c == '-')
            .collect::<String>()
    };
    let raw = if let Some(i) = places {
        format!("{}.{}", digits(&raw[..i]), digits(&raw[i + 1..]))
    } else {
        digits(&raw)
    };
    float_prefix(&raw)
        .filter(|n| n.is_finite() && n.abs() * 100.0 <= crate::models::MAX_MONEY as f64)
        .unwrap_or(0.0)
}
fn month_names(raw: &str) -> String {
    let groups = [
        ("01", vec!["january", "jan.", "jan"]),
        ("02", vec!["february", "feb.", "feb"]),
        ("03", vec!["march", "mar.", "mar"]),
        ("04", vec!["april", "apr.", "apr"]),
        ("05", vec!["may.", "may"]),
        ("06", vec!["june", "jun.", "jun"]),
        ("07", vec!["july", "jul.", "jul"]),
        ("08", vec!["august", "aug.", "aug"]),
        ("09", vec!["september", "sept.", "sept", "sep.", "sep"]),
        ("10", vec!["october", "oct.", "oct"]),
        ("11", vec!["november", "nov.", "nov"]),
        ("12", vec!["december", "dec.", "dec"]),
    ];
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut result = raw.to_owned();
    for (number, names) in groups {
        let found = result.char_indices().find_map(|(i, _)| {
            if i > 0 && word(result.as_bytes()[i - 1]) {
                return None;
            }
            for name in &names {
                let end = i + name.len();
                if result
                    .get(i..end)
                    .is_some_and(|s| s.eq_ignore_ascii_case(name))
                    && word(*name.as_bytes().last().unwrap())
                        != result.as_bytes().get(end).copied().is_some_and(word)
                {
                    return Some((i, end));
                }
            }
            None
        });
        if let Some((start, end)) = found {
            result.replace_range(start..end, number);
        }
    }
    result
}
fn date(raw: &str, order: &str) -> Result<String, Error> {
    let normalized = month_names(raw);
    let raw = normalized.as_str();
    let mut groups = raw
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if groups.len() < 3 {
        let digits = raw.chars().filter(char::is_ascii_digit).collect::<String>();
        let first = if order == "yyyy mm dd" { 4 } else { 2 };
        if digits.len() < first + 3 {
            return Err(Error::invalid("Choose the file's date format"));
        }
        groups = vec![
            digits[..first].into(),
            digits[first..first + 2].into(),
            digits[first + 2..].into(),
        ];
    }
    let (y, m, d) = match order {
        "yyyy mm dd" => (groups[0].clone(), &groups[1], &groups[2]),
        "yy mm dd" => (format!("20{}", groups[0]), &groups[1], &groups[2]),
        "mm dd yyyy" => (groups[2].clone(), &groups[0], &groups[1]),
        "mm dd yy" => (format!("20{}", groups[2]), &groups[0], &groups[1]),
        "dd mm yyyy" => (groups[2].clone(), &groups[1], &groups[0]),
        "dd mm yy" => (format!("20{}", groups[2]), &groups[1], &groups[0]),
        _ => return Err(Error::invalid("Choose a supported date format")),
    };
    let result = format!("{y}-{:0>2}-{:0>2}", m, d);
    crate::models::validate_date(&result)?;
    Ok(result)
}
pub(super) fn mapped(row: &Value, map: &Mapping, options: &Options) -> Result<Value, Error> {
    let out = loose(field(row, &map.outflow, "outflow"));
    let input = loose(field(row, &map.inflow, "inflow"));
    let mut amount = if options.split_mode && !options.in_out_mode {
        if out != 0.0 { -out.abs() } else { input.abs() }
    } else {
        loose(field(row, &map.amount, "amount"))
    };
    if options.in_out_mode {
        amount = if field(row, &map.in_out, "inOut") == Some(options.out_value.as_str()) {
            -amount.abs()
        } else {
            amount.abs()
        };
    }
    if options.flip_amount {
        amount = -amount;
    }
    let multiplier = float_prefix(&options.multiplier)
        .filter(|n| *n != 0.0)
        .unwrap_or(1.0);
    amount *= multiplier;
    let scaled = amount * 100.0;
    let floor = scaled.floor();
    let rounded = if scaled - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    if !rounded.is_finite() || rounded.abs() > crate::models::MAX_MONEY as f64 {
        return Err(Error::invalid(
            "Imported amount exceeds the exact integer range",
        ));
    }
    Ok(
        json!({"date":date(field(row,&map.date,"date").unwrap_or(""),&options.date_format)?,"amount":rounded as i64,"payee_name":field(row,&map.payee,"payee_name"),"category_name":field(row,&map.category,"category"),"notes":if options.import_notes{field(row,&map.notes,"notes").unwrap_or("")}else{""},"cleared":options.clear_on_import}),
    )
}

pub(super) fn format(filename: &str) -> &str {
    filename.rsplit('.').next().unwrap_or("")
}
pub(super) fn file(filename: &str, bytes: &[u8], options: &Options) -> Result<Vec<Value>, Error> {
    match format(&filename.to_ascii_lowercase()) {
        "csv" | "tsv" => csv(bytes, options),
        "qif" => super::formats::qif(bytes, options),
        "ofx" | "qfx" => super::formats::ofx(bytes, options),
        "xml" => super::formats::camt(bytes, options),
        _ => Err(Error::invalid("Invalid file type")),
    }
}
pub(super) fn structured(row: &Value, kind: &str, options: &Options) -> Result<Value, Error> {
    let mut result = mapped(
        &json!({"date":if kind=="qif"{row["date"].clone()}else{json!(row["date"].as_str().unwrap_or(""))},"payee_name":row["payee_name"],"amount":"0","notes":row["notes"],"category":row["category"]}),
        &Mapping::default(),
        &Options {
            date_format: if kind == "qif" {
                options.date_format.clone()
            } else {
                "yyyy mm dd".into()
            },
            split_mode: false,
            in_out_mode: false,
            ..options.clone()
        },
    )?;
    let mut amount = row["amount"]
        .as_f64()
        .ok_or_else(|| Error::invalid("Invalid financial file amount"))?;
    if options.flip_amount {
        amount = -amount;
    }
    let multiplier = float_prefix(&options.multiplier)
        .filter(|n| *n != 0.0)
        .unwrap_or(1.0);
    let scaled = amount * multiplier * 100.0;
    let floor = scaled.floor();
    let rounded = if scaled - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    if !rounded.is_finite() || rounded.abs() > crate::models::MAX_MONEY as f64 {
        return Err(Error::invalid(
            "Imported amount exceeds the exact integer range",
        ));
    }
    result["amount"] = json!(rounded as i64);
    for field in ["imported_id", "raw_synced_data", "pending"] {
        if let Some(value) = row.get(field) {
            result[field] = value.clone();
        }
    }
    Ok(result)
}
