extern crate env_logger;
extern crate futures;
extern crate openssl_probe;
extern crate tokio;
extern crate travis;
extern crate hyper;

use std::env;

use futures::prelude::*;
use futures::stream::futures_unordered::FuturesUnordered;
use hyper::client::connect::Connect;
use tokio::runtime::Runtime;
use travis::{Client, Future, Result, State, builds, repos};

fn jobs<C>(state: State, builds: builds::Builds<C>) -> Future<usize>
where
    C: Clone + Connect + Send + Sync,
{
    Box::pin(
        builds
            .iter(&builds::ListOptions::builder()
                .state(state.clone())
                .include(vec!["build.jobs".into()])
                .build()
                .unwrap())
            .try_fold::<_, Future<usize>, _>(0, move |acc, build| {
                Box::pin(future::ok(
                    acc +
                        build
                            .jobs
                            .iter()
                            .filter(|job| Some(state.clone()) == job.state)
                            .count(),
                ))
            }),
    )
}

fn run() -> Result<()> {
    env_logger::init();
    openssl_probe::init_ssl_cert_env_vars();

    let mut rt = Runtime::new()?;
    let travis = Client::oss(
        None,
        // rt for credential exchange ( if needed )
        &mut rt,
    )?;

    // all passed/failed jobs
    let work = travis
        .repos()
        .iter(
            env::var("GH_OWNER").ok().unwrap_or("rocallahan".into()),
            &repos::ListOptions::builder()
                .limit(100)
                .build()?,
        )
        .map_ok(|repo| {
            let builds = travis.builds(&repo.slug);
            let passed = jobs(State::Passed, builds.clone());
            let failed = jobs(State::Failed, builds);
            vec![
                future::try_join(passed, failed).and_then(
                    move |(p, f)| future::ok((repo.slug, p, f))
                ),
            ].into_iter()
                .collect::<FuturesUnordered<_>>()
        })
        .try_flatten()
        .try_fold::<_, Future<(usize, usize)>, _>(
            (0, 0),
            |(all_passed, all_failed), (slug, passed, failed)| {
                println!("{} ({}, {})", slug, passed, failed);
                Box::pin(
                    future::ok((all_passed + passed, all_failed + failed)),
                )
            },
        );

    // Start the event loop, driving the asynchronous code to completion.
    Ok(println!("{:#?}", rt.block_on(work)))
}

fn main() {
    run().unwrap()
}
