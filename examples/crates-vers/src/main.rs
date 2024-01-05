use std::collections::{BTreeSet, HashMap};

use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use pubgrub::version_set::VersionSet;
use rayon::prelude::*;
use semver::{Version, VersionReq};
use semver_pubgrub::SemverPubgrub;
use std::io::Write;

fn get_files_from_index() {
    println!("getting index");
    let mut index = crates_index::GitIndex::new_cargo_default().unwrap();
    index.update().unwrap();
    println!("collecting versions");
    let versions: BTreeSet<String> = index
        .crates_parallel()
        .flat_map_iter(|crt| {
            crt.unwrap()
                .versions()
                .iter()
                .filter_map(|ver| ver.version().parse::<Version>().ok())
                .map(|ver| ver.to_string())
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect();
    println!("writing version file");
    std::fs::create_dir_all("./data").unwrap();
    let mut file = std::fs::File::create("./data/versions.csv").unwrap();
    for ver in &versions {
        writeln!(file, "{}", ver).unwrap();
    }
    drop(file);
    println!("collecting requirements");
    let requirements: BTreeSet<String> = index
        .crates_parallel()
        .flat_map_iter(|crt| {
            crt.unwrap()
                .versions()
                .iter()
                .flat_map(|ver| ver.dependencies())
                .filter_map(|dep| dep.requirement().parse::<VersionReq>().ok())
                .map(|req| req.to_string())
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect();

    println!("writing requirements file");
    let mut file = std::fs::File::create("./data/requirements.csv").unwrap();
    for req in requirements {
        writeln!(file, "{}", req).unwrap();
    }
    drop(file);
}

fn read_files() -> Option<(BTreeSet<Version>, HashMap<VersionReq, SemverPubgrub>)> {
    let versions = std::fs::read_to_string("./data/versions.csv")
        .ok()?
        .lines()
        .map(|ver| ver.parse().unwrap())
        .collect();
    let requirements = std::fs::read_to_string("./data/requirements.csv")
        .ok()?
        .lines()
        .filter_map(|req| req.parse::<VersionReq>().ok())
        .map(|req| {
            let pver: SemverPubgrub = (&req).into();
            (req, pver)
        })
        .collect();
    Some((versions, requirements))
}

fn main() {
    // TODO: Use real argument pursing
    let arg = std::env::args().nth(1);

    if arg.is_none() || arg.as_deref() == Some("get-from-index") {
        get_files_from_index();
    }

    let intersection = arg.is_none() || arg.as_deref() == Some("intersection");

    let Some((versions, requirements)) = read_files() else {
        panic!("no files");
    };

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
            if intersection {
                for (req2, pver2) in &requirements {
                    let inter: SemverPubgrub = pver2.intersection(&pver);
                    for ver in &versions {
                        assert_eq!(req.matches(&ver) && req2.matches(&ver), inter.contains(ver));
                    }
                }
            }
        })
}
