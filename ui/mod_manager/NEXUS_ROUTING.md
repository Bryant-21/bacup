# Browser destinations and reviewed Nexus batches

This local continuation implements simultaneous manager-instance destination review and
bounded account/download actions. It changes dedicated manager modules only. It does not
establish registered SSO, live Nexus acceptance or compatibility between game builds.

## Visible flows

- A synthetic or user-provided nxm startup link goes straight to a single live matching
  game/profile inbox. With zero or several matches, the manager opens **Browser download
  destination**. Choose an explicit library/profile/native target, then Send. No choice
  is preselected for an ambiguous request. Refresh rechecks the published context.
- Routing queues the link for an explicit Download action in the destination window.
  Changing that window's library/profile/target/game or target folder invalidates the
  bound request. Receipt checks after an unknown outcome retain the original delivery ID
  and destination. A confirmed prior delivery never falls back into another library.
- Cancel, Escape, Back and window close discard local unsubmitted link state. A delivery
  already accepted by another window may remain queued there; cancelling routing does
  not claim to revoke it. A new link arriving while a dismissed delivery finishes gets
  its own review. An unavailable destination does not trigger a download.
- Select library rows, then **Information > Review selected Nexus account actions**.
  Review up to 20 rows, choose Endorse/Unendorse/Track/Untrack, select eligible identities
  and separately confirm before Apply. The account, profile, target and declared file/
  version are visible. Cancel before Apply sends nothing.
- **Settings > Nexus > Review pending downloads (next 20)** reviews fresh browser requests.
  Apply queues only the selected eligible identities through the existing owned queue,
  using the configured one-to-four transfer slots. **Pause this download batch** retains
  checkpoints. Each finished archive still requires a separate destination/installer
  review; bulk actions never install or deploy.

## IPC and failure contract

Windows named pipes publish one app-owned instance record per random instance ID under
the toolkit settings directory. Discovery examines at most 256 records and 64 live
instances. Records, responses and public receipts are bounded to 16 KiB; queues hold at
most 100 in-memory links. Instance metadata changes publish atomically; unchanged frames
do not rewrite it. IPC work runs off the UI thread with bounded waits.

The sender checks the actual named-pipe server PID, creation identity and primary-token
SID before transmitting a URL. The receiver checks the client SID before reading a body.
The Windows APIs are documented by Microsoft: [server PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid),
[client PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)
and [token information](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation).
This uses existing process-query/token-read rights; no registry, ACL, elevation privilege
or firewall change is part of routing. The boundary trusts app-owned metadata and the
logged-in Windows account; it does not isolate hostile processes running as that account.

An exclusive public receipt claim precedes inbox enqueue. Replays of one delivery ID
across windows queue once. Receipt acknowledgements contain only game/mod/file identity,
instance/process identity and destination context. API keys, nxm keys/expiry/user IDs,
full nxm/CDN URLs and IPC secrets are not persisted. Malformed responses stay uncertain.
Lost acknowledgements use receipt checks; no alternate destination is chosen silently.
If receipt update fails after local intake, the link stays queued here and the receipt
remains claimed, with an explicit warning. Closed unprocessed owners can release their
claim on an explicitly reviewed retry; delivered closed owners do not redirect it.

Account batches send sequentially and deduplicate game/mod identity. Different installed
versions for one endorsement identity require selection of one version. Context/session/
version/file/link changes invalidate review. Cancellation stops between requests and
reports already sent versus not sent. Failed requests remain remotely unconfirmed;
401/403/429 stop the remainder and enforce bounded backoff. Other item failures remain
visible individually. Cached state is invalidated; no optimistic account state is shown.
Download batches reuse entitlement, free-link, queue deduplication, cross-process job
locks, cancellation, durable checkpoint and rate-limit behavior already covered by
[NEXUS_CONTROLS.md](NEXUS_CONTROLS.md) and the download queue tests.

## Tested coverage and limits

Twenty routing service cases use disposable manager processes/real Windows named pipes
or disposable in-process inboxes. They cover multiple/single/no matching games, explicit
selection, stale context, creation-identity rejection, crash/close pruning, shared delivery
ID replay, lost acknowledgement/retry, malformed responses, receipt failure and absence
of secrets on disk. Thirty batch cases cover all four actions, cancellation/partial
failure, auth/rate stop, stale session/context/version/file/key, Free eligibility, duplicate
versions and generated transfer queue success/failure/rate/cancellation. Fifteen native
input cases cover destination selection/receipt/close, account acknowledgement/partial
cancellation, and download acknowledgement/cancel/pause at normal and 150% text sizes.

The full checkpoint passed 1,592 service cases and 308 native input cases. Subsequent
failure/close/layout checks passed 86 affected services and 22 native input cases.
Source OpenGL captures include the integrated host and main Log at 1280x800/150% text
and 1600x900/100% text. Private build receipts and exact logs are in [PROGRESS.md](PROGRESS.md).
These are synthetic links and mocked accounts/CDN; no live account, actual browser/OS
registry handoff or real game was tested. No latency/CDN-throughput claim is inferred.
The copied private executable's library pixels were captured and inspected; packaged
routing/bulk reviews were not separately driven. Older capture failures remain historical
evidence, not the outcome of this latest run.

Registered SSO needs an approved application identity. Live Free/Premium/browser/CDN/account
acceptance remains separate. Older singleton-IPC manager builds are not routing peers.
Public receipt history has no automatic retention policy; unknown outcomes are never
pruned to make a retry look safe. Bulk review covers the next 20 pending links, not bulk
resume of stored history or unattended library-wide installation. Remote batch outcome
history is session-local. Source/reference replacement and automatic compatible payload
choice remain absent. Full MO2/FOMOD parity is not claimed; [PARITY_REVIEW.md](PARITY_REVIEW.md)
and [INSTALLER_COVERAGE.md](INSTALLER_COVERAGE.md) retain exact remaining contracts.
