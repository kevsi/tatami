# Code signing policy

Free code signing provided by SignPath.io, certificate by SignPath Foundation.

## Team roles

| Role | Member | Responsibility |
|---|---|---|
| Author | [@kevsi](https://github.com/kevsi) | Trusted to modify the source code without additional review. |
| Reviewer | [@kevsi](https://github.com/kevsi) | Reviews every change proposed by people who do not have commit access. |
| Approver | [@kevsi](https://github.com/kevsi) | Approves each signing request, one release at a time; no release is signed automatically. |

Tatami is currently maintained by a single developer, so the three roles are held by the same
person. Every release is approved manually before its signing request is submitted.

## Privacy policy

Tatami runs entirely on the user's computer and keeps its data there, under
`%APPDATA%\Tatami` (SQLite database, image cache, downloads). The project collects **no
telemetry**, no analytics and no advertising identifiers, and sends no data to the project's own
servers.

Network traffic happens only in the following cases:

- **Manga sources chosen by the user.** Catalogue pages, series pages, chapter lists and page
  images are requested from the third-party websites the user browses. The built-in scheduler
  may also check the user's favourites for new chapters; it can be disabled or rescheduled in
  the settings.
- **DNS resolution.** The bundled engine (`kotatsu-parsers-redo`) resolves host names through
  DNS-over-HTTPS at Cloudflare (`cloudflare-dns.com`), so the host names being resolved are sent
  to that resolver.
- **Engine updates.** A signed engine archive is only installed when the user provides the
  manifest and archive themselves. No update source is configured by default, so the update
  module stays inactive until the user sets one.

Apart from these cases, this program will not transfer any information to other networked
systems unless specifically requested by the user or the person installing or operating it.
