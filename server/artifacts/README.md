Place downloaded test artifacts in this directory, grouped by outer release
format:

- `archives/` for tarballs and ZIP files
- `appimages/` for AppImages
- `binaries/` for standalone executables

Keep sidecars (checksums, zsync, etc) beside the artifact they describe
so basename-based discovery and trust tests can find them together.

Files are available at `http://127.0.0.1:8000/artifacts/<category>/<filename>`.
`SERVER_HOST` and `SERVER_PORT` can override the listener defaults.

Fetch the artifacts required by the end-to-end tests with:

```sh
just fetch-artifacts
```
