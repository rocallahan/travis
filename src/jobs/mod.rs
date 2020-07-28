//! interfaces for interacting with travis jobs

use super::{Client, Error, Future, Owner, State};
use super::commits::Commit;
use futures::prelude::*;
use hyper::client::connect::Connect;

#[derive(Debug, Deserialize)]
struct JobsWrapper {
    jobs: Vec<Job>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Job {
    pub id: usize,
    // standard rep fields
    pub number: Option<String>,
    pub state: Option<State>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    //pub build:
    pub queue: Option<String>,
    //pub repository
    pub commit: Option<Commit>,
    pub owner: Option<Owner>,
    //pub stage
}

pub struct Jobs<'a, C>
where
    C: Clone + Connect + Send + Sync + 'static,
{
    pub(crate) travis: &'a Client<C>,
    pub(crate) build_id: usize,
}

impl<'a, C> Jobs<'a, C>
where
    C: Clone + Connect + Send + Sync + 'static,
{
    pub fn list(&self) -> Future<Vec<Job>> {
        let host = self.travis.host.clone();
        let build_id = self.build_id;
        Box::pin(
            self.travis
                .get(async move {
                    format!(
                        "{host}/build/{build_id}/jobs",
                        host = host,
                        build_id = build_id,
                    ).parse()
                        .map_err(Error::from)
                })
                .and_then(|wrapper: JobsWrapper| future::ok(wrapper.jobs)),
        )
    }
}
