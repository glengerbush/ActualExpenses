//! Compatibility at the text-input boundary only. Stored ledger arithmetic remains integer.
use crate::{api::json_error, error::Error, models::MAX_MONEY};
use axum::{Json, extract::rejection::JsonRejection};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    expression: String,
    #[serde(default = "two")]
    decimal_places: u8,
}
fn two() -> u8 {
    2
}
pub async fn evaluate(input: Result<Json<Input>, JsonRejection>) -> Result<Json<Value>, Error> {
    let Json(input) = input.map_err(json_error)?;
    if input.expression.chars().count() > 256 || ![0, 2].contains(&input.decimal_places) {
        return Err(Error::invalid(
            "Use at most 256 characters and decimal_places 0 or 2",
        ));
    }
    let chars = input
        .expression
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let mut parser = Parser {
        chars,
        index: 0,
        depth: 0,
    };
    let value = parser.additive()? * 10_f64.powi(input.decimal_places as i32);
    if !value.is_finite() {
        return Err(Error::invalid("The calculation must have a finite result"));
    }
    // JavaScript Math.round chooses +infinity at exact halves, including negative halves.
    // Avoid (value+0.5).floor(): adding 0.5 itself rounds incorrectly near MAX_SAFE_INTEGER.
    let floor = value.floor();
    let amount = if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    if amount.abs() > MAX_MONEY as f64 {
        return Err(Error::invalid(
            "The calculated amount is outside the exact integer range",
        ));
    }
    Ok(Json(json!({"data":{"amount":amount as i64}})))
}
struct Parser {
    chars: Vec<char>,
    index: usize,
    depth: u32,
}
impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }
    fn take(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn additive(&mut self) -> Result<f64, Error> {
        let mut value = self.multiplicative()?;
        loop {
            if self.take('+') {
                value += self.multiplicative()?;
            } else if self.take('-') {
                value -= self.multiplicative()?;
            } else {
                break;
            }
        }
        Ok(value)
    }
    fn multiplicative(&mut self) -> Result<f64, Error> {
        let mut value = self.exponent()?;
        loop {
            if self.take('*') {
                value *= self.exponent()?;
            } else if self.take('/') {
                value /= self.exponent()?;
            } else {
                break;
            }
        }
        Ok(value)
    }
    fn exponent(&mut self) -> Result<f64, Error> {
        self.depth += 1;
        if self.depth > 32 {
            return Err(Error::invalid("The calculation is too deeply nested"));
        }
        let mut value = if self.take('(') {
            let value = self.additive()?;
            if !self.take(')') {
                return Err(Error::invalid("Close the calculation's parentheses"));
            }
            value
        } else {
            self.primary()?
        };
        if self.take('^') {
            value = value.powf(self.exponent()?);
        }
        self.depth -= 1;
        Ok(value)
    }
    fn primary(&mut self) -> Result<f64, Error> {
        // Actual applies a negative primary before exponentiation: -2^2 is (-2)^2.
        let negative = self.take('-');
        let start = self.index;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.index += 1;
        }
        let text: String = self.chars[start..self.index].iter().collect();
        // Actual's currency parser treats the last decimal mark as the fraction separator.
        let normalized = if let Some(last) = text.rfind('.') {
            format!("{}.{}", text[..last].replace('.', ""), &text[last + 1..])
        } else {
            text
        };
        let value = normalized
            .parse::<f64>()
            .map_err(|_| Error::invalid("Enter a number or calculation"))?;
        Ok(if negative { -value } else { value })
    }
}
