use std::collections::BTreeMap;
pub const MAX_CSS: usize = 16 * 1024 * 1024;
const MAX_FONT: usize = 2 * 1024 * 1024;
const MAX_FONTS: usize = 10 * 1024 * 1024;
pub struct Parsed {
    pub variables: BTreeMap<String, String>,
    pub fonts: Vec<String>,
}
pub fn name(value: &str) -> bool {
    value.starts_with("--")
        && value.len() > 2
        && !value.ends_with('-')
        && value[2..]
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
pub fn reference(value: &str) -> Option<&str> {
    if !value
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("var"))
    {
        return None;
    }
    let body = value[3..]
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')?
        .trim();
    name(body).then_some(body)
}
fn number(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|c| c.is_ascii_digit() || c == b'.')
        && value.chars().filter(|c| *c == '.').count() <= 1
        && value.bytes().any(|c| c.is_ascii_digit())
}
pub fn valid_value(property: &str, value: &str) -> bool {
    if property.starts_with("--font-") {
        return !value.is_empty()
            && value
                .split(',')
                .all(|v| !v.trim().is_empty() && !v.chars().any(|c| "(){};<>@\\".contains(c)));
    }
    if value.is_empty() || reference(value).is_some() {
        return true;
    }
    if let Some(hex) = value.strip_prefix('#') {
        return [3, 6, 8].contains(&hex.len()) && hex.bytes().all(|c| c.is_ascii_hexdigit());
    }
    for function in ["rgb", "rgba", "hsl", "hsla"] {
        if let Some(body) = value
            .strip_prefix(&format!("{function}("))
            .and_then(|v| v.strip_suffix(')'))
        {
            let args: Vec<_> = body.split(',').map(str::trim).collect();
            return (args.len() == 3 || args.len() == 4)
                && args
                    .iter()
                    .all(|v| number(v.strip_suffix('%').unwrap_or(v)));
        }
    }
    if number(value)
        || [
            "inherit",
            "initial",
            "unset",
            "revert",
            "transparent",
            "none",
            "auto",
            "normal",
        ]
        .contains(&value.to_ascii_lowercase().as_str())
    {
        return true;
    }
    [
        "px", "em", "rem", "%", "vh", "vw", "vmin", "vmax", "cm", "mm", "in", "pt", "pc", "ex",
        "ch",
    ]
    .iter()
    .any(|unit| value.strip_suffix(unit).is_some_and(number))
}
fn comments(css: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        result.push_str(&rest[..start]);
        let end = rest[start + 2..].find("*/").ok_or("Unclosed CSS comment")?;
        rest = &rest[start + 2 + end + 2..];
    }
    result.push_str(rest);
    Ok(result)
}
fn declarations(input: &str) -> Result<Vec<(&str, &str)>, String> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut depth = 0usize;
    for (index, c) in input.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else {
            match c {
                '\'' | '"' => quote = Some(c),
                '(' => depth += 1,
                ')' => {
                    depth = depth.checked_sub(1).ok_or("Unbalanced CSS parentheses")?;
                }
                ';' if depth == 0 => {
                    let declaration = input[start..index].trim();
                    if !declaration.is_empty() {
                        result.push(pair(declaration)?);
                    }
                    start = index + 1;
                }
                '{' | '}' | '<' => return Err("Nested blocks/selectors are unsupported".into()),
                _ => {}
            }
        }
    }
    if quote.is_some() || depth != 0 {
        return Err("Unclosed CSS string or parentheses".into());
    }
    let last = input[start..].trim();
    if !last.is_empty() {
        result.push(pair(last)?);
    }
    if result.len() > 4096 {
        return Err("Too many theme declarations".into());
    }
    Ok(result)
}
fn pair(input: &str) -> Result<(&str, &str), String> {
    let (property, value) = input.split_once(':').ok_or("Invalid CSS declaration")?;
    Ok((property.trim(), value.trim()))
}
fn block(input: &str) -> Result<(&str, &str), String> {
    let input = input
        .trim_start()
        .strip_prefix('{')
        .ok_or("Expected CSS block")?;
    let end = input.find('}').ok_or("Unclosed CSS block")?;
    let content = &input[..end];
    if content.contains('{') {
        return Err("Nested CSS blocks are unsupported".into());
    }
    Ok((content, input[end + 1..].trim()))
}
fn font(block: &str, total: &mut usize) -> Result<(), String> {
    for (property, value) in declarations(block)? {
        match property {
            "src" => {
                let mut rest = value;
                let mut found = false;
                while let Some(start) = rest.find("url(") {
                    let tail = &rest[start + 4..];
                    let end = tail.find(')').ok_or("Unclosed font URL")?;
                    let uri = tail[..end].trim().trim_matches(['\'', '"']);
                    if !uri.starts_with("data:") {
                        return Err("Only embedded data fonts are supported; remote/relative font URLs must be embedded before compilation".into());
                    }
                    let (mime, encoded) = uri.split_once(',').ok_or("Invalid embedded font")?;
                    if !mime.contains(";base64")
                        || !encoded
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"+/=\r\n".contains(&c))
                    {
                        return Err("Invalid base64 font data".into());
                    }
                    let bytes = encoded.len().saturating_mul(3).div_ceil(4);
                    if bytes > MAX_FONT {
                        return Err("Font exceeds 2 MiB limit".into());
                    }
                    *total = total.saturating_add(bytes);
                    if *total > MAX_FONTS {
                        return Err("Embedded fonts exceed 10 MiB limit".into());
                    }
                    found = true;
                    rest = tail[end + 1..].trim_start();
                    if let Some(format) = rest.strip_prefix("format(") {
                        let end = format.find(')').ok_or("Unclosed font format")?;
                        let format_name = format[..end].trim().trim_matches(['\'', '"']);
                        if !["woff", "woff2", "truetype", "opentype"].contains(&format_name) {
                            return Err("Unsupported font format".into());
                        }
                        rest = format[end + 1..].trim_start();
                    }
                    if let Some(next) = rest.strip_prefix(',') {
                        rest = next.trim_start();
                    } else if !rest.is_empty() {
                        return Err("Unsupported font src".into());
                    }
                }
                if !found || !rest.trim().is_empty() {
                    return Err("Font src must contain embedded url() data".into());
                }
            }
            "font-family" => {
                if !valid_value("--font-family", value) {
                    return Err("Invalid font family".into());
                }
            }
            "font-style" | "font-weight" | "font-display" | "font-stretch" | "unicode-range" => {
                if !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || " -,+.%'\"".contains(c))
                {
                    return Err("Unsupported font descriptor".into());
                }
            }
            _ => return Err(format!("Unsupported font descriptor: {property}")),
        }
    }
    Ok(())
}
pub fn parse(css: &str) -> Result<Parsed, String> {
    if css.len() > MAX_CSS {
        return Err("Theme exceeds 16 MiB limit".into());
    }
    let cleaned = comments(css)?;
    let mut rest = cleaned.trim();
    let mut fonts = Vec::new();
    let mut total = 0;
    while let Some(after) = rest.strip_prefix("@font-face") {
        let (content, remaining) = block(after)?;
        font(content, &mut total)?;
        fonts.push(format!("@font-face {{{content}}}"));
        rest = remaining;
    }
    let after = rest
        .strip_prefix(":root")
        .ok_or("Theme must contain one :root variable block")?;
    let (content, remaining) = block(after)?;
    if !remaining.is_empty() {
        return Err("Only one :root block and embedded fonts are supported".into());
    }
    let mut variables = BTreeMap::new();
    for (property, setting) in declarations(content)? {
        if !name(property) || !valid_value(property, setting) {
            return Err(format!("Unsupported value or property: {property}"));
        }
        variables.insert(
            property.into(),
            reference(setting)
                .map(|name| format!("var({name})"))
                .unwrap_or_else(|| setting.into()),
        );
    }
    Ok(Parsed { variables, fonts })
}
