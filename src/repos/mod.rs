//! Interfaces for interacting with travis repositories

use super::{Branch, Client, Error, Stream, Future, Owner, Pagination};
use futures::prelude::*;
use hyper::client::connect::Connect;
use std::borrow::Cow;

use url::form_urlencoded::Serializer;

#[derive(Debug, Deserialize, Clone)]
struct Wrapper {
    pub repositories: Vec<Repository>,
    #[serde(rename = "@pagination")]
    pagination: Pagination,
}

/// A travis repository
#[derive(Debug, Deserialize, Clone)]
pub struct Repository {
    pub id: usize,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub github_language: Option<String>,
    pub active: bool,
    pub private: bool,
    pub owner: Owner,
    #[serde(rename = "@permissions")]
    pub permissions: RepoPermissions,
    pub default_branch: Option<Branch>,
    pub starred: bool,
}

/// Permissions associated with this repository
/// available to the authenticated user
#[derive(Debug, Deserialize, Clone)]
pub struct RepoPermissions {
    pub read: bool,
    pub admin: bool,
    pub activate: bool,
    pub deactivate: bool,
    pub star: bool,
    pub unstar: bool,
    pub create_cron: bool,
    pub create_env_var: bool,
    pub create_key_pair: bool,
    pub delete_key_pair: bool,
    pub create_request: bool,
}

/// Repository list options
#[derive(Builder, Debug)]
#[builder(setter(into), default)]
pub struct ListOptions {
    include: Vec<String>,
    limit: i32,
    /// id, started_at, finished_at,
    /// append :desc to any attribute to reverse order.
    sort_by: String,
    starred: Option<bool>,
    private: Option<bool>,
    active: Option<bool>,
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
        if let &Some(ref active) = &self.active {
            params.push(("active", active.to_string()));
        }
        if let &Some(ref starred) = &self.starred {
            params.push(("starred", starred.to_string()));
        }
        if let &Some(ref private) = &self.private {
            params.push(("private", private.to_string()));
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
            starred: Default::default(),
            private: Default::default(),
            active: Default::default(),
        }
    }
}

#[derive(Clone)]
pub struct Repos<C>
where
    C: Clone + Connect + Send + Sync + 'static,
{
    pub(crate) travis: Client<C>,
}

impl<C> Repos<C>
where
    C: Clone + Connect + Send + Sync + 'static,
{
    /// get a list of repos for the a given owner (user or org)
    /// todo: add options
    /// https://developer.travis-ci.org/resource/repositories#for_owner
    pub fn list<'b, O>(
        &self,
        owner: O,
        options: &ListOptions,
    ) -> Future<Vec<Repository>>
    where
        O: Into<Cow<'b, str>>,
    {
        let host = self.travis.host.clone();
        let owner = owner.into().to_string();
        let options = options.into_query_string();
        Box::pin(
            self.travis
                .get(async move {
                    format!(
                        "{host}/owner/{owner}/repos??{query}",
                        host = host,
                        owner = owner,
                        query = options,
                    ).parse()
                        .map_err(Error::from)
                })
                .and_then(|wrapper: Wrapper| future::ok(wrapper.repositories)),
        )
    }

    pub fn iter<O>(
        &self,
        owner: O,
        options: &ListOptions,
    ) -> Stream<Repository>
    where
        O: Into<String>,
    {
        let host = self.travis.host.clone();
        let owner = owner.into().clone();
        let options = options.into_query_string();
        let first = self.travis
            .get::<Wrapper, _>(
                async move {
                    format!(
                        "{host}/owner/{owner}/repos?{query}",
                        host = host,
                        owner = owner,
                        query = options,
                    ).parse()
                        .map_err(Error::from)
                }
            )
            .map_ok(|mut wrapper: Wrapper| {
                let mut repositories = wrapper.repositories;
                repositories.reverse();
                wrapper.repositories = repositories;
                wrapper
            });
        // needed to move "self" into the closure below
        let clone = self.clone();
        Box::pin(
            first
                .map_ok(move |wrapper| {
                    stream::try_unfold::<_, _, Future<Option<(Repository, Wrapper)>>, _>(
                        wrapper,
                        move |mut state| match state.repositories.pop() {
                            Some(repository) => Box::pin(
                                future::ok(Some((repository, state))),
                            ),
                            _ => {
                                let host = clone.travis.host.clone();
                                match state.pagination.next.clone() {
                                    Some(path) => 
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
                                                let mut repositories =
                                                    next.repositories;
                                                repositories.reverse();
                                                next.repositories =
                                                    repositories;
                                                Some((
                                                    next.repositories
                                                        .pop()
                                                        .unwrap(),
                                                    next,
                                                ))
                                            }),
                                    ) as
                                        Future<Option<(Repository, Wrapper)>>,
                                    None => Box::pin(future::ok(None))
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
