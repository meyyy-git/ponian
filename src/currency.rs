//! Kurs mata uang dari Frankfurter (data Bank Sentral Eropa, tanpa API key), disegarkan paling sering dua kali sehari.
use std::collections::HashMap;
use windows::{Data::Json::JsonObject, core::HSTRING};

pub const MAIN: [&str; 7] = ["IDR", "USD", "EUR", "SGD", "MYR", "JPY", "SAR"];

/// Kurs per 1 USD. USD sendiri bernilai 1.
pub type Rates = HashMap<String, f64>;

pub fn parse_rates(json: &str) -> Option<Rates> {
    let obj = JsonObject::Parse(&HSTRING::from(json)).ok()?;
    let rates = obj.GetNamedObject(&HSTRING::from("rates")).ok()?;
    let mut out = Rates::new();
    for pair in rates {
        let key = pair.Key().ok()?.to_string();
        if let Ok(v) = pair.Value().and_then(|v| v.GetNumber()) {
            out.insert(key, v);
        }
    }
    out.insert("USD".into(), 1.0);
    (out.len() > 1).then_some(out)
}

pub fn fetch() -> Option<Rates> {
    parse_rates(&crate::net::get("https://api.frankfurter.app/latest?from=USD")?)
}

pub fn convert(rates: &Rates, amount: f64, from: &str, to: &str) -> Option<f64> {
    Some(amount / rates.get(from)? * rates.get(to)?)
}

/// "100 usd to idr", "100 usd ke idr", "100usd idr", "50 eur =" -> (100, "USD", "IDR"). Tujuan kosong = mata uang utama.
pub fn parse_query(s: &str) -> Option<(f64, String, Option<String>)> {
    let s = s.trim().trim_end_matches('=').trim().to_lowercase();
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let amount: f64 = s[..split].trim().replace('.', "").replace(',', ".").parse().ok()?;
    let words: Vec<&str> = s[split..].split_whitespace().filter(|w| !["to", "ke", "in", "dalam", "jadi"].contains(w)).collect();
    let code = |w: &str| (w.len() == 3 && w.chars().all(|c| c.is_ascii_alphabetic())).then(|| w.to_uppercase());
    let from = code(words.first()?)?;
    let to = match words.get(1) {
        Some(w) => Some(code(w)?),
        None => None,
    };
    (words.len() <= 2).then_some((amount, from, to))
}

/// 16205.5 -> "16.205,50"; angka besar tanpa desimal.
pub fn format(v: f64, code: &str) -> String {
    let decimals = if v >= 1000.0 || code == "IDR" || code == "JPY" { 0 } else { 2 };
    let rounded = format!("{:.*}", decimals, v);
    let (int, frac) = rounded.split_once('.').unwrap_or((&rounded, ""));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    let symbol = match code {
        "IDR" => "Rp",
        "USD" => "$",
        "EUR" => "\u{20AC}",
        _ => "",
    };
    let suffix = if symbol.is_empty() { format!(" {code}") } else { String::new() };
    if frac.is_empty() { format!("{symbol}{grouped}{suffix}") } else { format!("{symbol}{grouped},{frac}{suffix}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_queries() {
        assert_eq!(parse_query("100 usd to idr"), Some((100.0, "USD".into(), Some("IDR".into()))));
        assert_eq!(parse_query("1.500 idr ke usd ="), Some((1500.0, "IDR".into(), Some("USD".into()))));
        assert_eq!(parse_query("2,5 eur"), Some((2.5, "EUR".into(), None)));
        assert_eq!(parse_query("berapa kurs dolar"), None);
        assert_eq!(parse_query("100 dollar to rupiah"), None);
    }

    #[test]
    fn converts_and_formats() {
        let rates: Rates = [("USD".to_string(), 1.0), ("IDR".to_string(), 16205.5), ("EUR".to_string(), 0.92)].into();
        let v = convert(&rates, 100.0, "USD", "IDR").unwrap();
        assert_eq!(format(v, "IDR"), "Rp1.620.550");
        assert_eq!(format(convert(&rates, 1.0, "EUR", "USD").unwrap(), "USD"), "$1,09");
        assert_eq!(format(12.5, "SGD"), "12,50 SGD");
        assert!(parse_rates(r#"{"amount":1.0,"base":"USD","rates":{"IDR":16205.5,"EUR":0.92}}"#).unwrap().contains_key("IDR"));
    }
}
