//! Cuaca dari MET Norway (api.met.no), lisensi CC BY 4.0. Diminta paling sering tiap 30 menit.
use windows::{Data::Json::JsonObject, core::HSTRING};

#[derive(Clone, Debug, PartialEq)]
pub struct Weather {
    pub temp: f64,
    pub symbol: String,
    pub min: f64,
    pub max: f64,
}

pub fn parse(json: &str) -> Option<Weather> {
    let h = |s: &str| HSTRING::from(s);
    let series = JsonObject::Parse(&h(json)).ok()?.GetNamedObject(&h("properties")).ok()?.GetNamedArray(&h("timeseries")).ok()?;
    let temp_at = |i: u32| -> Option<f64> { series.GetObjectAt(i).ok()?.GetNamedObject(&h("data")).ok()?.GetNamedObject(&h("instant")).ok()?.GetNamedObject(&h("details")).ok()?.GetNamedNumber(&h("air_temperature")).ok() };
    let first = series.GetObjectAt(0).ok()?.GetNamedObject(&h("data")).ok()?;
    let symbol = ["next_1_hours", "next_6_hours"]
        .iter()
        .find_map(|k| first.GetNamedObject(&h(k)).ok()?.GetNamedObject(&h("summary")).ok()?.GetNamedString(&h("symbol_code")).ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let temp = temp_at(0)?;
    // 24 jam ke depan: data per jam di awal deret waktu
    let next: Vec<f64> = (0..24.min(series.Size().ok()?)).filter_map(temp_at).collect();
    let min = next.iter().copied().fold(temp, f64::min);
    let max = next.iter().copied().fold(temp, f64::max);
    Some(Weather { temp, symbol, min, max })
}

pub fn fetch(lat: f64, lon: f64) -> Option<Weather> {
    // MET meminta koordinat maksimal 4 desimal supaya cache mereka efektif
    parse(&crate::net::get(&format!("https://api.met.no/weatherapi/locationforecast/2.0/compact?lat={lat:.4}&lon={lon:.4}"))?)
}

/// (keterangan, emoji) dari kode simbol MET, misalnya "partlycloudy_day".
/// Emoji dipakai karena font ikon Segoe MDL2 tidak punya glyph cuaca.
pub fn describe(symbol: &str) -> (&'static str, &'static str) {
    let base = symbol.split('_').next().unwrap_or("");
    let night = symbol.ends_with("_night");
    match base {
        "clearsky" if night => ("Cerah", "\u{1F319}"),
        "clearsky" => ("Cerah", "\u{2600}\u{FE0F}"),
        "fair" => ("Cerah berawan", "\u{1F324}\u{FE0F}"),
        "partlycloudy" => ("Berawan sebagian", "\u{26C5}"),
        "cloudy" => ("Berawan", "\u{2601}\u{FE0F}"),
        "fog" => ("Berkabut", "\u{1F32B}\u{FE0F}"),
        b if b.contains("thunder") => ("Badai petir", "\u{26C8}\u{FE0F}"),
        b if b.contains("heavyrain") => ("Hujan lebat", "\u{1F327}\u{FE0F}"),
        b if b.contains("lightrain") => ("Hujan ringan", "\u{1F326}\u{FE0F}"),
        b if b.contains("rain") => ("Hujan", "\u{1F327}\u{FE0F}"),
        b if b.contains("sleet") => ("Hujan es", "\u{1F328}\u{FE0F}"),
        b if b.contains("snow") => ("Salju", "\u{2744}\u{FE0F}"),
        _ => ("", "\u{2601}\u{FE0F}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_met_compact() {
        let json = r#"{"properties":{"timeseries":[
            {"time":"2026-11-21T07:00:00Z","data":{"instant":{"details":{"air_temperature":27.4}},"next_1_hours":{"summary":{"symbol_code":"partlycloudy_day"}}}},
            {"time":"2026-11-21T08:00:00Z","data":{"instant":{"details":{"air_temperature":29.1}}}},
            {"time":"2026-11-21T09:00:00Z","data":{"instant":{"details":{"air_temperature":24.0}}}}]}}"#;
        assert_eq!(parse(json), Some(Weather { temp: 27.4, symbol: "partlycloudy_day".into(), min: 24.0, max: 29.1 }));
        assert_eq!(describe("partlycloudy_day").0, "Berawan sebagian");
        assert_eq!(describe("lightrainshowers_night").0, "Hujan ringan");
    }

    /// Butuh internet: `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn met_live() {
        let raw = crate::net::get("https://api.met.no/weatherapi/locationforecast/2.0/compact?lat=-6.1754&lon=106.8272");
        assert!(raw.is_some(), "HTTP gagal");
        assert!(parse(&raw.unwrap()).is_some(), "parse gagal");
    }
}
