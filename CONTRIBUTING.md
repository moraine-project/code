# Contributing

## Before you write code

Open an issue first for anything that changes a signed format, an object kind,
or a wire endpoint. The signed formats in `protocol/spec/` are frozen, and a
change there has to be argued before it is coded, because signatures and
interoperability depend on them.

Small fixes, tests, docs, and website work can go straight to a pull request.

## Building and testing

Everything lives in one repository, and the rule is simple: nothing lands
unformatted or failing.

```sh
unset RUSTUP_TOOLCHAIN
cargo +nightly fmt --all
cargo clippy --all-features --all-targets
cargo test --workspace
cargo run -q -p moraine-verify -- vectors
python3 interop/verify_vectors.py
cargo deny check
```

`cargo deny check` runs the advisory, license, duplicate, and source checks
against the lockfile. Install it once with `cargo install cargo-deny --locked`.
CI runs it, and `cargo audit`, on every push.

The server is tested against SQLite by default. To run the same suite on
Postgres, start one and point the tests at it:

```sh
MORAINE_TEST_POSTGRES=postgres://user:password@127.0.0.1:5432/db cargo test -p moraine-server
```

The website signs in WebAssembly, so its build needs `wasm-pack` and the
`wasm32-unknown-unknown` target:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

```sh
cd web
pnpm install
pnpm fmt && pnpm lint && pnpm check && pnpm test
pnpm build:static
```

End-to-end tests drive a real browser against a real server. They build the
site, start a server on a temporary data directory with the bundled
definitions, and exercise sign-in, publishing, and account recovery:

```sh
pnpm exec playwright install chromium   # once
pnpm test:e2e
```

## The rule for signed formats

If you touch anything that produces or checks signed bytes, three things move
together in the same pull request:

1. the vector corpus in `protocol/vectors/vectors.json`,
2. the Rust implementation,
3. the independent checker in `interop/`.

The corpus is only evidence when two implementations that share no code agree
on every verdict, including the reject reason. A vector that only one
implementation passes proves nothing. Run both commands above before you open
the pull request. Breaking changes are preferred over compatibility shims.
What is signed should be corrected, not carried.

## Style

Prefer names, types, and structure over comments. Comments are for constraints
and temporary problems, and are reserved for the tagged forms (`TODO`,
`FIXME`, `PERF`, `SECURITY`, and the rest). Do not describe what the next line
does. Keep one cohesive concept per file and put behaviour next to the domain
concept it belongs to rather than in a shared grab-bag module.

## Commits and pull requests

Write a one-line commit message in the imperative, the way the existing history
does, and squash review noise rather than stacking fixups. A pull request
should explain what changed, why, and what you ran to check it. Include a test
that fails without your change.

## License

By contributing you agree that your contribution is licensed under the same
terms as the crate you touched: the server and website are AGPL-3.0-or-later,
and the protocol libraries (`codec`, `crypto`, `model`, `metadata`, `install`,
`verify`, `publish`, `resolver`, `launcher`, and the `interop` checker) are
MIT OR Apache-2.0.
