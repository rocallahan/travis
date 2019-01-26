# travis [![Build Status](https://travis-ci.org/softprops/travis.svg?branch=master)](https://travis-ci.org/softprops/travis)

> Rust and Travis, sittin' in a tree

## [Documentation](https://softprops.github.io/travis)

## installation

```toml
[dependencies]
travis = "0.1"
```

## usage

The travis crate provides async api bindings for the [Travis v3 API](https://developer.travis-ci.org/). Most usage will require a `Core`
to execute futures and `Credential` to authenticate requests to construct
a `Client`

```rust
extern crate travis;
extern crate tokio;

use std::env;

use tokio::runtime::current_thread::Runtime;

use travis::{Client, Credential, Result};

fn run() -> Result<()> {
    let mut tr = Runtime::new()?;
    let travis = Client::pro(
        // authentication credentials
        env::var("GH_TOKEN").ok().map(
            |token| Credential::Github(token),
        ),
        // rt for credential exchange ( if needed )
        &mut rt,
    )?;
    Ok(())
}
```

See the [examples](examples) directory for some example code.

Doug Tangren (softprops) 2017
