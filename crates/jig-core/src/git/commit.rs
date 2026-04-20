use std::fmt;

/// Wrapper around a git2 commit OID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Oid(git2::Oid);

impl Oid {
    pub(crate) fn new(oid: git2::Oid) -> Self {
        Self(oid)
    }

    pub fn sha(&self) -> String {
        self.0.to_string()
    }

    pub fn short_sha(&self) -> String {
        self.0.to_string()[..7].to_string()
    }

    pub(crate) fn inner(&self) -> git2::Oid {
        self.0
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
