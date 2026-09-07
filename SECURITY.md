# Security policy

## Reporting a vulnerability

Report privately through GitHub's private vulnerability reporting on this
repository: open the **Security** tab and choose **Report a vulnerability**. Do
not open a public issue for anything exploitable, and do not paste a live
exploit into a discussion.

Include what you found, the smallest reproduction you have, the crate or
endpoint it affects, and the commit you tested. We aim to acknowledge within
three working days and to agree a disclosure date with you, defaulting to 90
days or the release of a fix, whichever comes first.

We do not currently offer a bug bounty.

## What counts as a vulnerability

In scope:

- The canonical encoding and signature verification (`codec`, `crypto`,
  `model`, `verify`). Any input that verifies when it should not, any valid
  object that fails to verify, or any case where two implementations disagree.
- Authentication and authorization in the server: sessions, CSRF, API key
  scope, org and project role checks, upload and submission gating.
- Federation and outbound fetching: SSRF, redirect handling, DNS rebinding,
  response or decompression limits, and signature confusion across homes.
- Anything that lets one account or project affect another, or read data it
  should not reach.
- Denial of service through unbounded work: uploads, feed pagination,
  dependency graphs, mirror probes, or definition files.

Out of scope, because the design says so explicitly:

- A signed release that turns out to be malicious. A signature attributes a
  release to a publisher. It never claims the software is safe.
- A project being delisted, blocked, or quarantined by an instance. That is
  local policy, and it never rewrites signed bytes.
- A publisher declaring false compatibility. That is a signed claim, labelled
  as declared rather than tested.
- An operator running this software without TLS, or a third-party deployment
  with weak headers.
- The vector corpus disagreeing with itself. Open an issue for that.

## Supported versions

This is an alpha. Fixes land on `main`, and there are no maintained backports
yet. The commit you tested matters more than a version number, so include it.
