//! Rust bindings for the [Travis (v3) API](https://developer.travis-ci.org/)
//!
//! # Examples
//!
//! Travis hosts a CI enironments for OSS projects and
//! for private projects (travis pro). The travis client exposes two iterfaces
//! for to accomidate these: `Client::oss` and `Client::pro`
//!
//! Depending on your usecase, you'll typically create one shared instance
//! of a Client within your application. If needed you may clone instances.
//!
//! ```no_run
//! // travis interfaces
//! extern crate travis;
//! // tokio async io
//! extern crate tokio_core;
//!
//! use tokio_core::reactor::Core;
//! use travis::{Client, Credential};
//!
//! fn main() {
//!   let mut core = Core::new().unwrap();
//!   let travis = Client::oss(
//!     Some(Credential::Github(
//!       String::from("gh-access-token")
//!     )),
//!     &mut core
//!   );
//! }
//! ```
//!
//! # Cargo features
//!
//! This crate has one Cargo feature, `tls`,
//! which adds HTTPS support via the `Client::{oss,pro}`
//! constructors. This feature is enabled by default.
#[deny(missing_docs)]
#[macro_use]
extern crate derive_builder;
extern crate futures;
extern crate hyper;
#[macro_use]
extern crate log;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate tokio_core;
extern crate url;
#[macro_use]
extern crate error_chain;
#[cfg(feature = "tls")]
extern crate hyper_tls;
#[cfg(feature = "rustls")]
extern crate hyper_rustls;

#[cfg(feature = "tls")]
use hyper_tls::HttpsConnector;
#[cfg(feature = "rustls")]
use hyper_rustls::HttpsConnector;

use futures::{Future as StdFuture, IntoFuture, Stream as StdStream, stream, future};
use futures::future::FutureResult;
use std::borrow::Cow;

use hyper::{Client as HyperClient, Body, Request, StatusCode, Uri};
use hyper::client::connect::Connect;
use hyper::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};

pub use hyper::Chunk;

use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use std::fmt;
use std::str::FromStr;
use tokio_core::reactor::{Core, Handle};
use url::percent_encoding::{PATH_SEGMENT_ENCODE_SET, utf8_percent_encode};

pub mod env;
use env::Env;
pub mod builds;
use builds::Builds;
pub mod commits;
pub mod jobs;
use jobs::Jobs;
pub mod repos;
use repos::Repos;

pub mod error;
use error::*;
pub use error::{Error, Result};

const OSS_HOST: &str = "https://api.travis-ci.org";
const PRO_HOST: &str = "https://api.travis-ci.com";

/// Enumeration of Travis Build/Job states
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Workload was received and machine is booting
    Received,
    /// Workload was created but not yet started
    Created,
    /// Workload was started but has not completed
    Started,
    /// Workload started but was canceled
    Canceled,
    /// Workload completed with a successful exit status
    Passed,
    /// Workload completed with a failure exit status
    Failed,
    /// Travis build errored
    Errored,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match *self {
                State::Received => "received",
                State::Created => "created",
                State::Started => "started",
                State::Canceled => "canceled",
                State::Passed => "passed",
                State::Failed => "failed",
                State::Errored => "errored",
            }
        )
    }
}

#[derive(Debug, Deserialize, Clone)]
struct Pagination {
    count: usize,
    first: Page,
    next: Option<Page>,
}

#[derive(Debug, Deserialize, Clone)]
struct Page {
    #[serde(rename = "@href")]
    href: String,
}

/// Representation of types of API credentials used to authenticate the client
#[derive(Clone, Debug)]
pub enum Credential {
    /// A Travis API token
    ///
    /// Typically obtained from `travis token` ruby cli
    Token(String),
    /// A Github API token
    ///
    /// This will be immediately exchanged for a travis token
    /// after constructing a `travis::Client` instance.
    /// Care should be taken to associate appropriate
    /// [Github scopes](https://docs.travis-ci.com/user/github-oauth-scopes/)
    /// with these tokens to perform target operations for on oss vs private
    /// repositories
    Github(String),
}

#[derive(Debug, Serialize)]
struct GithubToken {
    github_token: String,
}

#[derive(Debug, Deserialize)]
struct AccessToken {
    pub access_token: String,
}

/// A git branch ref
#[derive(Debug, Deserialize, Clone)]
pub struct Branch {
    pub name: String,
}

/// A Github owner
#[derive(Debug, Deserialize, Clone)]
pub struct Owner {
    pub id: usize,
    pub login: String,
}

/// A type alias for `Futures` that may return `travis::Errors`
pub type Future<T> = Box<StdFuture<Item = T, Error = Error>>;

/// A type alias for `Streams` that may result in `travis::Errors`
pub type Stream<T> = Box<stream::Stream<Item = T, Error = Error>>;

pub(crate) fn escape(raw: &str) -> String {
    utf8_percent_encode(raw, PATH_SEGMENT_ENCODE_SET).to_string()
}

/// Entry point for all travis operations
///
/// Instances of Clients may be cloned.
#[derive(Clone, Debug)]
pub struct Client<C>
where
    C: Clone + Connect + 'static,
{
    http: HyperClient<C>,
    credential: Option<Credential>,
    host: String,
}

#[cfg(feature = "tls")]
type Connector = HttpsConnector<hyper::client::HttpConnector>;
#[cfg(feature = "tls")]
fn create_connector(_: &Handle) -> Connector {
  HttpsConnector::new(4).unwrap()
}
#[cfg(feature = "rustls")]
type Connector = HttpsConnector<hyper::client::HttpConnector>;
#[cfg(feature = "rustls")]
fn create_connector(_: &Handle) -> Connector {
  HttpsConnector::new(4)
}

#[cfg(any(feature = "tls", feature = "rustls"))]
impl Client<Connector> {
    /// Creates an Travis client for open source github repository builds
    pub fn oss(
        credential: Option<Credential>,
        core: &mut Core,
    ) -> Result<Self> {
        let fut = Self::oss_async(credential, &core.handle());
        core.run(fut)
    }
    /// Creates a Travis client for private github repository builds
    pub fn pro(
        credential: Option<Credential>,
        core: &mut Core,
    ) -> Result<Self> {
        let fut = Self::pro_async(credential, &core.handle());
        core.run(fut)
    }

    /// Creates an Travis client for open source github repository builds
    pub fn oss_async(
        credential: Option<Credential>,
        handle: &Handle,
    ) -> Future<Self> {
        let connector = create_connector(handle);
        let http = HyperClient::builder()
            .keep_alive(true)
            .build(connector);
        Client::custom(OSS_HOST, http, credential)
    }

    /// Creates a Travis client for private github repository builds
    pub fn pro_async(
        credential: Option<Credential>,
        handle: &Handle,
    ) -> Future<Self> {
        let connector = create_connector(handle);
        let http = HyperClient::builder()
            .keep_alive(true)
            .build(connector);
        Client::custom(PRO_HOST, http, credential)
    }
}

impl<C> Client<C>
where
    C: Clone + Connect + 'static,
{
    /// Creates a Travis client for hosted versions of travis
    pub fn custom<H>(
        host: H,
        http: HyperClient<C>,
        credential: Option<Credential>,
    ) -> Future<Self>
    where
        H: Into<String>,
    {
        match credential {
            Some(Credential::Github(gh)) => {
                // exchange github token for travis token
                let host = host.into();
                let http_client = http.clone();
                let uri = Uri::from_str(&format!("{host}/auth/github", host = host))
                    .map_err(Error::from)
                    .into_future();
                let response = uri.and_then(move |uri| {
                    let mut req = Request::post(uri);
                    req.header(USER_AGENT, format!("Travis/{}", env!("CARGO_PKG_VERSION")));
                    req.header(ACCEPT, "application/vnd.travis-ci.2+json");
                    req.header(CONTENT_TYPE, "json");
                    let req = req.body::<Body>(
                        serde_json::to_vec(
                            &GithubToken { github_token: gh.to_owned() },
                        ).unwrap().into(),
                    ).unwrap();
                    http_client.request(req).map_err(Error::from)
                });

                let parse = response.and_then(move |response| {
                    let status = response.status();
                    let body = response.into_body().concat2().map_err(Error::from);
                    body.and_then(move |body| if status.is_success() {
                        debug!(
                            "body {}",
                            ::std::str::from_utf8(&body).unwrap()
                        );
                        serde_json::from_slice::<AccessToken>(&body).map_err(
                            |error| {
                                ErrorKind::Codec(error).into()
                            },
                        )
                    } else {
                        if StatusCode::FORBIDDEN == status {
                            return Err(
                                ErrorKind::Fault {
                                    code: status,
                                    error: String::from_utf8_lossy(&body)
                                        .into_owned()
                                        .clone(),
                                }.into(),
                            );
                        }
                        debug!(
                            "{} err {}",
                            status,
                            ::std::str::from_utf8(&body).unwrap()
                        );
                        match serde_json::from_slice::<ClientError>(&body) {
                            Ok(error) => Err(
                                ErrorKind::Fault {
                                    code: status,
                                    error: error.error_message,
                                }.into(),
                            ),
                            Err(error) => Err(ErrorKind::Codec(error).into()),
                        }
                    })
                });
                let client = parse.map(move |access| {
                    Self {
                        http,
                        credential: Some(Credential::Token(
                            access.access_token.to_owned(),
                        )),
                        host: host.into(),
                    }
                });
                Box::new(client)
            }
            _ => Box::new(future::ok(Self {
                http,
                credential,
                host: host.into(),
            })),
        }
    }

    /// get a list of repos for the a given owner (user or org)
    pub fn repos(&self) -> Repos<C> {
        Repos { travis: self.clone() }
    }

    /// get a ref to an env for a given repo slug
    pub fn env<'a, R>(&self, slug: R) -> Env<C>
    where
        R: Into<Cow<'a, str>>,
    {
        Env {
            travis: &self,
            slug: escape(slug.into().as_ref()),
        }
    }

    /// get a ref builds associated with a repo slug
    pub fn builds<'a, R>(&self, slug: R) -> Builds<C>
    where
        R: Into<Cow<'a, str>>,
    {
        Builds {
            travis: self.clone(),
            slug: escape(slug.into().as_ref()),
        }
    }

    /// get a ref to jobs associated with a build
    pub fn jobs(&self, build_id: usize) -> Jobs<C> {
        Jobs {
            travis: &self,
            build_id: build_id,
        }
    }

    pub fn raw_log(&self, job_id: u64) -> Stream<Chunk> {
        Box::new(
            self.raw_request(
                "GET",
                None,
                format!(
                    "{host}/job/{job_id}/log.txt",
                    host = self.host,
                    job_id = job_id
                ).parse()
                    .map_err(Error::from)
                    .into_future(),
            )
                .map(|stream| stream.map_err(Error::from))
                .flatten_stream()
        )
    }

    pub(crate) fn patch<T, B>(
        &self,
        uri: FutureResult<Uri, Error>,
        body: B,
    ) -> Future<T>
    where
        T: DeserializeOwned + 'static,
        B: Serialize,
    {
        self.request::<T>(
            "PATCH",
            Some(serde_json::to_vec(&body).unwrap()),
            uri,
        )
    }

    pub(crate) fn post<T, B>(
        &self,
        uri: FutureResult<Uri, Error>,
        body: B,
    ) -> Future<T>
    where
        T: DeserializeOwned + 'static,
        B: Serialize,
    {
        self.request::<T>(
            "POST",
            Some(serde_json::to_vec(&body).unwrap()),
            uri,
        )
    }

    pub(crate) fn get<T>(&self, uri: FutureResult<Uri, Error>) -> Future<T>
    where
        T: DeserializeOwned + 'static,
    {
        self.request::<T>("GET", None, uri)
    }

    pub(crate) fn delete(&self, uri: FutureResult<Uri, Error>) -> Future<()> {
        Box::new(self.request::<()>("DELETE", None, uri).then(
            |result| {
                match result {
                    Err(Error(ErrorKind::Codec(_), _)) => Ok(()),
                    otherwise => otherwise,
                }
            },
        ))
    }

    pub(crate) fn raw_request(
        &self,
        method: &'static str,
        body: Option<Vec<u8>>,
        uri: FutureResult<Uri, Error>,
    ) -> Future<Body> {
        let http_client = self.http.clone();
        let credential = self.credential.clone();
        let response = uri.and_then(move |uri| {
            let mut req = Request::builder();
            req.method(method);
            req.uri(uri);
            req.header(USER_AGENT, format!("Travis/{}", env!("CARGO_PKG_VERSION")));
            req.header("Travis-Api-Version", "3");
            req.header(CONTENT_TYPE, "json");
            if let Some(Credential::Token(ref token)) = credential {
                req.header(AUTHORIZATION, format!("token {}", token));
            }
            let body: Option<Body> = body.map(|b| b.into());
            let req = req.body::<Body>(body.unwrap_or_else(Body::empty)).unwrap();
            http_client.request(req).map_err(Error::from)
        });
        let result = response.and_then(|response| -> Future<Body> {
            let status = response.status();
            if status.is_success() {
                Box::new(future::ok(response.into_body()))
            } else {
                let body = response.into_body().concat2().map_err(Error::from);
                Box::new(body.and_then(move |body| {
                    debug!(
                        "{} err {}",
                        status,
                        ::std::str::from_utf8(&body).unwrap()
                    );
                    match serde_json::from_slice::<ClientError>(&body) {
                        Ok(error) => Err(
                            ErrorKind::Fault {
                                code: status,
                                error: error.error_message,
                            }.into(),
                        ),
                        Err(error) => Err(ErrorKind::Codec(error).into()),
                    }
                }))
            }
        });

        Box::new(result)
    }

    pub(crate) fn request<T>(
        &self,
        method: &'static str,
        body: Option<Vec<u8>>,
        uri: FutureResult<Uri, Error>,
    ) -> Future<T>
    where
        T: DeserializeOwned + 'static,
    {
        let result = self.raw_request(method, body, uri).and_then(|body| {
            body.concat2().map_err(Error::from)
        }).and_then(|body| {
            debug!("body {}", ::std::str::from_utf8(&body).unwrap());
            serde_json::from_slice::<T>(&body).map_err(|error| {
                ErrorKind::Codec(error).into()
            })
        });

        Box::new(result)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
