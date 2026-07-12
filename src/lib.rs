#[allow(warnings)]
mod bindings;

use bindings::exports::rayslash::module::provider::Guest;
use bindings::rayslash::module::{
    host,
    types::{Action, Icon, ModuleError, QueryContext, QueryResponse, ResultItem},
};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::Deserialize;

struct Component;

#[derive(Deserialize)]
struct GeocodingResponse {
    #[serde(default)]
    results: Vec<Place>,
}

#[derive(Deserialize)]
struct Place {
    name: String,
    #[serde(default)]
    country: String,
    timezone: String,
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
                    icon: Icon::Text("◷".into()),
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
    let response: GeocodingResponse = serde_json::from_slice(&response.body)
        .map_err(|_| "Location service returned invalid data.".to_owned())?;
    Ok(response.results)
}

fn result_for_place(place: Place, now: DateTime<Utc>) -> Option<ResultItem> {
    let timezone = place.timezone.parse::<Tz>().ok()?;
    let local = now.with_timezone(&timezone);
    let time = local.format("%H:%M").to_string();
    let location = if place.country.is_empty() {
        place.name
    } else {
        format!("{}, {}", place.name, place.country)
    };
    Some(ResultItem {
        id: format!("time:{}", place.timezone),
        title: format!("{time} in {location}"),
        subtitle: format!("{} · {}", local.format("%A, %B %-d"), place.timezone),
        icon: Icon::Text("◷".into()),
        score: None,
        action: Action::CopyText(time),
    })
}

bindings::export!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::parse;

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
}
