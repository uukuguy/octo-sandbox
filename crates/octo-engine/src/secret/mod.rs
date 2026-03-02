mod vault;
mod resolver;
mod taint;

#[cfg(test)]
mod vault_test;
#[cfg(test)]
mod resolver_test;

pub use vault::{CredentialVault, EncryptedStore};
pub use resolver::CredentialResolver;
pub use taint::{TaintLabel, TaintedValue, TaintSink, TaintViolation};
