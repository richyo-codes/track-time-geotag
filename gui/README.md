# Track Time Tagger GUI

This is an optional, early Dioxus GUI for Track Time Tagger. The Rust CLI at
the repository root remains the primary supported application.

## Privacy model

The intended browser workflow is local-first: selected FIT/GPX tracks and
photos are processed in WebAssembly in the browser, then geotagged copies are
downloaded. Local mode has no account, analytics, telemetry, advertising, or
third-party API calls. It does not upload selected files to an application
server.

Map links are optional; opening one shares that coordinate with the map
provider. Future Immich or repository integration will be an explicitly
separate networked mode.

Browsers can access only files the user selects, and they cannot overwrite
original local photos in place. The initial browser writer target is JPEG.

For the full design boundary, read [the GUI privacy plan](../docs/gui-privacy-plan.md).

## Run locally

Install the Dioxus CLI and WebAssembly target once:

```bash
cargo install dioxus-cli
rustup target add wasm32-unknown-unknown
```

Serve the web application locally:

```bash
dx serve --web
```

For the desktop renderer, run:

```bash
dx serve --desktop
```

The desktop build uses the system WebView and may require platform packages.
The optional ExifTool backend is intentionally excluded from web/WASM; see
[the advanced backend guide](../docs/exiftool-backend.md).

## GitHub Pages deployment

The repository includes a GitHub Actions workflow that builds the release WASM
GUI and deploys it to GitHub Pages on pushes to `main`. In the repository’s
Pages settings, select **GitHub Actions** as the publishing source before the
first deployment. The resulting project site is served below the repository
name, and the workflow supplies that base path to Dioxus.

The hosted page is still local-first: selected photos and tracks are processed
in the visitor’s browser. GitHub receives ordinary requests for the static app
files, but not the selected files or their derived GPS locations.

## Current status

The web GUI supports JPEG input: select or drag in one FIT/GPX track and one or
more JPEGs, set the camera timezone and matching tolerances, then download one
`geotagged-...` copy for each successful match. A separate tab shows previews
of the selected local photos. Existing GPS tags are skipped, and originals are
never modified.

Use “Dry run: analyze matches” first to annotate each thumbnail with its match,
skip reason, or error. This makes it possible to manually verify the results
before using the download action. A matched coordinate can be clicked to open
that location in OpenStreetMap; doing so shares that coordinate with
OpenStreetMap.

The page also sends a defense-in-depth Content Security Policy that disables
network connections from the application (`connect-src 'none'`) and restricts
images to local data/blob content. A CSP cannot protect against a compromised
host serving a different application, so an offline/PWA or desktop build gives
the strongest deployment boundary.

The matching and browser-compatible JPEG writer live in the shared
`track-time-tagger-core` crate. TIFF output, in-place desktop writes, and the
optional ExifTool backend remain CLI/desktop work.
