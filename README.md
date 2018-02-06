# podcore [![Build Status](https://travis-ci.org/brandur/podcore.svg?branch=master)](https://travis-ci.org/brandur/podcore)

```
brew install direnv
cargo install diesel_cli --no-default-features --features postgres
```

```
cp .envrc.sample .envrc
direnv allow

# $DATABASE_URL
createdb podcore
diesel migration run

# $TEST_DATABASE_URL
createdb podcore-test
DATABASE_URL=$TEST_DATABASE_URL diesel migration run
```

```
$ cargo build && target/debug/podcore api
GraphQL server started on 0.0.0.0:8080
```

Sample query (WIP):

```
$ curl -X POST http://localhost:8080/graphql -d 'query xxx { yyy }
```

GraphiQL is available at [localhost:8080](http://localhost:8080).

Schema changes:

```
diesel print-schema > src/schema.rs
```

Rustfmt (run on `nightly` because rustfmt can't seem to detach itself from
`nightly`)::

```
rustup install nightly
cargo install rustfmt-nightly
rustup run nightly cargo fmt
```

Tests:

```
cargo test

# run a single test (matches on name)
cargo test test_minimal_feed

# show stdout (note that `cargo test -- --nocapture` doesn't work because it
# only affects print! and println! macros)
RUST_TEST_NOCAPTURE=1 cargo test
```

<!--
# vim: set tw=79:
-->
