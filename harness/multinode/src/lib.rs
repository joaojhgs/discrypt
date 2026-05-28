//! Headless multinode harness for discrypt acceptance tests.
use discrypt_core::create_dm;
use discrypt_mls_core::Identity;

/// Build two fresh identities and return their safety number.
#[must_use]
pub fn two_node_dm_safety_number() -> String {
    let a = Identity::generate("alice");
    let b = Identity::generate("bob");
    let (_g, safety) = create_dm(&a, &b);
    safety
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_node_dm_has_safety_number() {
        assert!(!two_node_dm_safety_number().is_empty());
    }
}
