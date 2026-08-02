// Feder: A portable ActivityPub core for many runtimes.
// Copyright (C) 2026 Feder contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use axum::http::{HeaderMap, header::ACCEPT};
use mime::Mime;

const ACTIVITYPUB_MEDIA_TYPES: &[&str] = &[
    "application/activity+json",
    "application/ld+json",
    "application/json",
];
const HTML_MEDIA_TYPES: &[&str] = &["text/html", "application/xhtml+xml"];

struct MediaRange {
    media_type: Mime,
    quality: u16,
    order: usize,
}

#[derive(Clone, Copy)]
struct Preference {
    quality: u16,
    order: usize,
}

pub(crate) fn accepts_activitypub(headers: &HeaderMap) -> bool {
    let ranges = headers
        .get_all(ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|value| value.trim().parse::<Mime>().ok())
        .enumerate()
        .filter_map(|(order, media_range)| {
            let quality = media_range
                .get_param("q")
                .map_or(Some(1000), |value| parse_quality(value.as_str()))?;
            (quality > 0).then_some(MediaRange {
                media_type: media_range,
                quality,
                order,
            })
        })
        .collect::<Vec<_>>();

    let activitypub = preferred(ACTIVITYPUB_MEDIA_TYPES, &ranges);
    let html = preferred(HTML_MEDIA_TYPES, &ranges);

    match (activitypub, html) {
        (Some(activitypub), Some(html)) => prefers(activitypub, html),
        (Some(_), None) => true,
        _ => false,
    }
}

fn preferred(media_types: &[&str], ranges: &[MediaRange]) -> Option<Preference> {
    ranges
        .iter()
        .filter(|range| media_types.contains(&range.media_type.essence_str()))
        .map(|range| Preference {
            quality: range.quality,
            order: range.order,
        })
        .reduce(|current, candidate| {
            if prefers(candidate, current) {
                candidate
            } else {
                current
            }
        })
}

fn prefers(left: Preference, right: Preference) -> bool {
    left.quality > right.quality || (left.quality == right.quality && left.order < right.order)
}

fn parse_quality(value: &str) -> Option<u16> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    match whole {
        "0" => {
            let padding = 3 - fraction.len();
            let fraction = fraction.parse::<u16>().unwrap_or(0);
            Some(fraction * 10_u16.pow(u32::try_from(padding).ok()?))
        }
        "1" if fraction.bytes().all(|byte| byte == b'0') => Some(1000),
        _ => None,
    }
}
