//! Resolve only the locale pattern used by source formula preferences. The
//! trusted source functions still perform all number/currency formatting.
use crate::error::Error;
use icu_decimal::{DecimalFormatter, parts};
use icu_locale::Locale;
use std::fmt;
use writeable::{Part, PartsWrite, Writeable};

#[derive(Default)]
struct Separators {
    text: String,
    group: String,
    decimal: String,
}
impl fmt::Write for Separators {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.text.push_str(text);
        Ok(())
    }
}
impl PartsWrite for Separators {
    type SubPartsWrite = Self;
    fn with_part(
        &mut self,
        part: Part,
        mut f: impl FnMut(&mut Self) -> fmt::Result,
    ) -> fmt::Result {
        let start = self.text.len();
        f(self)?;
        if part == parts::GROUP && self.group.is_empty() {
            self.group = self.text[start..].into();
        } else if part == parts::DECIMAL {
            self.decimal = self.text[start..].into();
        }
        Ok(())
    }
}
pub(super) fn locale_format(locale: &str) -> Result<&'static str, Error> {
    let locale: Locale = locale
        .parse()
        .map_err(|_| Error::invalid("Invalid formula locale"))?;
    let formatter = DecimalFormatter::try_new(locale.into(), Default::default())
        .map_err(|_| Error::invalid("Unsupported formula locale"))?;
    let mut separators = Separators::default();
    formatter
        .format(&"1234567.89".parse().unwrap())
        .write_to_parts(&mut separators)
        .map_err(Error::internal)?;
    Ok(
        match (separators.group.as_str(), separators.decimal.as_str()) {
            (".", ",") => "dot-comma",
            (group, ",") if !group.is_empty() && group.trim().is_empty() => "space-comma",
            ("'" | "’", ".") => "apostrophe-dot",
            _ => "comma-dot",
        },
    )
}
