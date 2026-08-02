# GUI and privacy plan

## Local-first browser workflow

The web/WASM application processes selected FIT/GPX tracks and images entirely in the browser. The standard workflow does not upload photos, tracks, EXIF metadata, or derived GPS locations to a remote server.

1. The user explicitly selects a track and images or a directory.
2. The browser reads only those selected files.
3. WebAssembly performs timestamp matching and EXIF writing in browser memory.
4. The user downloads geotagged copies or explicitly chooses a local save destination.
5. Original local files remain unchanged.

The web release supports JPEG output. The current in-memory EXIF writer path is JPEG-only; TIFF support requires additional browser-compatible writing work.

## Privacy promises

- No account is required for local mode.
- No image, track, or GPS data is uploaded by local mode.
- No analytics, telemetry, advertising, or third-party API calls are included in local mode.
- The application only accesses files selected by the user.
- Map links are optional; opening one sends the selected coordinates to that map provider.
- Immich and other repository integrations are separate opt-in networked modes, with the destination and data sent explained before authorization.

A hosted static site still receives ordinary web requests such as IP address, browser headers, and requests for the application files. An offline/PWA installation or desktop build avoids relying on a hosted application server after installation.

## Platform boundaries

- Browser/WASM: integrated Rust writer only; no ExifTool, Perl, subprocesses, or in-place filesystem writes.
- Desktop: integrated Rust writer by default, with optional ExifTool fallback documented separately.
- Mobile: lower-priority target. It should follow the same local-first and explicit-permission model as the browser workflow.
