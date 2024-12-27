use semver::Version;

pub trait VersionLike: From<Version> + Clone + Ord {
    fn major(&self) -> u64;
    fn minor(&self) -> u64;
    fn patch(&self) -> u64;
    fn pre(&self) -> &str;
}

impl VersionLike for Version {
    fn major(&self) -> u64 {
        self.major
    }

    fn minor(&self) -> u64 {
        self.minor
    }

    fn patch(&self) -> u64 {
        self.patch
    }

    fn pre(&self) -> &str {
        &self.pre
    }
}
