use base64::{engine::general_purpose::STANDARD, Engine};
use dioxus::prelude::*;
use dioxus_html::{FileData, HasFileData, InteractionElementOffset};
use std::future::Future;
use track_time_tagger_core::TrackPoint;

const APP_CSS: &str = include_str!("../assets/main.css");

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Tag,
    Preview,
    Learn,
}

#[derive(Clone, PartialEq)]
struct Preview {
    name: String,
    data_url: String,
    status: String,
    coordinates: Option<String>,
    osm_url: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    track_timestamp: Option<i64>,
}

#[derive(Clone)]
struct PreviewUpdate {
    name: String,
    status: String,
    coordinates: Option<String>,
    osm_url: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    track_timestamp: Option<i64>,
}

#[derive(Clone, PartialEq)]
struct AnalysisApproval {
    signature: String,
    downloadable_matches: usize,
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut tab = use_signal(|| Tab::Tag);
    let mut track_file = use_signal(|| None::<FileData>);
    let track_points = use_signal(Vec::<TrackPoint>::new);
    let track_error = use_signal(String::new);
    let mut photos = use_signal(Vec::<FileData>::new);
    let previews = use_signal(Vec::<Preview>::new);
    let timezone = use_signal(|| "America/Toronto".to_string());
    let offset_seconds = use_signal(|| "0".to_string());
    let max_gap_seconds = use_signal(|| "300".to_string());
    let status = use_signal(String::new);
    let busy = use_signal(|| false);
    let analyzed = use_signal(|| None::<AnalysisApproval>);
    let mut selected_photo = use_signal(|| None::<usize>);
    let expanded_cluster = use_signal(|| None::<usize>);
    let preview_summary = previews();
    let matched_count = preview_summary
        .iter()
        .filter(|preview| preview.status.starts_with("MATCH"))
        .count();
    let not_matched_count = preview_summary
        .iter()
        .filter(|preview| preview.status.starts_with("SKIP") || preview.status.starts_with("ERROR"))
        .count();
    let pending_count = preview_summary.len() - matched_count - not_matched_count;

    rsx! {
        document::Title { "Track Time Tagger" }
        document::Link { rel: "icon", href: "favicon.png", r#type: "image/png" }
        document::Style { "{APP_CSS}" }
        document::Meta {
            http_equiv: "Content-Security-Policy",
            content: "default-src 'self'; base-uri 'none'; object-src 'none'; frame-src 'none'; form-action 'none'; connect-src 'none'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self' 'wasm-unsafe-eval'"
        }
        main { class: "page",
            header { class: "hero",
                div { class: "titlebar",
                    img {
                        class: "brand-logo",
                        src: "logo.png",
                        alt: "Track Time Tagger logo"
                    }
                    div { class: "brand-copy",
                        p { class: "eyebrow", "TRACK TIME TAGGER" }
                        h1 { "Add GPS to timestamped photos" }
                    }
                    div { class: "hero-actions",
                        span { class: "app-status", "LOCAL ONLY" }
                        button { class: "learn-more", onclick: move |_| tab.set(Tab::Learn), "Learn more" }
                    }
                }
                p { class: "lede", "Match camera timestamps to a FIT or GPX route, then download GPS-tagged JPEG copies. Nothing is uploaded." }
            }

            if !previews().is_empty() {
                nav { class: "tabs", aria_label: "GUI sections",
                    button { class: if tab() == Tab::Tag { "tab active" } else { "tab" }, onclick: move |_| tab.set(Tab::Tag), "Set up match" }
                    button { class: if tab() == Tab::Preview { "tab active" } else { "tab" }, onclick: move |_| tab.set(Tab::Preview), "Review photos" }
                }
            }

            if tab() == Tab::Tag {
                section { class: "card", aria_label: "Select local files",
                    h2 { "1. Add a track and photos" }
                    p { "Choose files below, or drop one FIT/GPX track and any number of JPEGs into this area." }
                    label {
                        class: "drop-zone",
                        ondragover: move |event| event.prevent_default(),
                        ondrop: move |event| {
                            event.prevent_default();
                            let (track, selected_photos) = split_files(event.files());
                            if track.is_some() || !selected_photos.is_empty() {
                                invalidate_analysis(previews, analyzed);
                            }
                            if let Some(track) = track {
                                track_file.set(Some(track.clone()));
                                load_track_preview(track, track_points, track_error);
                            }
                            if !selected_photos.is_empty() {
                                photos.set(selected_photos.clone());
                                load_previews(selected_photos, previews, busy);
                            }
                        },
                        input {
                            class: "drop-input",
                            r#type: "file",
                            accept: ".fit,.gpx,.jpg,.jpeg,application/gpx+xml,image/jpeg",
                            multiple: true,
                            onchange: move |event| {
                                let (track, selected_photos) = split_files(event.files());
                                if track.is_some() || !selected_photos.is_empty() {
                                    invalidate_analysis(previews, analyzed);
                                }
                                if let Some(track) = track {
                                    track_file.set(Some(track.clone()));
                                    load_track_preview(track, track_points, track_error);
                                }
                                if !selected_photos.is_empty() {
                                    photos.set(selected_photos.clone());
                                    load_previews(selected_photos, previews, busy);
                                }
                            }
                        }
                        span { "Drop FIT, GPX, or JPEG files here — or tap to choose" }
                    }
                    label { class: "field",
                        span { "GPS track (.fit or .gpx)" }
                        input {
                            r#type: "file", accept: ".fit,.gpx,application/gpx+xml",
                            onchange: move |event| {
                                if let Some(track) = event.files().into_iter().next() {
                                    invalidate_analysis(previews, analyzed);
                                    track_file.set(Some(track.clone()));
                                    load_track_preview(track, track_points, track_error);
                                }
                            }
                        }
                    }
                    if let Some(track) = track_file() {
                        p { class: "selection-count", "Track: {track.name()}" }
                    }
                    TrackPreview { points: track_points, error: track_error, matches: previews, selected_photo, expanded_cluster }
                    label { class: "field",
                        span { "Photos (JPEG only for this browser release)" }
                        input {
                            r#type: "file", accept: ".jpg,.jpeg,image/jpeg", multiple: true,
                            onchange: move |event| {
                                let selected_photos = event.files();
                                invalidate_analysis(previews, analyzed);
                                photos.set(selected_photos.clone());
                                load_previews(selected_photos, previews, busy);
                            }
                        }
                    }
                    if photos().is_empty() {
                        p { class: "muted", "Choose one or more JPEG images. Use Shift or Ctrl/Cmd in the file picker to select a batch." }
                    } else {
                        p { class: "selection-count", "{photos().len()} image(s) selected" }
                        ul { class: "selection-list", for file in photos() { li { "{file.name()}" } } }
                        if track_file().is_none() {
                            p { class: "next-step", "Next: add a FIT or GPX track." }
                        } else {
                            p { class: "next-step", "Next: confirm the match settings below, then choose “Analyze matches.”" }
                        }
                    }
                }

                MatchControls { track_file, photos, timezone, offset_seconds, max_gap_seconds, status, busy, previews, tab, analyzed }

                section { class: "privacy", aria_label: "Privacy promise",
                    h2 { "Private by design" }
                    p { "Your files are read only in this browser tab. No account, analytics, or third-party API calls are used in local mode, and originals are never overwritten." }
                }
            } else if tab() == Tab::Preview {
                section { class: "card", aria_label: "Local photo previews",
                    h2 { "Review photo matches" }
                    p { class: "muted", "These previews are created from the JPEG files selected on this device. Run a dry run to annotate each proposed GPS match." }
                    TrackPreview { points: track_points, error: track_error, matches: previews, selected_photo, expanded_cluster }
                    if preview_summary.is_empty() {
                        p { "No photos selected yet." }
                    } else {
                        div { class: "preview-summary", aria_label: "Photo match summary",
                            div { class: "preview-summary-title", "Photo match overview" }
                            div { class: "preview-summary-stats",
                                span { class: "preview-stat matched", strong { "{matched_count}" } " matched" }
                                span { class: "preview-stat not-matched", strong { "{not_matched_count}" } " not matched" }
                                if pending_count > 0 {
                                    span { class: "preview-stat pending", strong { "{pending_count}" } " pending" }
                                }
                            }
                        }
                        div { class: "preview-grid",
                            for (index, preview) in previews().into_iter().enumerate() {
                                figure {
                                    class: if selected_photo() == Some(index) { "preview selected" } else { "preview" },
                                    style: "width: 200px; min-width: 200px; max-width: 200px;",
                                    onclick: move |_| selected_photo.set(Some(index)),
                                    img { src: "{preview.data_url}", alt: "Preview of {preview.name}", style: "display: block; width: 200px; height: 150px; object-fit: cover;" }
                                    figcaption {
                                        strong { "{index + 1}. {preview.name}" }
                                        span { class: "preview-status", "{preview.status}" }
                                        if let (Some(coordinates), Some(osm_url)) = (&preview.coordinates, &preview.osm_url) {
                                            a {
                                                class: "preview-map",
                                                href: "{osm_url}",
                                                target: "_blank",
                                                rel: "noopener noreferrer",
                                                "{coordinates} ↗"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                MatchControls { track_file, photos, timezone, offset_seconds, max_gap_seconds, status, busy, previews, tab, analyzed }
            } else {
                section { class: "learn", aria_label: "How Track Time Tagger works",
                    div { class: "learn-heading",
                        div {
                            p { class: "eyebrow", "HOW IT WORKS" }
                            h2 { "A camera records the moment. A track records the place." }
                        }
                    }
                    p { class: "learn-lede", "Track Time Tagger joins those two records using their timestamps, then writes the matched GPS position into a downloadable JPEG copy." }
                    button { class: "primary learn-cta", onclick: move |_| tab.set(Tab::Tag), "Start tagging" }

                    div { class: "use-case-grid",
                        article { class: "use-case",
                            p { class: "use-case-label", "RACE-EVENT PHOTOS" }
                            h3 { "Give purchased race photos their place on the course" }
                            p { "A photographer's camera can have an accurate clock without GPS. The runner or other subject records their activity with a Garmin watch or GPS device, then provides its FIT file or GPX export." }
                            ol {
                                li { "Collect the photographer's JPEGs and the subject's FIT or GPX track." }
                                li { "Run a dry run and check the proposed locations at the finish, aid stations, and course turns." }
                                li { "Download GPS-tagged copies once the timing looks right." }
                            }
                        }
                        article { class: "use-case",
                            p { class: "use-case-label", "HIKING & BIRDWATCHING" }
                            h3 { "Pair a GPS-less SLR with a phone or watch" }
                            p { "Carry the camera that takes the photo you want, while an Android phone running OpenTracks or a GPS watch records the outing independently." }
                            ol {
                                li { "Keep the camera clock accurate and record the outing as a FIT or GPX track." }
                                li { "Match the track to the SLR's timestamped JPEGs after the trip." }
                                li { "Use the dry run to confirm each location before saving copies." }
                            }
                        }
                    }

                    section { class: "learn-note",
                        h3 { "What you need" }
                        p { "Timestamped JPEG photos, a timestamped FIT or GPX track, and the camera's timezone. A camera offset can correct a clock that was consistently fast or slow." }
                    }
                    section { class: "cli-note",
                        h3 { "Processing a large collection?" }
                        p { "The command-line version is designed for bulk processing: it can scan image directories recursively, preserve their folder layout in a separate output directory, and work with JPEG and TIFF files." }
                    }
                }
            }
            footer { class: "site-footer",
                span { "Track Time Tagger" }
                span { "FIT / GPX → JPEG GPS" }
                a {
                    href: "https://github.com/richyo-codes/track-time-geotag",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "GitHub ↗"
                }
            }
        }
    }
}

#[component]
fn MatchControls(
    track_file: Signal<Option<FileData>>,
    photos: Signal<Vec<FileData>>,
    mut timezone: Signal<String>,
    mut offset_seconds: Signal<String>,
    mut max_gap_seconds: Signal<String>,
    mut status: Signal<String>,
    mut busy: Signal<bool>,
    previews: Signal<Vec<Preview>>,
    mut tab: Signal<Tab>,
    mut analyzed: Signal<Option<AnalysisApproval>>,
) -> Element {
    let ready = track_file().is_some() && !photos().is_empty() && !busy();
    let current_signature = analysis_signature(
        track_file(),
        &photos(),
        &timezone(),
        &offset_seconds(),
        &max_gap_seconds(),
    );
    let approval = analyzed();
    let analysis_current = current_signature.as_ref().is_some_and(|signature| {
        approval
            .as_ref()
            .is_some_and(|approval| approval.signature == *signature)
    });
    let download_ready = ready
        && analysis_current
        && approval
            .as_ref()
            .is_some_and(|approval| approval.downloadable_matches > 0);

    rsx! {
        section { class: "card", aria_label: "Matching settings",
            h2 { "2. Confirm match settings" }
            div { class: "settings",
                label { class: "field", span { "Camera timezone" }
                    input { r#type: "text", value: "{timezone}", oninput: move |event| {
                        timezone.set(event.value());
                        invalidate_analysis(previews, analyzed);
                    } }
                }
                label { class: "field", span { "Camera offset (seconds)" }
                    input { r#type: "number", value: "{offset_seconds}", oninput: move |event| {
                        offset_seconds.set(event.value());
                        invalidate_analysis(previews, analyzed);
                    } }
                }
                label { class: "field", span { "Maximum gap (seconds)" }
                    input { r#type: "number", value: "{max_gap_seconds}", oninput: move |event| {
                        max_gap_seconds.set(event.value());
                        invalidate_analysis(previews, analyzed);
                    } }
                }
            }
        }
        div { class: "action-row",
            button {
                class: "primary", disabled: !ready,
                onclick: move |_| {
                    let Some(track_file) = track_file() else { return };
                    let photos = photos();
                    let timezone = timezone();
                    let offset_seconds = offset_seconds();
                    let max_gap_seconds = max_gap_seconds();
                    let Some(signature) = analysis_signature(
                        Some(track_file.clone()),
                        &photos,
                        &timezone,
                        &offset_seconds,
                        &max_gap_seconds,
                    ) else { return };
                    let photo_count = photos.len();
                    analyzed.set(None);
                    busy.set(true);
                    status.set("Analyzing selected photos without writing…".to_string());
                    spawn_task(async move {
                        let (summary, updates) = dry_run(
                            track_file, photos, timezone, offset_seconds, max_gap_seconds,
                        ).await;
                        let completed = updates.len() == photo_count;
                        let downloadable_matches = updates
                            .iter()
                            .filter(|update| update.status.starts_with("MATCH"))
                            .count();
                        annotate_previews(previews, updates);
                        if completed {
                            analyzed.set(Some(AnalysisApproval {
                                signature,
                                downloadable_matches,
                            }));
                        }
                        status.set(summary);
                        tab.set(Tab::Preview);
                        busy.set(false);
                    });
                },
                "1. Analyze matches"
            }
            button {
                class: "secondary", disabled: !download_ready,
                onclick: move |_| {
                    let Some(track_file) = track_file() else { return };
                    let photos = photos();
                    let timezone = timezone();
                    let offset_seconds = offset_seconds();
                    let max_gap_seconds = max_gap_seconds();
                    busy.set(true);
                    status.set("Reading the selected files…".to_string());
                    spawn_task(async move {
                        let result = tag_and_download(track_file, photos, timezone, offset_seconds, max_gap_seconds).await;
                        status.set(result);
                        busy.set(false);
                    });
                },
                "2. Download tagged copies"
            }
        }
        if !analysis_current {
            p { class: "action-hint", "Analyze the current files and settings before downloading." }
        } else if let Some(approval) = approval {
            if approval.downloadable_matches == 0 {
                p { class: "action-hint warning", "Analysis completed, but no photos are ready to download." }
            } else {
                p { class: "action-hint ready", "Analysis complete: {approval.downloadable_matches} photo(s) ready to download." }
            }
        }
        if !status().is_empty() { p { class: "status", "{status}" } }
    }
}

fn analysis_signature(
    track_file: Option<FileData>,
    photos: &[FileData],
    timezone: &str,
    offset_seconds: &str,
    max_gap_seconds: &str,
) -> Option<String> {
    let track = track_file?;
    if photos.is_empty() {
        return None;
    }
    Some(format!(
        "{}\n{}\n{}\n{}\n{}",
        track.name(),
        photos
            .iter()
            .map(|photo| photo.name())
            .collect::<Vec<_>>()
            .join("\n"),
        timezone,
        offset_seconds,
        max_gap_seconds
    ))
}

fn invalidate_analysis(
    mut previews: Signal<Vec<Preview>>,
    mut analyzed: Signal<Option<AnalysisApproval>>,
) {
    analyzed.set(None);
    let mut current = previews();
    for preview in &mut current {
        preview.status = "Not analyzed".to_string();
        preview.coordinates = None;
        preview.osm_url = None;
        preview.latitude = None;
        preview.longitude = None;
    }
    previews.set(current);
}

#[component]
fn TrackPreview(
    points: Signal<Vec<TrackPoint>>,
    error: Signal<String>,
    matches: Signal<Vec<Preview>>,
    mut selected_photo: Signal<Option<usize>>,
    mut expanded_cluster: Signal<Option<usize>>,
) -> Element {
    let mut zoom = use_signal(|| 1.0_f64);
    let mut center = use_signal(|| (300.0_f64, 130.0_f64));
    let mut drag_position = use_signal(|| None::<(f64, f64)>);
    let points = points();
    let matches = matches();
    let zoom_level = zoom().clamp(1.0, 4.0);
    let (center_x, center_y) = clamp_view_center(center(), zoom_level);
    let view_width = 600.0 / zoom_level;
    let view_height = 260.0 / zoom_level;
    let view_box = format!(
        "{:.2} {:.2} {:.2} {:.2}",
        center_x - view_width / 2.0,
        center_y - view_height / 2.0,
        view_width,
        view_height
    );
    let bounds = TrackBounds::from_points(&points);
    let svg_points = project_track(&points, &bounds, 600.0, 260.0, 24.0);
    let (start_x, start_y) = points
        .first()
        .map(|point| project_point(point, &bounds, 600.0, 260.0, 24.0))
        .unwrap_or((0.0, 0.0));
    let (end_x, end_y) = points
        .last()
        .map(|point| project_point(point, &bounds, 600.0, 260.0, 24.0))
        .unwrap_or((0.0, 0.0));

    if !error().is_empty() {
        return rsx! { p { class: "track-error", "{error()}" } };
    }
    if points.len() < 2 {
        return rsx! {};
    }
    let raw_markers = matches
        .iter()
        .enumerate()
        .filter_map(|(index, preview)| {
            let latitude = preview.latitude?;
            let longitude = preview.longitude?;
            let marker = TrackPoint {
                time: points[0].time,
                lat: latitude,
                lon: longitude,
                altitude: None,
            };
            let (x, y) = project_point(&marker, &bounds, 600.0, 260.0, 24.0);
            Some((index + 1, x, y, preview.name.clone()))
        })
        .collect::<Vec<_>>();
    let clusters = cluster_photo_markers(raw_markers, 46.0 / zoom_level);
    let expanded = expanded_cluster();
    let endpoint_radius = 7.0 / zoom_level;
    let marker_radius = 9.0 / zoom_level;
    let point_radius = 3.0 / zoom_level;
    let cluster_radius = 12.0 / zoom_level;
    let collapse_radius = 10.0 / zoom_level;
    let marker_font_size = 9.0 / zoom_level;
    let cluster_font_size = 11.0 / zoom_level;

    rsx! {
        section { class: "track-preview", aria_label: "GPS track shape preview",
            div { class: "track-preview-heading",
                div {
                    h3 { "Track shape" }
                    p { "A local preview of the selected GPS route." }
                }
                div { class: "track-preview-actions",
                    span { class: "track-point-count", "{points.len()} points" }
                    div { class: "track-zoom", aria_label: "Track preview zoom controls",
                        button {
                            r#type: "button", disabled: zoom_level <= 1.0,
                            aria_label: "Zoom out",
                            onclick: move |_| {
                                zoom.set((zoom() / 1.5).max(1.0));
                                expanded_cluster.set(None);
                            },
                            "−"
                        }
                        span { "{zoom_level:.1}×" }
                        button {
                            r#type: "button", disabled: zoom_level >= 4.0,
                            aria_label: "Zoom in",
                            onclick: move |_| {
                                zoom.set((zoom() * 1.5).min(4.0));
                                expanded_cluster.set(None);
                            },
                            "+"
                        }
                        button {
                            r#type: "button", disabled: zoom_level <= 1.0,
                            class: "zoom-reset",
                            onclick: move |_| {
                                zoom.set(1.0);
                                center.set((300.0, 130.0));
                                expanded_cluster.set(None);
                            },
                            "Reset"
                        }
                    }
                    if zoom_level > 1.0 {
                        div { class: "track-pan", aria_label: "Pan zoomed track preview",
                            button { r#type: "button", aria_label: "Pan left", onclick: move |_| center.set(pan_view(center(), zoom(), -1.0, 0.0)), "←" }
                            button { r#type: "button", aria_label: "Pan up", onclick: move |_| center.set(pan_view(center(), zoom(), 0.0, -1.0)), "↑" }
                            button { r#type: "button", aria_label: "Pan down", onclick: move |_| center.set(pan_view(center(), zoom(), 0.0, 1.0)), "↓" }
                            button { r#type: "button", aria_label: "Pan right", onclick: move |_| center.set(pan_view(center(), zoom(), 1.0, 0.0)), "→" }
                        }
                    }
                }
            }
            svg {
                class: if drag_position().is_some() { "track-svg dragging" } else { "track-svg" },
                view_box: "{view_box}",
                role: "img",
                onwheel: move |event| {
                    event.prevent_default();
                    let delta_y = event.data().delta().strip_units().y;
                    let next_zoom = if delta_y < 0.0 {
                        (zoom() * 1.2).min(4.0)
                    } else {
                        (zoom() / 1.2).max(1.0)
                    };
                    zoom.set(next_zoom);
                    center.set(clamp_view_center(center(), next_zoom));
                    expanded_cluster.set(None);
                },
                onpointerdown: move |event| {
                    event.prevent_default();
                    let position = event.data().element_coordinates();
                    drag_position.set(Some((position.x, position.y)));
                },
                onpointermove: move |event| {
                    let Some((previous_x, previous_y)) = drag_position() else { return };
                    event.prevent_default();
                    let position = event.data().element_coordinates();
                    let current_zoom = zoom();
                    let current_center = center();
                    center.set(clamp_view_center(
                        (
                            current_center.0 - (position.x - previous_x) / current_zoom,
                            current_center.1 - (position.y - previous_y) / current_zoom,
                        ),
                        current_zoom,
                    ));
                    drag_position.set(Some((position.x, position.y)));
                    expanded_cluster.set(None);
                },
                onpointerup: move |_| drag_position.set(None),
                onpointerleave: move |_| drag_position.set(None),
                title { "Shape of the selected GPS track" }
                polyline {
                    points: "{svg_points}",
                    fill: "none",
                    stroke: "#16705a",
                    stroke_width: "5",
                    stroke_linecap: "round",
                    stroke_linejoin: "round"
                }
                circle { cx: "{start_x}", cy: "{start_y}", r: "{endpoint_radius}", fill: "#55c7e8", stroke: "#fff", stroke_width: "3" }
                circle { cx: "{end_x}", cy: "{end_y}", r: "{endpoint_radius}", fill: "#ff9564", stroke: "#fff", stroke_width: "3" }
                for (cluster_index, cluster) in clusters.into_iter().enumerate() {
                    if cluster.markers.len() == 1 {
                        for marker in cluster.markers {
                            g {
                                class: "track-marker interactive",
                                onclick: move |_| selected_photo.set(Some(marker.number - 1)),
                                title { "Photo {marker.number}: {marker.name}" }
                                circle { cx: "{marker.x}", cy: "{marker.y}", r: "{marker_radius}", fill: if selected_photo() == Some(marker.number - 1) { "#ff9564" } else { "#7c3aed" }, stroke: "#fff", stroke_width: "3" }
                                text { x: "{marker.x}", y: "{marker.y}", class: "track-marker-label", style: "font-size: {marker_font_size}px", "{marker.number}" }
                            }
                        }
                    } else if expanded == Some(cluster_index) {
                        for marker in expand_photo_cluster(&cluster, 600.0, 260.0, 24.0, zoom_level) {
                            g {
                                class: "track-marker interactive",
                                onclick: move |_| selected_photo.set(Some(marker.number - 1)),
                                title { "Photo {marker.number}: {marker.name}" }
                                line {
                                    x1: "{marker.x}", y1: "{marker.y}",
                                    x2: "{marker.label_x}", y2: "{marker.label_y}",
                                    class: "photo-leader-line"
                                }
                                circle { cx: "{marker.x}", cy: "{marker.y}", r: "{point_radius}", fill: "#7c3aed" }
                                circle { cx: "{marker.label_x}", cy: "{marker.label_y}", r: "{marker_radius}", fill: if selected_photo() == Some(marker.number - 1) { "#ff9564" } else { "#7c3aed" }, stroke: "#fff", stroke_width: "3" }
                                text { x: "{marker.label_x}", y: "{marker.label_y}", class: "track-marker-label", style: "font-size: {marker_font_size}px", "{marker.number}" }
                            }
                        }
                        g {
                            class: "track-cluster interactive",
                            onclick: move |_| expanded_cluster.set(None),
                            title { "Collapse photo cluster" }
                            circle { cx: "{cluster.x}", cy: "{cluster.y}", r: "{collapse_radius}", fill: "#d97706", stroke: "#fff", stroke_width: "3" }
                            text { x: "{cluster.x}", y: "{cluster.y}", class: "track-marker-label", style: "font-size: {marker_font_size}px", "−" }
                        }
                    } else {
                        g {
                            class: "track-cluster interactive",
                            onclick: move |_| expanded_cluster.set(Some(cluster_index)),
                            title { "{cluster.markers.len()} matched photos — click to expand" }
                            circle { cx: "{cluster.x}", cy: "{cluster.y}", r: "{cluster_radius}", fill: "#7c3aed", stroke: "#fff", stroke_width: "3" }
                            text { x: "{cluster.x}", y: "{cluster.y}", class: "track-cluster-label", style: "font-size: {cluster_font_size}px", "{cluster.markers.len()}" }
                        }
                    }
                }
            }
            div { class: "track-legend",
                span { class: "legend-start", "Start" }
                span { class: "legend-end", "End" }
                span { class: "legend-photo", "Photo match" }
            }
        }
    }
}

fn load_track_preview(
    file: FileData,
    mut points: Signal<Vec<TrackPoint>>,
    mut error: Signal<String>,
) {
    points.set(Vec::new());
    error.set(String::new());
    spawn_task(async move {
        match file.read_bytes().await {
            Ok(bytes) => {
                match track_time_tagger_core::load_track_from_bytes(&file.name(), &bytes) {
                    Ok(track) => points.set(track),
                    Err(load_error) => error.set(format!("Could not preview track: {load_error}")),
                }
            }
            Err(read_error) => error.set(format!("Could not read track: {read_error}")),
        }
    });
}

struct TrackBounds {
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
}

impl TrackBounds {
    fn from_points(points: &[TrackPoint]) -> Self {
        Self {
            min_lat: points
                .iter()
                .map(|point| point.lat)
                .fold(f64::INFINITY, f64::min),
            max_lat: points
                .iter()
                .map(|point| point.lat)
                .fold(f64::NEG_INFINITY, f64::max),
            min_lon: points
                .iter()
                .map(|point| point.lon)
                .fold(f64::INFINITY, f64::min),
            max_lon: points
                .iter()
                .map(|point| point.lon)
                .fold(f64::NEG_INFINITY, f64::max),
        }
    }
}

fn project_track(
    points: &[TrackPoint],
    bounds: &TrackBounds,
    width: f64,
    height: f64,
    padding: f64,
) -> String {
    points
        .iter()
        .map(|point| {
            let (x, y) = project_point(point, bounds, width, height, padding);
            format!("{x:.2},{y:.2}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn project_point(
    point: &TrackPoint,
    bounds: &TrackBounds,
    width: f64,
    height: f64,
    padding: f64,
) -> (f64, f64) {
    let lon_span = (bounds.max_lon - bounds.min_lon).max(0.000001);
    let lat_span = (bounds.max_lat - bounds.min_lat).max(0.000001);
    let x = padding + ((point.lon - bounds.min_lon) / lon_span) * (width - padding * 2.0);
    let y = height - padding - ((point.lat - bounds.min_lat) / lat_span) * (height - padding * 2.0);
    (x, y)
}

fn clamp_view_center((x, y): (f64, f64), zoom: f64) -> (f64, f64) {
    let half_width = 300.0 / zoom;
    let half_height = 130.0 / zoom;
    (
        x.clamp(half_width, 600.0 - half_width),
        y.clamp(half_height, 260.0 - half_height),
    )
}

fn pan_view(center: (f64, f64), zoom: f64, horizontal: f64, vertical: f64) -> (f64, f64) {
    let step_x = 120.0 / zoom;
    let step_y = 52.0 / zoom;
    clamp_view_center(
        (center.0 + horizontal * step_x, center.1 + vertical * step_y),
        zoom,
    )
}

struct PhotoMarker {
    number: usize,
    x: f64,
    y: f64,
    name: String,
}

struct DisplayMarker {
    number: usize,
    x: f64,
    y: f64,
    label_x: f64,
    label_y: f64,
    name: String,
}

struct PhotoCluster {
    x: f64,
    y: f64,
    markers: Vec<PhotoMarker>,
}

fn cluster_photo_markers(
    raw_markers: Vec<(usize, f64, f64, String)>,
    threshold: f64,
) -> Vec<PhotoCluster> {
    let mut remaining = raw_markers
        .into_iter()
        .map(|(number, x, y, name)| PhotoMarker { number, x, y, name })
        .collect::<Vec<_>>();
    let mut clusters = Vec::new();
    while !remaining.is_empty() {
        let mut markers = vec![remaining.remove(0)];
        while let Some(index) = remaining.iter().position(|candidate| {
            markers.iter().any(|member| {
                let dx = member.x - candidate.x;
                let dy = member.y - candidate.y;
                dx * dx + dy * dy <= threshold * threshold
            })
        }) {
            markers.push(remaining.remove(index));
        }
        let x = markers.iter().map(|marker| marker.x).sum::<f64>() / markers.len() as f64;
        let y = markers.iter().map(|marker| marker.y).sum::<f64>() / markers.len() as f64;
        clusters.push(PhotoCluster { x, y, markers });
    }
    clusters
}

fn expand_photo_cluster(
    cluster: &PhotoCluster,
    width: f64,
    height: f64,
    padding: f64,
    zoom: f64,
) -> Vec<DisplayMarker> {
    let count = cluster.markers.len();
    let radius = (count as f64 * 22.0 / std::f64::consts::TAU).max(30.0) / zoom;
    let center_x = cluster.x.clamp(padding + radius, width - padding - radius);
    let center_y = cluster.y.clamp(padding + radius, height - padding - radius);
    cluster
        .markers
        .iter()
        .enumerate()
        .map(|(index, marker)| {
            let angle =
                -std::f64::consts::FRAC_PI_2 + index as f64 * std::f64::consts::TAU / count as f64;
            DisplayMarker {
                number: marker.number,
                x: marker.x,
                y: marker.y,
                label_x: center_x + radius * angle.cos(),
                label_y: center_y + radius * angle.sin(),
                name: marker.name.clone(),
            }
        })
        .collect()
}

fn split_files(files: Vec<FileData>) -> (Option<FileData>, Vec<FileData>) {
    let mut track = None;
    let mut photos = Vec::new();
    for file in files {
        let name = file.name().to_ascii_lowercase();
        if track.is_none() && (name.ends_with(".fit") || name.ends_with(".gpx")) {
            track = Some(file);
        } else if name.ends_with(".jpg") || name.ends_with(".jpeg") {
            photos.push(file);
        }
    }
    (track, photos)
}

fn load_previews(files: Vec<FileData>, mut previews: Signal<Vec<Preview>>, mut busy: Signal<bool>) {
    busy.set(true);
    spawn_task(async move {
        let mut loaded = Vec::new();
        for file in files {
            if let Ok(bytes) = file.read_bytes().await {
                loaded.push(Preview {
                    name: file.name(),
                    data_url: format!("data:image/jpeg;base64,{}", STANDARD.encode(bytes)),
                    status: "Not analyzed".to_string(),
                    coordinates: None,
                    osm_url: None,
                    latitude: None,
                    longitude: None,
                    track_timestamp: None,
                });
            }
        }
        previews.set(loaded);
        busy.set(false);
    });
}

fn annotate_previews(mut previews: Signal<Vec<Preview>>, updates: Vec<PreviewUpdate>) {
    let mut current = previews();
    for preview in &mut current {
        if let Some(update) = updates.iter().find(|update| update.name == preview.name) {
            preview.status = update.status.clone();
            preview.coordinates = update.coordinates.clone();
            preview.osm_url = update.osm_url.clone();
            preview.latitude = update.latitude;
            preview.longitude = update.longitude;
            preview.track_timestamp = update.track_timestamp;
        }
    }
    current.sort_by_key(|preview| {
        if preview.status.starts_with("MATCH") {
            preview.track_timestamp.unwrap_or(i64::MAX)
        } else {
            i64::MAX
        }
    });
    previews.set(current);
}

fn spawn_task(future: impl Future<Output = ()> + 'static) {
    #[cfg(feature = "web")]
    wasm_bindgen_futures::spawn_local(future);

    #[cfg(not(feature = "web"))]
    dioxus::spawn(future);
}

async fn dry_run(
    track_file: FileData,
    photos: Vec<FileData>,
    timezone_name: String,
    offset_seconds: String,
    max_gap_seconds: String,
) -> (String, Vec<PreviewUpdate>) {
    let timezone = match timezone_name.parse() {
        Ok(value) => value,
        Err(_) => {
            return (
                format!("Unknown IANA timezone: {timezone_name}"),
                Vec::new(),
            )
        }
    };
    let offset_seconds = match offset_seconds.parse() {
        Ok(value) => value,
        Err(_) => {
            return (
                "Camera offset must be a whole number of seconds.".to_string(),
                Vec::new(),
            )
        }
    };
    let max_gap_seconds = match max_gap_seconds.parse() {
        Ok(value) if value >= 0 => value,
        _ => {
            return (
                "Maximum gap must be zero or a positive whole number.".to_string(),
                Vec::new(),
            )
        }
    };
    let track_bytes = match track_file.read_bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                format!("Could not read {}: {error}", track_file.name()),
                Vec::new(),
            )
        }
    };
    let track =
        match track_time_tagger_core::load_track_from_bytes(&track_file.name(), &track_bytes) {
            Ok(track) => track,
            Err(error) => return (format!("Could not load track: {error:#}"), Vec::new()),
        };
    let mut updates = Vec::new();
    let mut matches = 0;
    for photo in photos {
        let name = photo.name();
        let bytes = match photo.read_bytes().await {
            Ok(bytes) => bytes,
            Err(error) => {
                updates.push(PreviewUpdate {
                    name,
                    status: format!("ERROR: could not read ({error})"),
                    coordinates: None,
                    osm_url: None,
                    latitude: None,
                    longitude: None,
                    track_timestamp: None,
                });
                continue;
            }
        };
        match track_time_tagger_core::analyze_jpeg(
            &bytes,
            timezone,
            offset_seconds,
            max_gap_seconds,
            &track,
        ) {
            Ok(analysis) => {
                let prefix = if analysis.already_has_gps {
                    "SKIP: existing GPS"
                } else {
                    "MATCH"
                };
                let suffix = if analysis.matched.interpolated {
                    " interpolated"
                } else {
                    " exact"
                };
                let coordinates =
                    format!("{:.5}, {:.5}", analysis.matched.lat, analysis.matched.lon);
                updates.push(PreviewUpdate {
                    name,
                    status: format!("{prefix}{suffix}"),
                    osm_url: Some(openstreetmap_url(
                        analysis.matched.lat,
                        analysis.matched.lon,
                    )),
                    coordinates: Some(coordinates),
                    latitude: Some(analysis.matched.lat),
                    longitude: Some(analysis.matched.lon),
                    track_timestamp: if analysis.already_has_gps {
                        None
                    } else {
                        Some(analysis.image_utc.timestamp_millis())
                    },
                });
                matches += 1;
            }
            Err(error) => updates.push(PreviewUpdate {
                name,
                status: format!("SKIP: {error:#}"),
                coordinates: None,
                osm_url: None,
                latitude: None,
                longitude: None,
                track_timestamp: None,
            }),
        }
    }
    (format!("Dry run complete: {matches} match(es) analyzed. Review the Photo previews tab before downloading."), updates)
}

fn openstreetmap_url(lat: f64, lon: f64) -> String {
    format!("https://www.openstreetmap.org/?mlat={lat:.7}&mlon={lon:.7}#map=18/{lat:.7}/{lon:.7}")
}

async fn tag_and_download(
    track_file: FileData,
    photos: Vec<FileData>,
    timezone_name: String,
    offset_seconds: String,
    max_gap_seconds: String,
) -> String {
    let timezone = match timezone_name.parse() {
        Ok(value) => value,
        Err(_) => return format!("Unknown IANA timezone: {timezone_name}"),
    };
    let offset_seconds = match offset_seconds.parse() {
        Ok(value) => value,
        Err(_) => return "Camera offset must be a whole number of seconds.".to_string(),
    };
    let max_gap_seconds = match max_gap_seconds.parse() {
        Ok(value) if value >= 0 => value,
        _ => return "Maximum gap must be zero or a positive whole number.".to_string(),
    };
    let track_bytes = match track_file.read_bytes().await {
        Ok(bytes) => bytes,
        Err(error) => return format!("Could not read {}: {error}", track_file.name()),
    };
    let track =
        match track_time_tagger_core::load_track_from_bytes(&track_file.name(), &track_bytes) {
            Ok(track) => track,
            Err(error) => return format!("Could not load track: {error:#}"),
        };
    use std::io::{Cursor, Write};
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut skipped = Vec::new();
    let mut tagged_images = Vec::new();
    for photo in photos {
        let name = photo.name();
        let bytes = match photo.read_bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(error) => {
                skipped.push(format!("{name}: {error}"));
                continue;
            }
        };
        match track_time_tagger_core::geotag_jpeg(
            bytes,
            timezone,
            offset_seconds,
            max_gap_seconds,
            false,
            &track,
        ) {
            Ok(tagged) => tagged_images.push((name, tagged)),
            Err(error) => skipped.push(format!("{name}: {error:#}")),
        }
    }
    if tagged_images.is_empty() {
        return format!("No tagged copies were created. {}", skipped.join(" | "));
    }
    tagged_images.sort_by_key(|(_, tagged)| tagged.image_utc);
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let mut manifest_files = Vec::new();
    for (name, tagged) in tagged_images {
        let archive_name = format!("geotagged-{name}");
        if let Err(error) = archive.start_file(&archive_name, options) {
            skipped.push(format!("{name}: building ZIP: {error}"));
            continue;
        }
        if let Err(error) = archive.write_all(&tagged.bytes) {
            skipped.push(format!("{name}: writing ZIP: {error}"));
            continue;
        }
        manifest_files.push(serde_json::json!({
            "source_file": name,
            "output_file": archive_name,
            "image_utc": tagged.image_utc.to_rfc3339(),
            "latitude": tagged.matched.lat,
            "longitude": tagged.matched.lon,
            "altitude_meters": tagged.matched.altitude,
            "interpolated": tagged.matched.interpolated,
            "before_gap_seconds": tagged.matched.before_gap,
            "after_gap_seconds": tagged.matched.after_gap,
        }));
    }
    let downloaded = manifest_files.len();
    if downloaded == 0 {
        return format!(
            "No tagged copies could be added to the ZIP. {}",
            skipped.join(" | ")
        );
    }
    let manifest = serde_json::json!({
        "format_version": 1,
        "application": "Track Time Tagger",
        "track_file": track_file.name(),
        "camera_timezone": timezone_name,
        "camera_offset_seconds": offset_seconds,
        "maximum_gap_seconds": max_gap_seconds,
        "photos": manifest_files,
        "skipped": skipped,
    });
    if let Err(error) = archive.start_file("track-time-geotag-manifest.json", options) {
        return format!("Could not build ZIP manifest: {error}");
    }
    if let Err(error) = archive.write_all(manifest.to_string().as_bytes()) {
        return format!("Could not write ZIP manifest: {error}");
    }
    let archive = match archive.finish() {
        Ok(cursor) => cursor.into_inner(),
        Err(error) => return format!("Could not finish ZIP archive: {error}"),
    };
    if let Err(error) = download(archive, "track-time-geotagged-photos.zip") {
        return format!("Could not download ZIP archive: {error}");
    }
    if skipped.is_empty() {
        format!("Downloaded a ZIP with {downloaded} geotagged JPEG copy/copies and a manifest.")
    } else {
        format!(
            "Downloaded a ZIP with {downloaded} geotagged JPEG copy/copies; skipped {}. {}",
            skipped.len(),
            skipped.join(" | ")
        )
    }
}

#[cfg(feature = "web")]
fn download(bytes: Vec<u8>, filename: &str) -> Result<(), String> {
    use js_sys::{Array, Uint8Array};
    use wasm_bindgen::{JsCast, JsValue};
    let array = Uint8Array::from(bytes.as_slice());
    let parts = Array::new();
    parts.push(&array.buffer());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|error| format!("creating download: {error:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|error| format!("creating download URL: {error:?}"))?;
    let window = web_sys::window().ok_or("browser window is unavailable")?;
    let document = window.document().ok_or("browser document is unavailable")?;
    let anchor = document
        .create_element("a")
        .map_err(|error| format!("creating download link: {error:?}"))?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "creating download link failed".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    web_sys::Url::revoke_object_url(&url)
        .map_err(|error| format!("finishing download: {error:?}"))?;
    let _ = JsValue::NULL;
    Ok(())
}

#[cfg(not(feature = "web"))]
fn download(_: Vec<u8>, _: &str) -> Result<(), String> {
    Err("downloads are currently available in the web build".to_string())
}
