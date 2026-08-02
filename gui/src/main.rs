use base64::{engine::general_purpose::STANDARD, Engine};
use dioxus::prelude::*;
use dioxus_html::{FileData, HasFileData};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Tag,
    Preview,
}

#[derive(Clone, PartialEq)]
struct Preview {
    name: String,
    data_url: String,
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

    let ready = track_file().is_some() && !photos().is_empty();

    rsx! {
        document::Title { "Track Time Tagger" }
        main { class: "page",
            header { class: "hero",
                p { class: "eyebrow", "LOCAL-ONLY GPS GEOTAGGING" }
                h1 { "Track Time Tagger" }
                p { class: "lede", "Match camera timestamps to a FIT or GPX route, then download GPS-tagged JPEG copies. Nothing is uploaded." }
            }

            nav { class: "tabs", aria_label: "GUI sections",
                button { class: if tab() == Tab::Tag { "tab active" } else { "tab" }, onclick: move |_| tab.set(Tab::Tag), "Tag photos" }
                button { class: if tab() == Tab::Preview { "tab active" } else { "tab" }, onclick: move |_| tab.set(Tab::Preview), "Photo previews" }
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
                                load_previews(selected_photos, previews);
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
                                load_previews(selected_photos, previews);
                            }
                        }
                    }
                    if photos().is_empty() {
                        p { class: "muted", "Choose one or more JPEG images. Use Shift or Ctrl/Cmd in the file picker to select a batch." }
                    } else {
                        p { class: "selection-count", "{photos().len()} image(s) selected" }
                        ul { class: "selection-list", for file in photos() { li { "{file.name()}" } } }
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

                section { class: "card privacy", aria_label: "Privacy promise",
                    h2 { "Private by design" }
                    ul {
                        li { "No account, analytics, or third-party API calls in local mode." }
                        li { "Your files are read only in this browser tab and are never uploaded." }
                        li { "Tagging creates downloads; your original local photos are never overwritten." }
                    }
                }

                button {
                    class: "primary", disabled: !ready,
                    onclick: move |_| {
                        let Some(track_file) = track_file() else { return };
                        let photos = photos();
                        let timezone = timezone();
                        let offset_seconds = offset_seconds();
                        let max_gap_seconds = max_gap_seconds();
                        status.set("Reading the selected files…".to_string());
                        spawn(async move {
                            let result = tag_and_download(track_file, photos, timezone, offset_seconds, max_gap_seconds).await;
                            status.set(result);
                        });
                    },
                    "Match and download copies"
                }
                if !status().is_empty() { p { class: "status", "{status}" } }
            } else {
                section { class: "card", aria_label: "Local photo previews",
                    h2 { "Selected photo previews" }
                    p { class: "muted", "These previews are created from the JPEG files selected on this device. They are not uploaded." }
                    if previews().is_empty() {
                        p { "No photos selected yet." }
                    } else {
                        div { class: "preview-grid",
                            for preview in previews() {
                                figure { class: "preview",
                                    img { src: "{preview.data_url}", alt: "Preview of {preview.name}" }
                                    figcaption { "{preview.name}" }
                                }
                            }
                        }
                    }
                }
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

fn load_previews(files: Vec<FileData>, mut previews: Signal<Vec<Preview>>) {
    spawn(async move {
        let mut loaded = Vec::new();
        for file in files {
            if let Ok(bytes) = file.read_bytes().await {
                loaded.push(Preview {
                    name: file.name(),
                    data_url: format!("data:image/jpeg;base64,{}", STANDARD.encode(bytes)),
                });
            }
        }
        previews.set(loaded);
    });
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
