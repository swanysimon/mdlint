# Release $VERSION

## Installation

Download the appropriate binary for your platform from the assets below.

### From crates.io

```bash
cargo install markdownlint-rs
```

### Binary Downloads

See assets below for pre-built binaries for your platform.

### Verify checksums

Each binary includes a `.sha256` file for verification:

```bash
# Linux/macOS
sha256sum -c mdlint-*.sha256

# Windows (PowerShell)
$expected = (Get-Content mdlint-*.sha256).Split()[0]
$actual = (Get-FileHash mdlint.exe).Hash.ToLower()
if ($expected -eq $actual) { "OK" } else { "FAILED" }
```

### Docker

```bash
docker pull ghcr.io/${REPO}:${VERSION}
```
