use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use exif::{In, Reader, Tag, Value as ExifValue};
use fitparser::{FitDataField, Value};
use gpx::read as read_gpx;
use little_exif::{exif_tag::ExifTag, filetype::FileExtension, metadata::Metadata, rational::uR64};
use std::{
    cmp::Ordering,
    io::{BufReader, Cursor},
};

#[derive(Debug, Clone)]
pub struct TrackPoint {
    pub time: DateTime<Utc>,
    pub lat: f64,
    pub lon: f64,
    pub altitude: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Match {
    pub lat: f64,
    pub lon: f64,
    pub altitude: Option<f64>,
    pub before_gap: i64,
    pub after_gap: i64,
    pub interpolated: bool,
}

#[derive(Debug, Clone)]
pub struct GeotaggedImage {
    pub exif_local_time: NaiveDateTime,
    pub image_utc: DateTime<Utc>,
    pub matched: Match,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ImageAnalysis {
    pub exif_local_time: NaiveDateTime,
    pub image_utc: DateTime<Utc>,
    pub matched: Match,
    pub already_has_gps: bool,
}

pub fn load_track_from_bytes(name: &str, bytes: &[u8]) -> Result<Vec<TrackPoint>> {
    let extension = name.rsplit('.').next().unwrap_or_default();
    if extension.eq_ignore_ascii_case("fit") {
        load_fit_track(bytes)
    } else if extension.eq_ignore_ascii_case("gpx") {
        load_gpx_track(bytes)
    } else {
        bail!("unsupported track format; choose a .fit or .gpx file")
    }
}

pub fn geotag_jpeg(
    image: Vec<u8>,
    timezone: Tz,
    camera_offset_seconds: i64,
    max_gap_seconds: i64,
    overwrite_gps: bool,
    track: &[TrackPoint],
) -> Result<GeotaggedImage> {
    let analysis = analyze_jpeg(
        &image,
        timezone,
        camera_offset_seconds,
        max_gap_seconds,
        track,
    )?;
    if analysis.already_has_gps && !overwrite_gps {
        bail!("already contains GPS metadata")
    }
    let mut bytes = image;
    write_gps_to_jpeg(&mut bytes, &analysis.matched)?;
    Ok(GeotaggedImage {
        exif_local_time: analysis.exif_local_time,
        image_utc: analysis.image_utc,
        matched: analysis.matched,
        bytes,
    })
}

pub fn analyze_jpeg(
    image: &[u8],
    timezone: Tz,
    camera_offset_seconds: i64,
    max_gap_seconds: i64,
    track: &[TrackPoint],
) -> Result<ImageAnalysis> {
    let (local_time, already_has_gps) = read_jpeg_info(image)?;
    let image_utc =
        local_naive_to_utc(local_time, timezone)? + Duration::seconds(camera_offset_seconds);
    let matched = match_track(track, image_utc, max_gap_seconds)
        .context("outside the track or exceeds the maximum gap")?;
    Ok(ImageAnalysis {
        exif_local_time: local_time,
        image_utc,
        matched,
        already_has_gps,
    })
}

fn load_fit_track(bytes: &[u8]) -> Result<Vec<TrackPoint>> {
    let records = fitparser::from_bytes(bytes).context("parsing FIT track")?;
    let mut points = Vec::new();
    for record in records {
        let timestamp = timestamp_field(record.fields(), "timestamp");
        let lat = numeric_field(record.fields(), "position_lat");
        let lon = numeric_field(record.fields(), "position_long");
        let altitude = numeric_field(record.fields(), "enhanced_altitude")
            .or_else(|| numeric_field(record.fields(), "altitude"));
        if let (Some(time), Some(lat), Some(lon)) = (timestamp, lat, lon) {
            let lat = normalize_coordinate(lat, true);
            let lon = normalize_coordinate(lon, false);
            if lat.is_finite()
                && lon.is_finite()
                && (-90.0..=90.0).contains(&lat)
                && (-180.0..=180.0).contains(&lon)
            {
                points.push(TrackPoint {
                    time,
                    lat,
                    lon,
                    altitude,
                });
            }
        }
    }
    validate_points(points, "FIT")
}

fn load_gpx_track(bytes: &[u8]) -> Result<Vec<TrackPoint>> {
    let gpx = read_gpx(BufReader::new(Cursor::new(bytes))).context("parsing GPX track")?;
    let mut points = Vec::new();
    for track in gpx.tracks {
        for segment in track.segments {
            for waypoint in segment.points {
                let Some(time) = waypoint.time else { continue };
                let time = DateTime::parse_from_rfc3339(&time.format()?)?.with_timezone(&Utc);
                let point = waypoint.point();
                let lat = point.y();
                let lon = point.x();
                if lat.is_finite()
                    && lon.is_finite()
                    && (-90.0..=90.0).contains(&lat)
                    && (-180.0..=180.0).contains(&lon)
                {
                    points.push(TrackPoint {
                        time,
                        lat,
                        lon,
                        altitude: waypoint.elevation,
                    });
                }
            }
        }
    }
    validate_points(points, "GPX")
}

fn validate_points(mut points: Vec<TrackPoint>, kind: &str) -> Result<Vec<TrackPoint>> {
    points.sort_by_key(|point| point.time);
    points.dedup_by_key(|point| point.time);
    if points.len() < 2 {
        bail!("{kind} file contained fewer than two timestamped GPS points")
    }
    Ok(points)
}

fn timestamp_field(fields: &[FitDataField], name: &str) -> Option<DateTime<Utc>> {
    let field = fields.iter().find(|field| field.name() == name)?;
    match field.value() {
        Value::Timestamp(time) => Some(time.with_timezone(&Utc)),
        _ => None,
    }
}

fn numeric_field(fields: &[FitDataField], name: &str) -> Option<f64> {
    let field = fields.iter().find(|field| field.name() == name)?;
    match field.value() {
        Value::Float64(value) => Some(*value),
        Value::Float32(value) => Some(*value as f64),
        Value::SInt32(value) => Some(*value as f64),
        Value::UInt32(value) | Value::UInt32z(value) => Some(*value as f64),
        Value::SInt64(value) => Some(*value as f64),
        Value::UInt64(value) | Value::UInt64z(value) => Some(*value as f64),
        Value::SInt16(value) => Some(*value as f64),
        Value::UInt16(value) | Value::UInt16z(value) => Some(*value as f64),
        Value::SInt8(value) => Some(*value as f64),
        Value::UInt8(value) | Value::UInt8z(value) | Value::Byte(value) | Value::Enum(value) => {
            Some(*value as f64)
        }
        _ => None,
    }
}

fn normalize_coordinate(value: f64, latitude: bool) -> f64 {
    let limit = if latitude { 90.0 } else { 180.0 };
    if value.abs() <= limit {
        value
    } else {
        value * 180.0 / 2_147_483_648.0
    }
}

fn read_jpeg_info(bytes: &[u8]) -> Result<(NaiveDateTime, bool)> {
    let mut reader = BufReader::new(Cursor::new(bytes));
    let exif = Reader::new()
        .read_from_container(&mut reader)
        .context("reading JPEG EXIF")?;
    let datetime = [Tag::DateTimeOriginal, Tag::DateTimeDigitized, Tag::DateTime]
        .iter()
        .find_map(|tag| exif.get_field(*tag, In::PRIMARY))
        .context("no EXIF capture timestamp")?;
    let raw = match &datetime.value {
        ExifValue::Ascii(values) => values.first().context("empty EXIF timestamp")?,
        _ => bail!("EXIF timestamp is not ASCII"),
    };
    let raw = std::str::from_utf8(raw)?.trim_end_matches('\0').trim();
    let local_time = NaiveDateTime::parse_from_str(raw, "%Y:%m:%d %H:%M:%S")
        .with_context(|| format!("unsupported EXIF timestamp: {raw}"))?;
    let has_gps = exif.get_field(Tag::GPSLatitude, In::PRIMARY).is_some()
        && exif.get_field(Tag::GPSLongitude, In::PRIMARY).is_some();
    Ok((local_time, has_gps))
}

fn local_naive_to_utc(local: NaiveDateTime, timezone: Tz) -> Result<DateTime<Utc>> {
    match timezone.from_local_datetime(&local) {
        LocalResult::Single(time) => Ok(time.with_timezone(&Utc)),
        LocalResult::Ambiguous(_, _) => bail!("EXIF timestamp {local} is ambiguous in {timezone}"),
        LocalResult::None => {
            bail!("EXIF timestamp {local} does not exist in {timezone} due to a DST transition")
        }
    }
}

pub fn match_track(track: &[TrackPoint], time: DateTime<Utc>, max_gap: i64) -> Option<Match> {
    match track.binary_search_by(|point| point.time.cmp(&time)) {
        Ok(index) => Some(Match {
            lat: track[index].lat,
            lon: track[index].lon,
            altitude: track[index].altitude,
            before_gap: 0,
            after_gap: 0,
            interpolated: false,
        }),
        Err(0) => None,
        Err(index) if index >= track.len() => None,
        Err(index) => {
            let a = &track[index - 1];
            let b = &track[index];
            let before_gap = (time - a.time).num_seconds();
            let after_gap = (b.time - time).num_seconds();
            if before_gap > max_gap || after_gap > max_gap {
                return None;
            }
            let total_ms = (b.time - a.time).num_milliseconds();
            if total_ms <= 0 {
                return None;
            }
            let fraction = (time - a.time).num_milliseconds() as f64 / total_ms as f64;
            Some(Match {
                lat: lerp(a.lat, b.lat, fraction),
                lon: lerp_longitude(a.lon, b.lon, fraction),
                altitude: match (a.altitude, b.altitude) {
                    (Some(x), Some(y)) => Some(lerp(x, y, fraction)),
                    (Some(x), None) => Some(x),
                    (None, Some(y)) => Some(y),
                    _ => None,
                },
                before_gap,
                after_gap,
                interpolated: true,
            })
        }
    }
}

fn lerp(a: f64, b: f64, fraction: f64) -> f64 {
    a + (b - a) * fraction
}
fn lerp_longitude(a: f64, b: f64, fraction: f64) -> f64 {
    let mut delta = b - a;
    if delta > 180.0 {
        delta -= 360.0
    } else if delta < -180.0 {
        delta += 360.0
    }
    let value = a + delta * fraction;
    match value.partial_cmp(&180.0).unwrap_or(Ordering::Equal) {
        Ordering::Greater => value - 360.0,
        _ if value < -180.0 => value + 360.0,
        _ => value,
    }
}

fn write_gps_to_jpeg(bytes: &mut Vec<u8>, matched: &Match) -> Result<()> {
    let file_type = FileExtension::JPEG;
    let mut metadata = Metadata::new_from_vec(bytes, file_type)
        .map_err(|error| anyhow::anyhow!("reading JPEG metadata: {error}"))?;
    metadata.set_tag(ExifTag::GPSLatitude(decimal_to_dms(matched.lat.abs())));
    metadata.set_tag(ExifTag::GPSLatitudeRef(
        if matched.lat < 0.0 { "S" } else { "N" }.to_string(),
    ));
    metadata.set_tag(ExifTag::GPSLongitude(decimal_to_dms(matched.lon.abs())));
    metadata.set_tag(ExifTag::GPSLongitudeRef(
        if matched.lon < 0.0 { "W" } else { "E" }.to_string(),
    ));
    if let Some(altitude) = matched.altitude {
        metadata.set_tag(ExifTag::GPSAltitude(vec![uR64::from(altitude.abs())]));
        metadata.set_tag(ExifTag::GPSAltitudeRef(vec![if altitude < 0.0 {
            1
        } else {
            0
        }]));
    }
    metadata
        .write_to_vec(bytes, file_type)
        .map_err(|error| anyhow::anyhow!("writing JPEG metadata: {error}"))?;
    Ok(())
}

fn decimal_to_dms(value: f64) -> Vec<uR64> {
    let degrees = value.floor();
    let minutes_fraction = (value - degrees) * 60.0;
    let minutes = minutes_fraction.floor();
    let seconds = (minutes_fraction - minutes) * 60.0;
    vec![
        uR64::from(degrees),
        uR64::from(minutes),
        uR64::from(seconds),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_timestamped_gpx_points_from_bytes() {
        let track = load_track_from_bytes(
            "sample.gpx",
            include_bytes!("../../tests/fixtures/sample.gpx"),
        )
        .unwrap();

        assert_eq!(track.len(), 2);
        assert_eq!(track[0].lat, 42.98);
        assert_eq!(track[1].lon, -81.239);
    }

    #[test]
    fn interpolates_a_track_match() {
        let track = [
            TrackPoint {
                time: Utc.timestamp_opt(100, 0).unwrap(),
                lat: 10.0,
                lon: 20.0,
                altitude: Some(100.0),
            },
            TrackPoint {
                time: Utc.timestamp_opt(110, 0).unwrap(),
                lat: 12.0,
                lon: 24.0,
                altitude: Some(120.0),
            },
        ];

        let matched = match_track(&track, Utc.timestamp_opt(105, 0).unwrap(), 60).unwrap();

        assert_eq!(matched.lat, 11.0);
        assert_eq!(matched.lon, 22.0);
        assert_eq!(matched.altitude, Some(110.0));
        assert!(matched.interpolated);
    }
}
