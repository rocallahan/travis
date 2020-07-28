//! interfaces for interacting with travis envs

use futures::prelude::*;

use super::{Client, Error, Future};
use hyper::client::connect::Connect;
use std::borrow::Cow;

#[derive(Debug, Deserialize)]
struct EnvVarsWrapper {
    env_vars: Vec<EnvVar>,
}

#[derive(Debug, Serialize)]
pub struct EnvVarCreate {
    #[serde(rename = "env_var.name")]
    pub name: String,
    #[serde(rename = "env_var.value")]
    pub value: String,
    #[serde(rename = "env_var.public")]
    pub public: bool,
}

#[derive(Debug, Serialize)]
pub struct EnvVarPatch {
    #[serde(rename = "env_var.name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "env_var.value", skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(rename = "env_var.public", skip_serializing_if = "Option::is_none")]
    pub public: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct EnvVar {
    pub id: String,
    pub name: Option<String>,
    pub public: Option<bool>,
    pub value: Option<String>,
    #[serde(rename = "@permissions")]
    pub permissions: EnvVarPermissions,
}

#[derive(Debug, Deserialize)]
pub struct EnvVarPermissions {
    pub read: bool,
    pub write: bool,
}

/// Interface for travis repositorty env vars
///
/// This is typicall accessed through the travis client
/// via `travis.env("owner/repo")`
pub struct Env<'a, C>
where
    C: Clone + Connect + Send + Sync + 'static,
{
    pub(crate) travis: &'a Client<C>,
    pub(crate) slug: String,
}

impl<'a, C> Env<'a, C>
where
    C: Clone + Connect + Send + Sync + 'static,
{
    /// Return a vector of EnvVars
    pub fn vars(&self) -> Future<Vec<EnvVar>> {
        let host = self.travis.host.clone();
        let slug = self.slug.clone();
        Box::pin(
            self.travis
                .get(async move {
                    format!(
                        "{host}/repo/{slug}/env_vars",
                        host = host,
                        slug = slug
                    ).parse()
                        .map_err(Error::from)
                })
                .and_then(
                    |wrapper: EnvVarsWrapper| future::ok(wrapper.env_vars),
                ),
        )
    }

    /// gets an env var by id
    pub fn get<'v, V>(&self, var_id: V) -> Future<EnvVar>
    where
        V: Into<Cow<'v, str>>,
    {
        let host = self.travis.host.clone();
        let slug = self.slug.clone();
        let var_id = var_id.into().to_string();
        self.travis.get(async move {
            format!(
                "{host}/repo/{slug}/env_var/{var_id}",
                host = host,
                slug = slug,
                var_id = var_id,
            ).parse()
                .map_err(Error::from)
        })
    }

    /// updates the contents of an env var
    pub fn update<'v, V>(
        &self,
        var_id: V,
        options: EnvVarPatch,
    ) -> Future<EnvVar>
    where
        V: Into<Cow<'v, str>>,
    {
        let host = self.travis.host.clone();
        let slug = self.slug.clone();
        let var_id = var_id.into().to_string();
        self.travis.patch(async move {
            format!(
                "{host}/repo/{slug}/env_var/{var_id}",
                host = host,
                slug = slug,
                var_id = var_id,
            ).parse()
                .map_err(Error::from)
        },
                          options,
        )
    }

    /// sets a new env var for this repo
    pub fn set(&self, options: EnvVarCreate) -> Future<EnvVar> {
        let host = self.travis.host.clone();
        let slug = self.slug.clone();
        self.travis.post(async move {
            format!(
                "{host}/repo/{slug}/env_vars",
                host = host,
                slug = slug
            ).parse()
                .map_err(Error::from)
        },
                         options,
        )
    }

    /// deletes env var
    pub fn delete<'v, V>(&self, var_id: V) -> Future<()>
    where
        V: Into<Cow<'v, str>>,
    {
        let host = self.travis.host.clone();
        let slug = self.slug.clone();
        let var_id = var_id.into().to_string();
        self.travis.delete(async move {
            format!(
                "{host}/repo/{slug}/env_var/{var_id}",
                host = host,
                slug = slug,
                var_id = var_id,
            ).parse()
                .map_err(Error::from)
        })
    }
}
