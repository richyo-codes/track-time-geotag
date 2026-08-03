use base64::{engine::general_purpose::STANDARD, Engine};
use dioxus::prelude::*;
use dioxus_html::{FileData, HasFileData};
use std::future::Future;

const APP_CSS: &str = include_str!("../assets/main.css");

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Tag,
    Preview,
}

#[derive(Clone, PartialEq)]
struct Preview {
    name: String,
    data_url: String,
    status: String,
    coordinates: Option<String>,
    osm_url: Option<String>,
}

#[derive(Clone)]
struct PreviewUpdate {
    name: String,
    status: String,
    coordinates: Option<String>,
    osm_url: Option<String>,
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut tab = use_signal(|| Tab::Tag);
    let mut track_file = use_signal(|| None::<FileData>);
    let mut photos = use_signal(Vec::<FileData>::new);
    let previews = use_signal(Vec::<Preview>::new);
    let mut timezone = use_signal(|| "America/Toronto".to_string());
    let mut offset_seconds = use_signal(|| "0".to_string());
    let mut max_gap_seconds = use_signal(|| "300".to_string());
    let mut status = use_signal(String::new);
    let mut busy = use_signal(|| false);

    let ready = track_file().is_some() && !photos().is_empty() && !busy();

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
                        h1 { "Photo geotagging, on your terms" }
                    }
                    span { class: "app-status", "LOCAL ONLY" }
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
                    div {
                        class: "drop-zone",
                        ondragover: move |event| event.prevent_default(),
                        ondrop: move |event| {
                            event.prevent_default();
                            let (track, selected_photos) = split_files(event.files());
                            if let Some(track) = track { track_file.set(Some(track)); }
                            if !selected_photos.is_empty() {
                                photos.set(selected_photos.clone());
                                load_previews(selected_photos, previews, busy);
                            }
                        },
                        "Drop files here"
                    }
                    label { class: "field",
                        span { "GPS track (.fit or .gpx)" }
                        input {
                            r#type: "file", accept: ".fit,.gpx,application/gpx+xml",
                            onchange: move |event| {
                                track_file.set(event.files().into_iter().next());
                            }
                        }
                    }
                    if let Some(track) = track_file() {
                        p { class: "selection-count", "Track: {track.name()}" }
                    }
                    label { class: "field",
                        span { "Photos (JPEG only for this browser release)" }
                        input {
                            r#type: "file", accept: ".jpg,.jpeg,image/jpeg", multiple: true,
                            onchange: move |event| {
                                let selected_photos = event.files();
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
                        p { class: "muted", "Next, add a GPS track and run a dry run to review the proposed matches." }
                    }
                }

                section { class: "card", aria_label: "Matching settings",
                    h2 { "2. Match settings" }
                    div { class: "settings",
                        label { class: "field", span { "Camera timezone" }
                            input { r#type: "text", value: "{timezone}", oninput: move |event| timezone.set(event.value()) }
                        }
                        label { class: "field", span { "Camera offset (seconds)" }
                            input { r#type: "number", value: "{offset_seconds}", oninput: move |event| offset_seconds.set(event.value()) }
                        }
                        label { class: "field", span { "Maximum gap (seconds)" }
                            input { r#type: "number", value: "{max_gap_seconds}", oninput: move |event| max_gap_seconds.set(event.value()) }
                        }
                    }
                }

                div { class: "action-row",
                button {
                    class: "secondary", disabled: !ready,
                    onclick: move |_| {
                        let Some(track_file) = track_file() else { return };
                        let photos = photos();
                        let timezone = timezone();
                        let offset_seconds = offset_seconds();
                        let max_gap_seconds = max_gap_seconds();
                            busy.set(true);
                            status.set("Analyzing selected photos without writing…".to_string());
                            spawn_task(async move {
                            let (summary, updates) = dry_run(
                                track_file, photos, timezone, offset_seconds, max_gap_seconds,
                            ).await;
                            annotate_previews(previews, updates);
                            status.set(summary);
                            tab.set(Tab::Preview);
                            busy.set(false);
                        });
                    },
                    "Dry run: analyze matches"
                }
                button {
                    class: "primary", disabled: !ready,
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
                    "Match and download copies"
                }
                }
                if !status().is_empty() { p { class: "status", "{status}" } }

                section { class: "privacy", aria_label: "Privacy promise",
                    h2 { "Private by design" }
                    p { "Your files are read only in this browser tab. No account, analytics, or third-party API calls are used in local mode, and originals are never overwritten." }
                }
            } else {
                section { class: "card", aria_label: "Local photo previews",
                    h2 { "Review photo matches" }
                    p { class: "muted", "These previews are created from the JPEG files selected on this device. Run a dry run to annotate each proposed GPS match." }
                    if previews().is_empty() {
                        p { "No photos selected yet." }
                    } else {
                        div { class: "preview-grid",
                            for preview in previews() {
                                figure { class: "preview", style: "width: 200px; min-width: 200px; max-width: 200px;",
                                    img { src: "{preview.data_url}", alt: "Preview of {preview.name}", style: "display: block; width: 200px; height: 150px; object-fit: cover;" }
                                    figcaption {
                                        strong { "{preview.name}" }
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
            }
            footer { class: "site-footer",
                span { "Track Time Tagger" }
                span { "FIT / GPX → JPEG GPS" }
                span { "Local-first processing" }
            }
        }
    }
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
        }
    }
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
                });
                matches += 1;
            }
            Err(error) => updates.push(PreviewUpdate {
                name,
                status: format!("SKIP: {error:#}"),
                coordinates: None,
                osm_url: None,
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
    let mut downloaded = 0;
    let mut skipped = Vec::new();
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
            Ok(tagged) => match download(tagged.bytes, &name) {
                Ok(()) => downloaded += 1,
                Err(error) => skipped.push(format!("{name}: {error}")),
            },
            Err(error) => skipped.push(format!("{name}: {error:#}")),
        }
    }
    if skipped.is_empty() {
        format!("Downloaded {downloaded} geotagged JPEG copy/copies.")
    } else {
        format!(
            "Downloaded {downloaded}; skipped {}. {}",
            skipped.len(),
            skipped.join(" | ")
        )
    }
}

#[cfg(feature = "web")]
fn download(bytes: Vec<u8>, name: &str) -> Result<(), String> {
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
    anchor.set_download(&format!("geotagged-{name}"));
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
