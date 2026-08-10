# Changelog

## [2.1.0] - 2026-08-10

### Added

- Added `Builder::execute_checked`, which preserves successful Reqwest responses
  and decodes non-success responses with `rp-postgrest-error` 0.4.
- Added `ExecuteError` to distinguish request failures, response-body read
  failures, structured PostgREST errors, and malformed error responses without
  losing authoritative status, response headers, effective URL, or malformed
  body bytes.

## [2.0.0] - 2026-08-10

### Added

- Added `Postgrest::new_with_client`, allowing callers to configure deadlines,
  proxies, TLS, connection pooling, and other HTTP client policy once and share
  it across PostgREST queries.

### Changed

- **Breaking:** Upgraded and re-exported Reqwest 0.13. Reqwest types exposed by
  the public API are not type-compatible with previous Reqwest versions.
