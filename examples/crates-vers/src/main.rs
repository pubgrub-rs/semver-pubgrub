use std::collections::{BTreeSet, HashMap, HashSet};

use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use pubgrub::version_set::VersionSet;
use rayon::prelude::*;
use semver::{Version, VersionReq};
use semver_pubgrub::SemverPubgrub;

fn main() {
    let index = crates_index::GitIndex::new_cargo_default().unwrap();
    // index.update().unwrap();
    let versions: BTreeSet<Version> = index
        .crates_parallel()
        .flat_map_iter(|crt| {
            crt.unwrap()
                .versions()
                .iter()
                .filter_map(|ver| {
                    let e = ver.version().parse();
                    match e {
                        Ok(e) => Some(e),
                        Err(e) => {
                            eprintln!("{}: {e:?}", ver.version());
                            None
                        }
                    }
                })
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect();
    let requirements: HashMap<VersionReq, SemverPubgrub> = index
        .crates_parallel()
        .flat_map_iter(|crt| {
            crt.unwrap()
                .versions()
                .iter()
                .flat_map(|ver| ver.dependencies())
                .map(|dep| dep.requirement().into())
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect::<HashSet<Box<str>>>()
        .into_par_iter()
        .filter_map(|req| {
            let e = req.parse::<VersionReq>();
            match e {
                Ok(e) => Some(e),
                Err(e) => {
                    eprintln!("{}: {e:?}", req);
                    None
                }
            }
        })
        .map(|req| {
            let pver: SemverPubgrub = (&req).into();
            (req, pver)
        })
        .collect();

    let style = ProgressBar::new(requirements.len() as u64).with_style(
        ProgressStyle::with_template(
            "[Time: {elapsed}, Rate: {per_sec}, Remaining: {eta}] {wide_bar} {pos:>6}/{len:6}: {percent:>3}%",
        )
        .unwrap(),
    );

    requirements
        .par_iter()
        .progress_with(style)
        .for_each(|(req, pver)| {
            for ver in &versions {
                if req.matches(&ver) != pver.contains(ver) {
                    eprintln!("{}", ver);
                    eprintln!("{}", req);
                    dbg!(&pver);
                    assert_eq!(req.matches(ver), pver.contains(ver));
                }
            }

            let neg = pver.complement();
            for ver in &versions {
                if !req.matches(ver) != neg.contains(ver) {
                    eprintln!("{}", ver);
                    eprintln!("{}", req);
                    dbg!(&neg);
                    assert_eq!(!req.matches(ver), neg.contains(ver));
                }
            }

            for (req2, pver2) in &requirements {
                let inter: SemverPubgrub = pver2.intersection(&pver);
                for ver in &versions {
                    assert_eq!(req.matches(&ver) && req2.matches(&ver), inter.contains(ver));
                }
            }
        })
}
