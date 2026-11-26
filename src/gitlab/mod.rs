//! Merge Requests über GitLab und glab: Remotes und Host-Zuordnung (E16),
//! glab-Runner, GraphQL-Abfragen und Parser, Domänenlogik, Service (Abruf,
//! Prüfung, Token) und Checkout-Menü (K1).

pub mod activity;
pub mod checkout;
pub mod glab;
pub mod graphql;
pub mod model;
pub mod remote;
pub mod secret;
pub mod service;
pub mod time;

/// Gemeinsame Helfer der Tests der einzelnen Module.
#[cfg(test)]
pub(crate) mod test_support {
    use super::glab::{FakeGlab, FakeReply, GatedRunner, GlabOutput, HttpHead};
    use super::secret::SecretString;

    pub(crate) fn gated() -> GatedRunner<FakeGlab> {
        GatedRunner::new(FakeGlab::new())
    }

    pub(crate) fn secret(s: &str) -> SecretString {
        SecretString::new(s.to_string())
    }

    pub(crate) fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Antwort von `glab api graphql -i`: HTTP 200, bei `errors` Exit 1.
    pub(crate) fn gql(body: &str) -> FakeReply {
        let failed = body.contains("\"errors\"");
        FakeReply::output(GlabOutput {
            exit: i32::from(failed),
            http: Some(HttpHead {
                code: 200,
                reason: "OK".to_string(),
            }),
            body: body.to_string(),
            stderr_tail: if failed {
                "glab: GraphQL-Fehler".to_string()
            } else {
                String::new()
            },
        })
    }
}
