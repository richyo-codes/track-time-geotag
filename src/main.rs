use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::{
    generate,
    shells::{Bash, Fish, Zsh},
};
use dialoguer::{theme::ColorfulTheme, Confirm};
use exif::{In, Reader, Tag, Value as ExifValue};
use fitparser::{FitDataField, Value};
use gpx::read as read_gpx;
use little_exif::{exif_tag::ExifTag, metadata::Metadata, rational::uR64};
use std::{
    cmp::Ordering,
    ffi::OsStr,
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MapProvider {
    Both,
    Osm,
    Google,
    None,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// FIT or GPX activity containing timestamped positions.
    #[arg(short, long, required_unless_present = "completions")]
    track: Option<PathBuf>,

    /// Generate shell completion script and write it to stdout.
    #[arg(long, value_enum)]
    completions: Option<CompletionShell>,

    /// Image file or directory to scan.
    #[arg(short, long, default_value = ".")]
    images: PathBuf,

    /// Optional output directory. Preserves the input directory layout and leaves inputs unchanged.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// IANA timezone used to interpret timezone-less EXIF timestamps.
    #[arg(long, default_value = "America/Toronto")]
    timezone: Tz,

    /// Add this many seconds to the image time before matching the track.
    #[arg(long, default_value_t = 0)]
    camera_offset_seconds: i64,

    /// Refuse matches farther than this many seconds from the nearest track point.
    #[arg(long, default_value_t = 300)]
    max_gap_seconds: i64,

    /// Include images in nested directories.
    #[arg(long)]
    recursive: bool,

    /// Which verification links to print.
    #[arg(long, value_enum, default_value_t = MapProvider::Both)]
    map: MapProvider,

    /// Print proposals but never write metadata or prompt.
    #[arg(long)]
    dry_run: bool,

    /// Automatically accept every valid match.
    #[arg(long, conflicts_with = "dry_run")]
    yes: bool,

    /// Replace existing GPS metadata. By default, already geotagged images are skipped.
    #[arg(long)]
    overwrite_gps: bool,

    /// Disable the _original backup when writing metadata.
    #[arg(long)]
    no_backup: bool,

    /// Use the external Perl-based ExifTool writer instead of the built-in Rust writer.
    #[arg(long)]
    exiftool: bool,
}

#[derive(Debug, Clone)]
struct TrackPoint {
    time: DateTime<Utc>,
    lat: f64,
    lon: f64,
    altitude: Option<f64>,
}

#[derive(Debug)]
struct ImageInfo {
    path: PathBuf,
    local_time: NaiveDateTime,
    already_has_gps: bool,
}

#[derive(Debug)]
struct Match {
    lat: f64,
    lon: f64,
    altitude: Option<f64>,
    before_gap: i64,
    after_gap: i64,
    interpolated: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(shell) = args.completions {
        generate_completions(shell);
        return Ok(());
    }
    let track_path = args
        .track
        .as_deref()
        .expect("--track is required unless --completions is used");
    if args.exiftool {
        ensure_exiftool_available()?;
    }

    let track = load_track(track_path)?;
    println!(
        "Loaded {} GPS points from {} through {}",
        track.len(),
        track.first().unwrap().time,
        track.last().unwrap().time
    );

    let images = find_images(&args.images, args.recursive)?;
    let input_is_file = args.images.is_file();
    if images.is_empty() {
        bail!(
            "no supported JPEG/TIFF images found under {}",
            args.images.display()
        );
    }

    let mut updated = 0usize;
    let mut skipped = 0usize;
    let theme = ColorfulTheme::default();

    for path in images {
        let info = match read_image_info(&path) {
            Ok(info) => info,
            Err(err) => {
                eprintln!("SKIP {}: {err:#}", path.display());
                skipped += 1;
                continue;
            }
        };

        if info.already_has_gps && !args.overwrite_gps {
            println!("SKIP {}: already contains GPS metadata", path.display());
            skipped += 1;
            continue;
        }

        let image_utc = local_naive_to_utc(info.local_time, args.timezone)?
            + Duration::seconds(args.camera_offset_seconds);

        let matched = match match_track(&track, image_utc, args.max_gap_seconds) {
            Some(m) => m,
            None => {
                println!(
                    "SKIP {}: {} is outside the track or exceeds --max-gap-seconds",
                    path.display(),
                    image_utc
                );
                skipped += 1;
                continue;
            }
        };

        println!("\n{}", path.display());
        println!("  EXIF local:  {} ({})", info.local_time, args.timezone);
        println!("  Match UTC:   {}", image_utc);
        println!("  Position:    {:.7}, {:.7}", matched.lat, matched.lon);
        if let Some(alt) = matched.altitude {
            println!("  Altitude:    {:.1} m", alt);
        }
        println!(
            "  Track gaps:  {}s before, {}s after{}",
            matched.before_gap,
            matched.after_gap,
            if matched.interpolated {
                " (interpolated)"
            } else {
                ""
            }
        );
        print_map_links(args.map, matched.lat, matched.lon);

        if args.dry_run {
            continue;
        }

        let accept = args.yes
            || Confirm::with_theme(&theme)
                .with_prompt("Write this GPS position to the image?")
                .default(false)
                .interact()?;

        if accept {
            let destination = match &args.output {
                Some(output) => {
                    let destination = output_path(&info.path, &args.images, output, input_is_file)?;
                    if let Some(parent) = destination.parent() {
                        fs::create_dir_all(parent).with_context(|| {
                            format!("creating output directory {}", parent.display())
                        })?;
                    }
                    if destination != info.path {
                        fs::copy(&info.path, &destination).with_context(|| {
                            format!(
                                "copying {} to {}",
                                info.path.display(),
                                destination.display()
                            )
                        })?;
                    }
                    destination
                }
                None => info.path.clone(),
            };
            write_gps(&destination, &matched, args.no_backup, args.exiftool)?;
            updated += 1;
            println!("  Updated {}.", destination.display());
        } else {
            skipped += 1;
            println!("  Not changed.");
        }
    }

    println!("\nDone: {updated} updated, {skipped} skipped.");
    Ok(())
}

fn generate_completions(shell: CompletionShell) {
    let mut command = Args::command();
    match shell {
        CompletionShell::Bash => generate(
            Bash,
            &mut command,
            "track-time-tagger",
            &mut std::io::stdout(),
        ),
        CompletionShell::Zsh => generate(
            Zsh,
            &mut command,
            "track-time-tagger",
            &mut std::io::stdout(),
        ),
        CompletionShell::Fish => generate(
            Fish,
            &mut command,
            "track-time-tagger",
            &mut std::io::stdout(),
        ),
    }
}

fn ensure_exiftool_available() -> Result<()> {
    let status = Command::new("exiftool")
        .arg("-ver")
        .status()
        .context("failed to run exiftool; install it first")?;
    if !status.success() {
        bail!("exiftool is installed but did not run successfully");
    }
    Ok(())
}

fn load_track(path: &Path) -> Result<Vec<TrackPoint>> {
    match path.extension().and_then(OsStr::to_str) {
        Some(extension) if extension.eq_ignore_ascii_case("fit") => load_fit_track(path),
        Some(extension) if extension.eq_ignore_ascii_case("gpx") => load_gpx_track(path),
        Some(extension) => bail!("unsupported track format .{extension}; use a .fit or .gpx file"),
        None => bail!("track file must have a .fit or .gpx extension"),
    }
}

fn load_fit_track(path: &Path) -> Result<Vec<TrackPoint>> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let records = fitparser::from_reader(&mut file)
        .with_context(|| format!("parsing FIT file {}", path.display()))?;

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

    points.sort_by_key(|p| p.time);
    points.dedup_by_key(|p| p.time);
    if points.len() < 2 {
        bail!("FIT file contained fewer than two timestamped GPS points");
    }
    Ok(points)
}

fn load_gpx_track(path: &Path) -> Result<Vec<TrackPoint>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let gpx = read_gpx(BufReader::new(file))
        .with_context(|| format!("parsing GPX file {}", path.display()))?;

    let mut points = Vec::new();
    for track in gpx.tracks {
        for segment in track.segments {
            for waypoint in segment.points {
                let Some(time) = waypoint.time else {
                    continue;
                };
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

    points.sort_by_key(|p| p.time);
    points.dedup_by_key(|p| p.time);
    if points.len() < 2 {
        bail!("GPX file contained fewer than two timestamped GPS points");
    }
    Ok(points)
}

fn timestamp_field(fields: &[FitDataField], name: &str) -> Option<DateTime<Utc>> {
    let field = fields.iter().find(|f| f.name() == name)?;
    match field.value() {
        Value::Timestamp(dt) => Some(dt.with_timezone(&Utc)),
        _ => None,
    }
}

fn numeric_field(fields: &[FitDataField], name: &str) -> Option<f64> {
    let field = fields.iter().find(|f| f.name() == name)?;
    match field.value() {
        Value::Float64(v) => Some(*v),
        Value::Float32(v) => Some(*v as f64),
        Value::SInt32(v) => Some(*v as f64),
        Value::UInt32(v) | Value::UInt32z(v) => Some(*v as f64),
        Value::SInt64(v) => Some(*v as f64),
        Value::UInt64(v) | Value::UInt64z(v) => Some(*v as f64),
        Value::SInt16(v) => Some(*v as f64),
        Value::UInt16(v) | Value::UInt16z(v) => Some(*v as f64),
        Value::SInt8(v) => Some(*v as f64),
        Value::UInt8(v) | Value::UInt8z(v) | Value::Byte(v) | Value::Enum(v) => Some(*v as f64),
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

fn find_images(root: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    if root.is_file() {
        return Ok(is_supported_image(root)
            .then(|| root.to_path_buf())
            .into_iter()
            .collect());
    }
    if !root.is_dir() {
        bail!("image path does not exist: {}", root.display());
    }

    let max_depth = if recursive { usize::MAX } else { 1 };
    let mut paths: Vec<_> = WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file() && is_supported_image(entry.path()))
        .map(|entry| entry.into_path())
        .collect();
    paths.sort();
    Ok(paths)
}

fn output_path(
    input: &Path,
    input_root: &Path,
    output_root: &Path,
    input_is_file: bool,
) -> Result<PathBuf> {
    if input_is_file {
        let file_name = input.file_name().context("input image has no file name")?;
        return Ok(output_root.join(file_name));
    }

    let relative = input.strip_prefix(input_root).with_context(|| {
        format!(
            "image {} is not under input directory {}",
            input.display(),
            input_root.display()
        )
    })?;
    Ok(output_root.join(relative))
}

fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "tif" | "tiff"
            )
        })
        .unwrap_or(false)
}

fn read_image_info(path: &Path) -> Result<ImageInfo> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let exif = Reader::new()
        .read_from_container(&mut reader)
        .with_context(|| format!("reading EXIF from {}", path.display()))?;

    let datetime = [Tag::DateTimeOriginal, Tag::DateTimeDigitized, Tag::DateTime]
        .iter()
        .find_map(|tag| exif.get_field(*tag, In::PRIMARY))
        .context("no DateTimeOriginal, DateTimeDigitized, or DateTime EXIF field")?;

    let raw = match &datetime.value {
        ExifValue::Ascii(values) => values.first().context("empty EXIF datetime")?,
        _ => bail!("EXIF datetime is not ASCII"),
    };
    let raw = std::str::from_utf8(raw)?.trim_end_matches('\0').trim();
    let local_time = NaiveDateTime::parse_from_str(raw, "%Y:%m:%d %H:%M:%S")
        .with_context(|| format!("unsupported EXIF datetime: {raw}"))?;

    let already_has_gps = exif.get_field(Tag::GPSLatitude, In::PRIMARY).is_some()
        && exif.get_field(Tag::GPSLongitude, In::PRIMARY).is_some();

    Ok(ImageInfo {
        path: path.to_path_buf(),
        local_time,
        already_has_gps,
    })
}

fn local_naive_to_utc(local: NaiveDateTime, tz: Tz) -> Result<DateTime<Utc>> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(a, b) => bail!(
            "EXIF time {local} is ambiguous in {tz} ({} or {}); use --camera-offset-seconds or a fixed-offset timezone",
            a, b
        ),
        LocalResult::None => bail!("EXIF time {local} does not exist in {tz} due to a DST transition"),
    }
}

fn match_track(track: &[TrackPoint], time: DateTime<Utc>, max_gap: i64) -> Option<Match> {
    let index = track.binary_search_by(|p| p.time.cmp(&time));
    match index {
        Ok(i) => Some(Match {
            lat: track[i].lat,
            lon: track[i].lon,
            altitude: track[i].altitude,
            before_gap: 0,
            after_gap: 0,
            interpolated: false,
        }),
        Err(0) => None,
        Err(i) if i >= track.len() => None,
        Err(i) => {
            let a = &track[i - 1];
            let b = &track[i];
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

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp_longitude(a: f64, b: f64, t: f64) -> f64 {
    let mut delta = b - a;
    if delta > 180.0 {
        delta -= 360.0;
    } else if delta < -180.0 {
        delta += 360.0;
    }
    let result = a + delta * t;
    match result.partial_cmp(&180.0).unwrap_or(Ordering::Equal) {
        Ordering::Greater => result - 360.0,
        _ if result < -180.0 => result + 360.0,
        _ => result,
    }
}

fn print_map_links(provider: MapProvider, lat: f64, lon: f64) {
    if matches!(provider, MapProvider::Both | MapProvider::Osm) {
        println!(
            "  OpenStreetMap: https://www.openstreetmap.org/?mlat={lat:.7}&mlon={lon:.7}#map=18/{lat:.7}/{lon:.7}"
        );
    }
    if matches!(provider, MapProvider::Both | MapProvider::Google) {
        println!(
            "  Google Maps:  https://www.google.com/maps/search/?api=1&query={lat:.7},{lon:.7}"
        );
    }
}

fn write_gps(path: &Path, matched: &Match, no_backup: bool, use_exiftool: bool) -> Result<()> {
    if use_exiftool {
        return write_gps_with_exiftool(path, matched, no_backup);
    }

    write_gps_with_little_exif(path, matched, no_backup)
}

fn write_gps_with_little_exif(path: &Path, matched: &Match, no_backup: bool) -> Result<()> {
    if !no_backup {
        let backup = path.with_file_name(format!(
            "{}_original",
            path.file_name().and_then(OsStr::to_str).unwrap_or("image")
        ));
        std::fs::copy(path, &backup)
            .with_context(|| format!("creating backup {}", backup.display()))?;
    }

    let mut metadata = Metadata::new_from_path(path)
        .map_err(|err| anyhow::anyhow!("reading EXIF from {}: {err}", path.display()))?;
    metadata.set_tag(ExifTag::GPSLatitude(decimal_to_dms(matched.lat.abs())));
    metadata.set_tag(ExifTag::GPSLatitudeRef(if matched.lat < 0.0 {
        "S".to_string()
    } else {
        "N".to_string()
    }));
    metadata.set_tag(ExifTag::GPSLongitude(decimal_to_dms(matched.lon.abs())));
    metadata.set_tag(ExifTag::GPSLongitudeRef(if matched.lon < 0.0 {
        "W".to_string()
    } else {
        "E".to_string()
    }));

    if let Some(altitude) = matched.altitude {
        metadata.set_tag(ExifTag::GPSAltitude(vec![uR64::from(altitude.abs())]));
        metadata.set_tag(ExifTag::GPSAltitudeRef(vec![if altitude < 0.0 {
            1
        } else {
            0
        }]));
    }

    metadata
        .write_to_file(path)
        .map_err(|err| anyhow::anyhow!("writing EXIF to {}: {err}", path.display()))?;
    Ok(())
}

fn decimal_to_dms(value: f64) -> Vec<uR64> {
    let degrees = value.floor();
    let minutes_with_fraction = (value - degrees) * 60.0;
    let minutes = minutes_with_fraction.floor();
    let seconds = (minutes_with_fraction - minutes) * 60.0;
    vec![
        uR64::from(degrees),
        uR64::from(minutes),
        uR64::from(seconds),
    ]
}

fn write_gps_with_exiftool(path: &Path, matched: &Match, no_backup: bool) -> Result<()> {
    let mut cmd = Command::new("exiftool");
    if no_backup {
        cmd.arg("-overwrite_original");
    }
    cmd.arg(format!("-GPSLatitude={:.9}", matched.lat.abs()))
        .arg(format!(
            "-GPSLatitudeRef={}",
            if matched.lat < 0.0 { "S" } else { "N" }
        ))
        .arg(format!("-GPSLongitude={:.9}", matched.lon.abs()))
        .arg(format!(
            "-GPSLongitudeRef={}",
            if matched.lon < 0.0 { "W" } else { "E" }
        ));

    if let Some(alt) = matched.altitude {
        cmd.arg(format!("-GPSAltitude={:.3}", alt.abs()))
            .arg(format!("-GPSAltitudeRef={}", if alt < 0.0 { 1 } else { 0 }));
    }

    let status = cmd
        .arg(path)
        .status()
        .with_context(|| format!("running exiftool for {}", path.display()))?;
    if !status.success() {
        bail!("exiftool failed for {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_fit_semicircles() {
        assert!((normalize_coordinate(1_073_741_824.0, false) - 90.0).abs() < 1e-9);
    }

    #[test]
    fn interpolates_midpoint() {
        let a = TrackPoint {
            time: Utc.timestamp_opt(100, 0).unwrap(),
            lat: 10.0,
            lon: 20.0,
            altitude: Some(100.0),
        };
        let b = TrackPoint {
            time: Utc.timestamp_opt(110, 0).unwrap(),
            lat: 12.0,
            lon: 24.0,
            altitude: Some(120.0),
        };
        let m = match_track(&[a, b], Utc.timestamp_opt(105, 0).unwrap(), 60).unwrap();
        assert_eq!(m.lat, 11.0);
        assert_eq!(m.lon, 22.0);
        assert_eq!(m.altitude, Some(110.0));
    }

    #[test]
    fn matches_exact_track_timestamp_without_interpolation() {
        let point = TrackPoint {
            time: Utc.timestamp_opt(100, 0).unwrap(),
            lat: 10.0,
            lon: 20.0,
            altitude: None,
        };

        let matched = match_track(&[point], Utc.timestamp_opt(100, 0).unwrap(), 0).unwrap();

        assert_eq!(matched.lat, 10.0);
        assert_eq!(matched.lon, 20.0);
        assert_eq!(matched.before_gap, 0);
        assert_eq!(matched.after_gap, 0);
        assert!(!matched.interpolated);
    }

    #[test]
    fn rejects_matches_outside_max_gap() {
        let track = [
            TrackPoint {
                time: Utc.timestamp_opt(100, 0).unwrap(),
                lat: 0.0,
                lon: 0.0,
                altitude: None,
            },
            TrackPoint {
                time: Utc.timestamp_opt(110, 0).unwrap(),
                lat: 1.0,
                lon: 1.0,
                altitude: None,
            },
        ];

        assert!(match_track(&track, Utc.timestamp_opt(105, 0).unwrap(), 4).is_none());
        assert!(match_track(&track, Utc.timestamp_opt(105, 0).unwrap(), 5).is_some());
        assert!(match_track(&track, Utc.timestamp_opt(99, 0).unwrap(), 60).is_none());
    }

    #[test]
    fn interpolates_longitude_across_dateline() {
        let track = [
            TrackPoint {
                time: Utc.timestamp_opt(100, 0).unwrap(),
                lat: 0.0,
                lon: 179.0,
                altitude: None,
            },
            TrackPoint {
                time: Utc.timestamp_opt(110, 0).unwrap(),
                lat: 0.0,
                lon: -179.0,
                altitude: None,
            },
        ];

        let matched = match_track(&track, Utc.timestamp_opt(105, 0).unwrap(), 60).unwrap();

        assert!((matched.lon - 180.0).abs() < 1e-9);
    }

    #[test]
    fn uses_available_altitude_when_only_one_point_has_it() {
        let track = [
            TrackPoint {
                time: Utc.timestamp_opt(100, 0).unwrap(),
                lat: 0.0,
                lon: 0.0,
                altitude: Some(100.0),
            },
            TrackPoint {
                time: Utc.timestamp_opt(110, 0).unwrap(),
                lat: 1.0,
                lon: 1.0,
                altitude: None,
            },
        ];

        let matched = match_track(&track, Utc.timestamp_opt(105, 0).unwrap(), 60).unwrap();

        assert_eq!(matched.altitude, Some(100.0));
    }

    #[test]
    fn converts_timezone_less_timestamp_to_utc() {
        let local =
            NaiveDateTime::parse_from_str("2024:01:15 12:00:00", "%Y:%m:%d %H:%M:%S").unwrap();

        let utc = local_naive_to_utc(local, "America/Toronto".parse().unwrap()).unwrap();

        assert_eq!(utc, Utc.with_ymd_and_hms(2024, 1, 15, 17, 0, 0).unwrap());
    }

    #[test]
    fn rejects_ambiguous_dst_timestamp() {
        let local =
            NaiveDateTime::parse_from_str("2024:11:03 01:30:00", "%Y:%m:%d %H:%M:%S").unwrap();

        let error = local_naive_to_utc(local, "America/Toronto".parse().unwrap()).unwrap_err();

        assert!(error.to_string().contains("ambiguous"));
    }

    #[test]
    fn recognizes_supported_image_extensions_case_insensitively() {
        assert!(is_supported_image(Path::new("photo.JPG")));
        assert!(is_supported_image(Path::new("scan.tiff")));
        assert!(!is_supported_image(Path::new("photo.png")));
        assert!(!is_supported_image(Path::new("photo")));
    }

    #[test]
    fn rejects_unknown_track_formats() {
        let error = load_track(Path::new("activity.csv")).unwrap_err();

        assert!(error.to_string().contains(".fit or .gpx"));
    }

    #[test]
    fn loads_timestamped_gpx_track_points() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.gpx");
        let track = load_track(&path).unwrap();

        assert_eq!(track.len(), 2);
        assert_eq!(
            track[0].time,
            Utc.with_ymd_and_hms(2024, 1, 15, 17, 0, 0).unwrap()
        );
        assert_eq!(track[0].lat, 42.98);
        assert_eq!(track[0].lon, -81.24);
        assert_eq!(track[0].altitude, Some(100.0));
    }

    #[test]
    fn mirrors_nested_images_into_output_directory() {
        let destination = output_path(
            Path::new("photos/hikes/bird.jpg"),
            Path::new("photos"),
            Path::new("geotagged"),
            false,
        )
        .unwrap();

        assert_eq!(destination, Path::new("geotagged/hikes/bird.jpg"));
    }

    #[test]
    fn places_single_file_in_output_directory() {
        let destination = output_path(
            Path::new("photo.jpg"),
            Path::new("photo.jpg"),
            Path::new("geotagged"),
            true,
        )
        .unwrap();

        assert_eq!(destination, Path::new("geotagged/photo.jpg"));
    }
}
