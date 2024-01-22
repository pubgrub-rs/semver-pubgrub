use arbitrary::Arbitrary;

#[derive(Arbitrary)]
enum ArbitraryOp {
    Caret,
    Tilde,
    Greater,
    GreaterEq,
    Less,
    LessEq,
    Exact,
    Wildcard,
}

#[derive(Arbitrary)]
pub struct ArbitraryComparator {
    op: ArbitraryOp,
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
    pre: Option<u8>,
}

impl ArbitraryComparator {
    pub fn to_comparator(&self) -> semver::Comparator {
        let op = match self.op {
            ArbitraryOp::Caret => semver::Op::Caret,
            ArbitraryOp::Tilde => semver::Op::Tilde,
            ArbitraryOp::Greater => semver::Op::Greater,
            ArbitraryOp::GreaterEq => semver::Op::GreaterEq,
            ArbitraryOp::Less => semver::Op::Less,
            ArbitraryOp::LessEq => semver::Op::LessEq,
            ArbitraryOp::Exact => semver::Op::Exact,
            ArbitraryOp::Wildcard => semver::Op::Wildcard,
        };
        let patch = self.minor.and(self.patch);
        let pre = patch
            .and(self.pre)
            .map(|p| p.to_string())
            .map(|p| semver::Prerelease::new(&p).unwrap())
            .unwrap_or_default();
        semver::Comparator {
            op,
            major: self.major,
            minor: self.minor,
            patch,
            pre,
        }
    }
}

impl std::fmt::Debug for ArbitraryComparator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_comparator().fmt(f)
    }
}

#[derive(Arbitrary)]
pub struct ArbitraryVersion {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Option<u8>,
    build: Option<u8>,
}

impl ArbitraryVersion {
    pub fn to_version(&self) -> semver::Version {
        semver::Version {
            major: self.major,
            minor: self.minor,
            patch: self.patch,
            pre: self
                .pre
                .map(|p| p.to_string())
                .map(|p| semver::Prerelease::new(&p).unwrap())
                .unwrap_or_default(),
            build: self
                .build
                .map(|p| p.to_string())
                .map(|p| semver::BuildMetadata::new(&p).unwrap())
                .unwrap_or_default(),
        }
    }
}

impl std::fmt::Debug for ArbitraryVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_version().fmt(f)
    }
}
