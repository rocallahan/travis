//! interfaces for interacting with travis builds

use super::{Branch, Client, Error, Stream, Future, Owner, Pagination, State};
use futures::prelude::*;
use hyper::client::connect::Connect;
use crate::jobs::Job;
use url::form_urlencoded::Serializer;

#[derive(Debug, Deserialize, Clone)]
struct Wrapper {
    builds: Vec<Build>,
    #[serde(rename = "@pagination")]
    pagination: Pagination,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Build {
    pub id: usize,
    pub number: String,
    pub state: State,
    pub duration: Option<usize>,
    pub event_type: String,
    pub previous_state: Option<State>,
    pub pull_request_title: Option<String>,
    pub pull_request_number: Option<usize>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    // repository
    pub branch: Branch,
    // commit
    pub jobs: Vec<Job>,
    // stages
    pub created_by: Owner,
}

/// list options
#[derive(Builder, Debug)]
#[builder(setter(into), default)]
pub struct ListOptions {
    include: Vec<String>,
    limit: i32,
    /// id, started_at, finished_at,
    /// append :desc to any attribute to reverse order.
    sort_by: String,
    created_by: Option<String>,
    event_type: Option<String>,
    previous_state: Option<State>,
    state: Option<State>,
}

impl ListOptions {
    pub fn builder() -> ListOptionsBuilder {
        ListOptionsBuilder::default()
    }

    fn into_query_string(&self) -> String {
        let mut params = vec![
            ("include", self.include.join(",")),
            ("limit", self.limit.to_string()),
            ("sort_by", self.sort_by.clone()),
        ];
        if let &Some(ref created_by) = &self.created_by {
            params.push(("created_by", created_by.clone()));
        }
        if let &Some(ref event_type) = &self.event_type {
            params.push(("event_type", event_type.clone()));
        }
        if let &Some(ref previous_state) = &self.previous_state {
            params.push(("previous_state", previous_state.to_string()));
        }
        if let &Some(ref state) = &self.state {
            params.push(("state", state.to_string()));
        }
        Serializer::new(String::new()).extend_pairs(params).finish()
    }
}

impl Default for ListOptions {
    fn default() -> Self {
        ListOptions {
            include: Default::default(),
            limit: 25,
            sort_by: "started_at".into(),
            created_by: Default::default(),
            event_type: Default::default(),
            previous_state: Default::default(),
            state: Default::default(),
        }
    }
}

#[derive(Clone)]
pub struct Builds<C>
where
    C: Clone + Connect + Send + Sync + 'static,
{
    pub(crate) travis: Client<C>,
    pub(crate) slug: String,
}

impl<C> Builds<C>
where
    C: Clone + Connect + Send + Sync + 'static,
{
    pub fn list(&self, options: &ListOptions) -> Future<Vec<Build>> {
        let host = self.travis.host.clone();
        let slug = self.slug.clone();
        let options = options.into_query_string();
        Box::pin(
            self.travis
                .get(async move {
                    format!(
                        "{host}/repo/{slug}/builds?{query}",
                        host = host,
                        slug = slug,
                        query = options,
                    ).parse()
                        .map_err(Error::from)
                })
                .and_then(|wrapper: Wrapper| future::ok(wrapper.builds)),
        )
    }

    pub fn iter(
        &self,
        options: &ListOptions,
    ) -> Stream<Build> {
        let host = self.travis.host.clone();
        let slug = self.slug.clone();
        let options = options.into_query_string();
        let first = self.travis
            .get::<Wrapper, _>(async move {
                format!(
                    "{host}/repo/{slug}/builds?{query}",
                    host = host,
                    slug = slug,
                    query = options,
                ).parse()
                    .map_err(Error::from)
            })
            .map_ok(|mut wrapper: Wrapper| {
                let mut builds = wrapper.builds;
                builds.reverse();
                wrapper.builds = builds;
                wrapper
            });
        // needed to move "self" into the closure below
        let clone = self.clone();
        Box::pin(
            first
                .map_ok(move |wrapper| {
                    stream::try_unfold::<_, _, Future<Option<(Build, Wrapper)>>, _>(
                        wrapper,
                        move |mut state| match state.builds.pop() {
                            Some(build) => Box::pin(future::ok(Some((build, state)))),
                            _ => {
                                match state.pagination.next.clone() {
                                    Some(path) => {
                                        let host = clone.travis.host.clone();
                                        Box::pin(
                                            clone
                                                .travis
                                                .get::<Wrapper, _>(async move {
                                                    format!(
                                                        "{host}{path}",
                                                        host = host,
                                                        path = path.href
                                                    ).parse()
                                                        .map_err(Error::from)
                                                })
                                                .map_ok(|mut next| {
                                                    let mut builds = next.builds;
                                                    builds.reverse();
                                                    next.builds = builds;
                                                    Some((
                                                        next.builds.pop().unwrap(),
                                                        next,
                                                    ))
                                                }),
                                        ) as
                                            Future<Option<(Build, Wrapper)>>
                                    }
                                    None => Box::pin(future::ok(None)),
                                }
                            }
                        },
                    )
                })
                .into_stream()
                .try_flatten(),
        )
    }
}
