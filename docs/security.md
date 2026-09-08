# What the system does and does not prove

Moraine's job is to make claims checkable. It is worth being precise about
which claims are checked, because a registry that overstates its guarantees is
worse than one that is honest about its limits.

## Bytes and signatures

Every signed object is encoded in a canonical form with one valid byte
representation. A signature covers the encoded bytes and a domain string that
names the object kind, so a signature made for one kind cannot be replayed as
another. A digest is over the same canonical bytes, so two implementations that
agree on the format agree on the digest.

Checking a release means checking that the signature is valid for the key it
names, that the key was allowed to sign at that point in the project's history,
and that the artifact digests match the bytes in hand. If any of those fail,
the release is rejected. There is no partial trust and no "probably fine".

What a valid signature means is narrow: the holder of the key signed these
bytes. It does not mean the software is safe, correct, or free of malware. A
publisher can sign a harmful release. The signature only says who published it,
and that the bytes have not changed since.

## Declared and attested

Compatibility a publisher states is *declared*. It is stored, shown as
declared, and never treated as a measurement. Evidence someone else produces
about a release, such as a review, a scanner result, or a build provenance
record, is *attested*. It is stored and attributed to whoever produced it. The
two are never merged into one field, because they are not the same kind of
claim and a reader deserves to know which one they are looking at.

Attestations are evidence, not verdicts. An SBOM lists components; it does not
say they are safe. A missing attestation is not itself a finding.

## Ownership and keys

A project has a root key. Releases are signed by that key or by a key it
delegates to. Delegation has a validity range, so a leaked delegate key can be
cut off without discarding the project. Ownership can be transferred, and the
transfer is itself a signed record. A key compromise has a recovery path, and
recovery is visible in the project's history rather than silent.

Key changes are the most dangerous moment for anyone following a project,
because they let the meaning of the name change hands. An instance keeps key
changes in the feed where they can be seen, and the client refuses to go
backwards: a lower feed sequence than one it already observed is rejected
unless a recovery case explicitly allows it.

## Federation and mirrors

A home is authoritative for its own projects. A directory or mirror serves
signed bytes it fetched from a home and verified. A mirror that serves a copy
cannot alter it without the signature failing, and a directory that lists a
project does not gain the ability to change it.

When this instance syncs from another home, it verifies signatures as it
stores. A response that fails verification is discarded, not cached. Outbound
fetches are bounded in size and page count, and federation to loopback over
plain HTTP is off unless an operator turns it on for local tests.

Each sync also records the head it observed, so a home that later presents a
different entry at a sequence it already showed leaves a durable conflict
instead of a transient one. That log is local to this instance and is not
exchanged with anyone, so it can only prove equivocation this instance saw
itself. Cross-instance witness exchange is not implemented; if you need a
second party to corroborate a home, run a second instance and compare.

## What none of this covers

- A malicious release with a valid signature. This is the reason the UI shows
  who published something and when, not just a green check.
- A publisher lying about compatibility. Declared is declared.
- An operator lying about a project. The signatures let you check against a
  different home; the guarantees do not depend on trusting this one.
- Confidentiality of whatever you publish. Published means public. Do not put
  secrets in a mod, a manifest, or a changelog.
- Denial of service from a party willing to spend resources. Limits raise the
  cost; they do not remove the risk.

## Reporting a problem

See [`SECURITY.md`](../SECURITY.md) at the repository root. Do not open a public
issue for an exploitable finding.
