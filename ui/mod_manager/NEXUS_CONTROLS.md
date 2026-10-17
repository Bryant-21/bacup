# Nexus controls and acceptance bounds

The shared Rust backend uses a connected client, owned queue, cancellable controller
workers, target metadata and reviewed installer. Optional Windows Credential Manager storage
supports startup reconnect in the UI and `manager nexus login --saved` in the CLI.
All account/transport tests use in-memory responses; browser actions are intercepted.
No credentials, account writes, account-bound download or protocol registry change was
performed against a real service.

## Local workflows

- Information > Open replacement file on Nexus opens the exact public files-page identity.
  Free users request a fresh Mod Manager Download link there, then forward or paste that
  nxm link. No Free download entitlement is bypassed; Premium queue remains explicit.
- Settings > Nexus > Check files, versions and account (next 20) rotates through distinct
  mapped identities. Automatic version checks default on while preserving an explicit off
  preference; a separate persisted option includes published file graphs/account state.
  Durable per-identity timestamps/results continue the next batch after a one-minute pause,
  survive restart and retain account backoff. Neither read workflow installs or writes.
- Information > Endorse / Unendorse / Track / Untrack opens an account request review.
  It shows the connected account, game/mod, target-effective installed file and version.
  A separate confirmation enables Send. Cancel sends nothing. Unendorse uses Nexus's
  `abstain` action with the installed `Version`, as specified by the official
  [Nexus API client](https://raw.githubusercontent.com/Nexus-Mods/node-nexus-api/master/src/Nexus.ts).
  Eligibility and actual published-version correspondence are decided by Nexus.
- Target customization > Source accepts a target-specific file ID and version label.
  Blank inherits a shared source's ID; a substituted folder remains unknown. Zero is
  explicitly unknown. Positive IDs bind to the current parent game/mod identity; a shared
  reassociation clears the effective file ID. This is declared metadata, not payload proof.
- Downloads > Review for install shows profile/target and incoming game/mod/file identity.
  Choose new mod, canonical owned reinstall, or an additive owned layer above a matching
  referenced provider. No destination is selected automatically. References cannot be
  replaced in place. Installer file review and Install remain separate actions. The archive,
  local metadata, destination and reference context are pinned through staging/commit.

## Defined coverage

The batch has at most 20 distinct mod identities, 10,000 files / 20,000 edges per identity
and 20,000 records per account inventory. Each pass reads endorsements/tracking once,
then mod version and files per identity: exactly **42 mocked requests for 20 mods**.
Separate per-mod reads plus version reads would require 80 requests for the same scope.
This establishes request count, not a live network latency or throughput result.

Partial endpoint failures preserve other successful reads, while unavailable versions,
files, endorsement and tracking remain unknown. Complete empty account lists alone establish
Undecided/Untracked. Invalid paths, duplicate account identities and cyclic update graphs
are rejected. Authentication/permission/rate-limit failures stop remaining requests and
set backoff. Cancellation propagates; attempted batch caches are invalidated before work.
Session/account/profile/target/source/version/file changes discard stale completion.

Remote account requests invalidate cached state; HTTP 202 and interrupted/failed requests
report an unconfirmed outcome. Cancellation after transmission cannot reverse a remote
action. Refresh state before retrying. No optimistic Endorsed/Untracked state is invented.

New tests cover batch deduplication/request bounds, partial failures, 401/403/429 stop,
cancellation, public browser URLs, abstain/202 contracts, stale reviews, target binding,
shared reassociation and downloaded additive completion/cancel/failure/commit cancellation.
Actual ImGui input covers Free file navigation and review/acknowledgement/routing/cancel
at 1280x800, 1600x900 and 150% text scale. Source rendering captures include the main Log.

Full suites passed: 1,548 services / 374.57s and 297 native input cases / 195.62s.
After final cache/lookup and row-label adjustments, 50 affected service cases / 10.14s
and 16 focused native cases / 9.53s passed. Ruff and scoped whitespace checks passed.
The private build includes all three new Nexus modules; copied-fixture GUI ready/close,
source preservation and 18 helper checks passed. Executable receipt and paths are in
[PROGRESS.md](PROGRESS.md). Packaged pixel capture remains unavailable (`screen grab failed`),
so source OpenGL captures do not certify the packaged window's pixels.

## Remaining gaps

Live Free/Premium eligibility, browser/OS handoff and Nexus/CDN/account outcomes have not
been accepted. The public files-page URL is locally verified navigation, not proof of a
browser download. Registered SSO still needs an approved application identity. Separate
simultaneously running libraries now have a destination chooser and bounded reviewed account/
download batches in [NEXUS_ROUTING.md](NEXUS_ROUTING.md). Unattended install-all remains absent.
Automatic compatible payload choice, content-equivalence
verification, reference removals/full replacement and expansion of an existing additive
association to other profiles/targets remain absent. Reads are snapshots rather than an
event stream. Other original parity gaps are listed in [PARITY_REVIEW.md](PARITY_REVIEW.md).
