# Security Policy / Güvenlik Politikası

## Supported versions
Only the latest release receives fixes. · Yalnızca son sürüm düzeltme alır.

## Reporting a vulnerability / Açık bildirme
Please **do not open a public issue** for security problems. Use GitHub's private reporting:
**Security → Report a vulnerability** on <https://github.com/fatihkunak/Draxbyte/security/advisories/new>.
Güvenlik sorunları için herkese açık issue açma; yukarıdaki özel bildirim bağlantısını kullan.

Helpful details: affected version, steps to reproduce, impact (for example “deletes a file that should be protected”).
You can expect an acknowledgement within a few days; this is a one-person project, so please be patient.

## What counts
- Anything that can make Draxbyte delete or expose files it should never touch (protected paths, symlink/junction traversal, race between the safety check and the delete)
- Leaking an API key (logs, error messages, network requests to the wrong host)
- Sending file contents or more than the documented metadata to an AI provider

## Release integrity
Release exes are signed with a **self-signed** certificate and published with a SHA256 checksum (see the release notes). Verify the checksum, or build from source.
