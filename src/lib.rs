#[allow(warnings)]
mod bindings;

use bindings::exports::rayslash::module::provider::Guest;
use bindings::rayslash::module::{
    host,
    types::{Action, Icon, ModuleError, QueryContext, QueryResponse, ResultItem},
};
use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

const PLACE_CACHE_TTL_SECONDS: u64 = 7 * 24 * 60 * 60;

struct Component;

#[derive(Deserialize)]
struct GeocodingResponse {
    #[serde(default)]
    results: Vec<ApiPlace>,
}

#[derive(Deserialize)]
struct ApiPlace {
    name: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    country_code: String,
    #[serde(default)]
    feature_code: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct Place {
    name: String,
    #[serde(default)]
    country: String,
    timezone: String,
}

#[derive(Deserialize, Serialize)]
struct CachedPlaces {
    query: String,
    fetched_at: u64,
    places: Vec<Place>,
}

impl Guest for Component {
    fn query(context: QueryContext) -> Result<QueryResponse, ModuleError> {
        let Some(place_query) = parse(&context.query) else {
            return Ok(QueryResponse {
                results: Vec::new(),
                exclusive: false,
            });
        };
        match find_places(place_query) {
            Ok(places) => {
                let now = DateTime::<Utc>::from_timestamp(host::unix_time() as i64, 0)
                    .ok_or_else(|| ModuleError::Internal("host timestamp is invalid".into()))?;
                let results = places
                    .into_iter()
                    .take(context.max_results as usize)
                    .filter_map(|place| result_for_place(place, now))
                    .collect::<Vec<_>>();
                Ok(QueryResponse {
                    exclusive: !results.is_empty(),
                    results,
                })
            }
            Err(message) => Ok(QueryResponse {
                results: vec![ResultItem {
                    id: "time:error".into(),
                    title: message.clone(),
                    subtitle: "Open-Meteo geocoding is temporarily unavailable".into(),
                    icon: Icon::PackagePath("icon.svg".into()),
                    score: None,
                    action: Action::ShowMessage(message),
                }],
                exclusive: true,
            }),
        }
    }
}

fn parse(query: &str) -> Option<&str> {
    let query = query.trim();
    let prefix = "time in ";
    (query.len() > prefix.len() && query[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| query[prefix.len()..].trim())
        .filter(|place| !place.is_empty())
}

fn find_places(query: &str) -> Result<Vec<Place>, String> {
    let normalized = query.trim().to_lowercase();
    if let Some(places) = common_places(&normalized) {
        return Ok(places);
    }
    let key = place_cache_key(&normalized);
    let now = host::unix_time();
    let cached = host::cache_get(&key)
        .ok()
        .flatten()
        .and_then(|bytes| serde_json::from_slice::<CachedPlaces>(&bytes).ok())
        .filter(|cached| cached.query == normalized);
    if let Some(cached) = cached.as_ref()
        && now.saturating_sub(cached.fetched_at) <= PLACE_CACHE_TTL_SECONDS
    {
        return Ok(cached.places.clone());
    }

    match request_places(query) {
        Ok(places) => {
            let cached = CachedPlaces {
                query: normalized,
                fetched_at: now,
                places: places.clone(),
            };
            if let Ok(bytes) = serde_json::to_vec(&cached) {
                let _ = host::cache_put(&key, &bytes);
            }
            Ok(places)
        }
        Err(error) => cached.map(|cached| cached.places).ok_or(error),
    }
}

fn common_places(query: &str) -> Option<Vec<Place>> {
    let normalized = normalize_name(query);
    let city = match normalized.as_str() {
        "amsterdam" => ("Amsterdam", "Netherlands", "Europe/Amsterdam"),
        "athens" => ("Athens", "Greece", "Europe/Athens"),
        "auckland" => ("Auckland", "New Zealand", "Pacific/Auckland"),
        "bangkok" => ("Bangkok", "Thailand", "Asia/Bangkok"),
        "beijing" => ("Beijing", "China", "Asia/Shanghai"),
        "berlin" => ("Berlin", "Germany", "Europe/Berlin"),
        "bogota" => ("Bogotá", "Colombia", "America/Bogota"),
        "buenosaires" => (
            "Buenos Aires",
            "Argentina",
            "America/Argentina/Buenos_Aires",
        ),
        "cairo" => ("Cairo", "Egypt", "Africa/Cairo"),
        "capetown" => ("Cape Town", "South Africa", "Africa/Johannesburg"),
        "chicago" => ("Chicago", "United States", "America/Chicago"),
        "delhi" | "newdelhi" => ("New Delhi", "India", "Asia/Kolkata"),
        "denver" => ("Denver", "United States", "America/Denver"),
        "dubai" => ("Dubai", "United Arab Emirates", "Asia/Dubai"),
        "dublin" => ("Dublin", "Ireland", "Europe/Dublin"),
        "helsinki" => ("Helsinki", "Finland", "Europe/Helsinki"),
        "hongkong" => ("Hong Kong", "Hong Kong", "Asia/Hong_Kong"),
        "honolulu" => ("Honolulu", "United States", "Pacific/Honolulu"),
        "istanbul" => ("Istanbul", "Türkiye", "Europe/Istanbul"),
        "jakarta" => ("Jakarta", "Indonesia", "Asia/Jakarta"),
        "johannesburg" => ("Johannesburg", "South Africa", "Africa/Johannesburg"),
        "karachi" => ("Karachi", "Pakistan", "Asia/Karachi"),
        "lagos" => ("Lagos", "Nigeria", "Africa/Lagos"),
        "lima" => ("Lima", "Peru", "America/Lima"),
        "lisbon" => ("Lisbon", "Portugal", "Europe/Lisbon"),
        "london" => ("London", "United Kingdom", "Europe/London"),
        "losangeles" | "la" => ("Los Angeles", "United States", "America/Los_Angeles"),
        "madrid" => ("Madrid", "Spain", "Europe/Madrid"),
        "manila" => ("Manila", "Philippines", "Asia/Manila"),
        "melbourne" => ("Melbourne", "Australia", "Australia/Melbourne"),
        "mexicocity" => ("Mexico City", "Mexico", "America/Mexico_City"),
        "moscow" => ("Moscow", "Russia", "Europe/Moscow"),
        "mumbai" => ("Mumbai", "India", "Asia/Kolkata"),
        "nairobi" => ("Nairobi", "Kenya", "Africa/Nairobi"),
        "newyork" | "newyorkcity" | "nyc" => ("New York", "United States", "America/New_York"),
        "oslo" => ("Oslo", "Norway", "Europe/Oslo"),
        "paris" => ("Paris", "France", "Europe/Paris"),
        "reykjavik" => ("Reykjavík", "Iceland", "Atlantic/Reykjavik"),
        "riodejaneiro" | "rio" => ("Rio de Janeiro", "Brazil", "America/Sao_Paulo"),
        "rome" => ("Rome", "Italy", "Europe/Rome"),
        "sanfrancisco" | "sf" => ("San Francisco", "United States", "America/Los_Angeles"),
        "santiago" => ("Santiago", "Chile", "America/Santiago"),
        "saopaulo" | "sãopaulo" => ("São Paulo", "Brazil", "America/Sao_Paulo"),
        "seoul" => ("Seoul", "South Korea", "Asia/Seoul"),
        "shanghai" => ("Shanghai", "China", "Asia/Shanghai"),
        "singapore" => ("Singapore", "Singapore", "Asia/Singapore"),
        "stockholm" => ("Stockholm", "Sweden", "Europe/Stockholm"),
        "sydney" => ("Sydney", "Australia", "Australia/Sydney"),
        "taipei" => ("Taipei", "Taiwan", "Asia/Taipei"),
        "tehran" => ("Tehran", "Iran", "Asia/Tehran"),
        "tokyo" => ("Tokyo", "Japan", "Asia/Tokyo"),
        "toronto" => ("Toronto", "Canada", "America/Toronto"),
        "vancouver" => ("Vancouver", "Canada", "America/Vancouver"),
        "vienna" => ("Vienna", "Austria", "Europe/Vienna"),
        "warsaw" => ("Warsaw", "Poland", "Europe/Warsaw"),
        "washington" | "washingtondc" => ("Washington, D.C.", "United States", "America/New_York"),
        "zurich" => ("Zürich", "Switzerland", "Europe/Zurich"),
        _ => return common_country(&normalized),
    };
    Some(vec![Place {
        name: city.0.to_owned(),
        country: city.1.to_owned(),
        timezone: city.2.to_owned(),
    }])
}

fn common_country(normalized: &str) -> Option<Vec<Place>> {
    let (name, code) = match normalized {
        "argentina" => ("Argentina", "AR"),
        "australia" => ("Australia", "AU"),
        "brazil" => ("Brazil", "BR"),
        "canada" => ("Canada", "CA"),
        "china" => ("China", "CN"),
        "france" => ("France", "FR"),
        "germany" => ("Germany", "DE"),
        "india" => ("India", "IN"),
        "indonesia" => ("Indonesia", "ID"),
        "italy" => ("Italy", "IT"),
        "japan" => ("Japan", "JP"),
        "mexico" => ("Mexico", "MX"),
        "newzealand" => ("New Zealand", "NZ"),
        "russia" => ("Russia", "RU"),
        "southafrica" => ("South Africa", "ZA"),
        "southkorea" | "korea" => ("South Korea", "KR"),
        "spain" => ("Spain", "ES"),
        "unitedkingdom" | "uk" => ("United Kingdom", "GB"),
        "unitedstates" | "unitedstatesofamerica" | "usa" | "us" => ("United States", "US"),
        _ => return None,
    };
    let now = DateTime::<Utc>::from_timestamp(host::unix_time() as i64, 0)?;
    Some(country_places(name, code, now))
}

fn request_places(query: &str) -> Result<Vec<Place>, String> {
    let request = host::HttpRequest {
        method: "GET".into(),
        url: format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=10&language=en&format=json",
            urlencoding::encode(query)
        ),
        headers: Vec::new(),
        body: Vec::new(),
    };
    let response =
        host::request(&request).map_err(|_| "Could not look up that location.".to_owned())?;
    if response.status != 200 {
        return Err(format!(
            "Location service returned HTTP {}.",
            response.status
        ));
    }
    let response = decode_geocoding_response(&response.body)?;
    let now = DateTime::<Utc>::from_timestamp(host::unix_time() as i64, 0)
        .ok_or_else(|| "Host timestamp is invalid.".to_owned())?;
    Ok(resolve_api_places(response.results, query, now))
}

fn decode_geocoding_response(body: &[u8]) -> Result<GeocodingResponse, String> {
    serde_json::from_slice(body).map_err(|_| "Location service returned invalid data.".to_owned())
}

fn resolve_api_places(api_places: Vec<ApiPlace>, query: &str, now: DateTime<Utc>) -> Vec<Place> {
    let normalized_query = normalize_name(query);
    if let Some(country) = api_places.iter().find(|place| {
        is_country(place)
            && (normalize_name(&place.name) == normalized_query
                || normalize_name(&place.country) == normalized_query)
    }) {
        return country_places(&country.name, &country.country_code, now);
    }

    let mut places = Vec::new();
    for place in api_places {
        let Some(timezone) = place
            .timezone
            .filter(|timezone| !timezone.trim().is_empty())
        else {
            continue;
        };
        let resolved = Place {
            name: place.name,
            country: place.country,
            timezone,
        };
        if !places.iter().any(|existing: &Place| {
            existing.name.eq_ignore_ascii_case(&resolved.name)
                && existing.country.eq_ignore_ascii_case(&resolved.country)
                && existing.timezone == resolved.timezone
        }) {
            places.push(resolved);
        }
    }
    places
}

fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn is_country(place: &ApiPlace) -> bool {
    matches!(
        place.feature_code.as_str(),
        "PCLI" | "PCLD" | "PCLF" | "PCLS"
    ) && !place.country_code.is_empty()
}

fn country_places(country_name: &str, country_code: &str, now: DateTime<Utc>) -> Vec<Place> {
    let country_code = country_code.to_ascii_uppercase();
    let mut zones = country_zones(&country_code)
        .into_iter()
        .filter_map(|zone| zone.parse::<Tz>().ok().map(|timezone| (zone, timezone)))
        .collect::<Vec<_>>();
    if country_code == "CN" {
        // China legally observes Beijing Time nationwide. Asia/Urumqi is kept in
        // tzdb for regional/civil compatibility, not as a second national zone.
        zones.retain(|(zone, _)| *zone == "Asia/Shanghai");
    }
    if zones.is_empty() {
        return Vec::new();
    }

    let capital_zone = capital_zone(&country_code);
    let mut groups = BTreeMap::<Vec<i32>, Vec<(&str, Tz)>>::new();
    for (zone, timezone) in zones {
        groups
            .entry(offset_signature(timezone, now))
            .or_default()
            .push((zone, timezone));
    }

    let mut representatives = groups
        .into_values()
        .map(|group| {
            group
                .iter()
                .find(|(zone, _)| Some(*zone) == capital_zone)
                .copied()
                .unwrap_or(group[0])
        })
        .collect::<Vec<_>>();
    representatives.sort_by_key(|(zone, _)| Some(*zone) != capital_zone);

    let multiple = representatives.len() > 1;
    representatives
        .into_iter()
        .enumerate()
        .map(|(index, (zone, _))| {
            let is_capital = index == 0 && Some(zone) == capital_zone;
            Place {
                name: if multiple {
                    if is_capital {
                        capital_label(&country_code)
                            .unwrap_or(country_name)
                            .to_owned()
                    } else {
                        zone_location(zone)
                    }
                } else {
                    country_name.to_owned()
                },
                country: if multiple {
                    country_name.to_owned()
                } else {
                    String::new()
                },
                timezone: zone.to_owned(),
            }
        })
        .collect()
}

fn country_zones(country_code: &str) -> Vec<&'static str> {
    include_str!("../data/zone1970.tab")
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let countries = fields.next()?;
            let _coordinates = fields.next()?;
            let zone = fields.next()?;
            countries
                .split(',')
                .any(|code| code == country_code)
                .then_some(zone)
        })
        .collect()
}

fn offset_signature(timezone: Tz, now: DateTime<Utc>) -> Vec<i32> {
    (0..24)
        .map(|month| {
            let sample = now + Duration::days(i64::from(month) * 31);
            timezone
                .offset_from_utc_datetime(&sample.naive_utc())
                .fix()
                .local_minus_utc()
        })
        .collect()
}

fn capital_zone(country_code: &str) -> Option<&'static str> {
    Some(match country_code {
        "AR" => "America/Argentina/Buenos_Aires",
        "AU" => "Australia/Sydney",
        "BR" => "America/Sao_Paulo",
        "CA" => "America/Toronto",
        "CD" => "Africa/Kinshasa",
        "CL" => "America/Santiago",
        "CN" => "Asia/Shanghai",
        "EC" => "America/Guayaquil",
        "ES" => "Europe/Madrid",
        "FM" => "Pacific/Pohnpei",
        "GL" => "America/Nuuk",
        "ID" => "Asia/Jakarta",
        "KI" => "Pacific/Tarawa",
        "KZ" => "Asia/Almaty",
        "MN" => "Asia/Ulaanbaatar",
        "MX" => "America/Mexico_City",
        "MY" => "Asia/Kuala_Lumpur",
        "NZ" => "Pacific/Auckland",
        "PF" => "Pacific/Tahiti",
        "PG" => "Pacific/Port_Moresby",
        "PT" => "Europe/Lisbon",
        "RU" => "Europe/Moscow",
        "UA" => "Europe/Kyiv",
        "US" => "America/New_York",
        "UZ" => "Asia/Tashkent",
        _ => return None,
    })
}

fn capital_label(country_code: &str) -> Option<&'static str> {
    Some(match country_code {
        "AR" => "Buenos Aires",
        "AU" => "Canberra",
        "BR" => "Brasília",
        "CA" => "Ottawa",
        "CD" => "Kinshasa",
        "CL" => "Santiago",
        "CN" => "Beijing",
        "EC" => "Quito",
        "ES" => "Madrid",
        "FM" => "Palikir",
        "GL" => "Nuuk",
        "ID" => "Jakarta",
        "KI" => "South Tarawa",
        "KZ" => "Astana",
        "MN" => "Ulaanbaatar",
        "MX" => "Mexico City",
        "MY" => "Kuala Lumpur",
        "NZ" => "Wellington",
        "PF" => "Papeete",
        "PG" => "Port Moresby",
        "PT" => "Lisbon",
        "RU" => "Moscow",
        "UA" => "Kyiv",
        "US" => "Washington, D.C.",
        "UZ" => "Tashkent",
        _ => return None,
    })
}

fn zone_location(zone: &str) -> String {
    match zone {
        "America/Noronha" => "Fernando de Noronha".to_owned(),
        "America/Rio_Branco" => "Rio Branco".to_owned(),
        "America/Sao_Paulo" => "São Paulo".to_owned(),
        _ => zone.rsplit('/').next().unwrap_or(zone).replace('_', " "),
    }
}

fn place_cache_key(query: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in query.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("place-v2-{hash:016x}.json")
}

fn result_for_place(place: Place, now: DateTime<Utc>) -> Option<ResultItem> {
    let timezone = place.timezone.parse::<Tz>().ok()?;
    let local = now.with_timezone(&timezone);
    let time = local.format("%H:%M").to_string();
    let location = if place.country.is_empty() || place.country.eq_ignore_ascii_case(&place.name) {
        place.name
    } else {
        format!("{}, {}", place.name, place.country)
    };
    Some(ResultItem {
        id: format!("time:{}", place.timezone),
        title: format!("{time} in {location}"),
        subtitle: format!("{} · {}", local.format("%A, %B %-d"), place.timezone),
        icon: Icon::PackagePath("icon.svg".into()),
        score: None,
        action: Action::CopyText(time),
    })
}

bindings::export!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{
        common_places, country_places, decode_geocoding_response, parse, resolve_api_places,
    };

    #[test]
    fn parses_location() {
        assert_eq!(parse("time in São Paulo"), Some("São Paulo"));
        assert_eq!(parse("TIME IN Tokyo"), Some("Tokyo"));
    }

    #[test]
    fn ignores_other_queries() {
        assert_eq!(parse("timer 5m"), None);
        assert_eq!(parse("time in "), None);
    }

    #[test]
    fn accepts_country_metadata_without_an_embedded_timezone() {
        let response = decode_geocoding_response(
            br#"{"results":[{"name":"Brazil","country":"Brazil","country_code":"BR","feature_code":"PCLI"}]}"#,
        )
        .unwrap();

        assert_eq!(response.results.len(), 1);
        assert!(response.results[0].timezone.is_none());
    }

    #[test]
    fn common_city_index_resolves_without_the_network() {
        let places = common_places("São Paulo").expect("common city should resolve");
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].timezone, "America/Sao_Paulo");
    }

    #[test]
    fn exact_country_result_excludes_unrelated_geocoder_matches() {
        let response = decode_geocoding_response(
            br#"{"results":[
                {"name":"United States","country":"United States","country_code":"US","feature_code":"PCLI"},
                {"name":"Brazil","country":"Brazil","country_code":"BR","feature_code":"PCLI"},
                {"name":"Mount Vernon","country":"United States","country_code":"US","feature_code":"PPL","timezone":"America/Chicago"}
            ]}"#,
        )
        .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 7, 25, 12, 0, 0).unwrap();
        let places = resolve_api_places(response.results, "united states", now);

        assert!(!places.is_empty());
        assert_eq!(places[0].name, "Washington, D.C.");
        assert_eq!(places[0].timezone, "America/New_York");
        assert!(places.iter().all(|place| place.country == "United States"));
    }

    #[test]
    fn brazil_has_four_rule_distinct_zones_with_capital_first() {
        let now = Utc.with_ymd_and_hms(2026, 7, 25, 12, 0, 0).unwrap();
        let places = country_places("Brazil", "BR", now);

        assert_eq!(places.len(), 4);
        assert_eq!(places[0].name, "Brasília");
        assert_eq!(places[0].timezone, "America/Sao_Paulo");
    }

    #[test]
    fn argentina_and_china_have_one_national_time_result() {
        let now = Utc.with_ymd_and_hms(2026, 7, 25, 12, 0, 0).unwrap();

        assert_eq!(country_places("Argentina", "AR", now).len(), 1);
        let china = country_places("China", "CN", now);
        assert_eq!(china.len(), 1);
        assert_eq!(china[0].timezone, "Asia/Shanghai");
    }
}
