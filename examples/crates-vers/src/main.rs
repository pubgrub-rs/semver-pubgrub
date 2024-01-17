use std::ops::RangeBounds;
use std::{cmp::min, collections::BTreeSet};

use hibitset::{BitSet, BitSetLike};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressFinish, ProgressStyle};
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

fn read_files() -> Option<(Vec<Version>, Vec<(VersionReq, SemverPubgrub)>)> {
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

    let template =  "contains: [Time: {elapsed}, Rate: {per_sec}, Remaining: {eta}] {wide_bar} {pos:>6}/{len:6}: {percent:>3}%";
    let style = ProgressBar::new(requirements.len() as u64)
        .with_style(ProgressStyle::with_template(template).unwrap())
        .with_finish(ProgressFinish::AndLeave);

    let requirements: Vec<_> = requirements
        .into_par_iter()
        .progress_with(style)
        .map(|(req, pver)| {
            let bounding_range = pver.bounding_range();
            let neg = pver.complement();
            let mut bitset = BitSet::new();
            for (id, ver) in versions.iter().enumerate() {
                let mat = req.matches(ver);
                if mat != pver.contains(ver) {
                    eprintln!("{}", ver);
                    eprintln!("{}", req);
                    dbg!(&pver);
                    assert_eq!(req.matches(ver), pver.contains(ver));
                }
                if !mat != neg.contains(ver) {
                    eprintln!("{}", ver);
                    eprintln!("{}", req);
                    dbg!(&neg);
                    assert_eq!(!req.matches(ver), neg.contains(ver));
                }
                if mat {
                    if !bounding_range.unwrap().contains(&ver) {
                        eprintln!("{}", ver);
                        eprintln!("{}", req);
                        dbg!(&pver);
                        assert!(bounding_range.unwrap().contains(&ver));
                    }
                    bitset.add(id.try_into().unwrap());
                }
            }
            (req, pver, bitset)
        })
        .collect();

    if intersection {
        let template = "intersection: [Time: {elapsed}, Rate: {per_sec}, Remaining: {eta}] {wide_bar} {pos:>6}/{len:6}: {percent:>3}%";
        let style = ProgressBar::new(requirements.len() as u64)
            .with_style(ProgressStyle::with_template(template).unwrap())
            .with_finish(ProgressFinish::AndLeave);

        requirements
            .par_iter()
            .progress_with(style)
            .enumerate()
            .for_each(|(i, (_, pver, bs))| {
                for (_, pver2, bs2) in &requirements[(i + 1)..] {
                    let inter: SemverPubgrub = pver2.intersection(&pver);
                    assert_eq!(inter, pver.intersection(&pver2));
                    let bs_inter: BitSet = (bs & bs2).into_iter().collect();
                    if inter == SemverPubgrub::empty() {
                        assert!(bs_inter.is_empty())
                    } else if &inter == pver {
                        assert_eq!(&bs_inter, bs)
                    } else if &inter == pver2 {
                        assert_eq!(&bs_inter, bs2)
                    } else {
                        let start = (bs | bs2).iter().min().unwrap_or(0).saturating_sub(30);
                        let end = min(
                            (bs | bs2).iter().max().unwrap_or(!0).saturating_add(30),
                            versions.len() as u32,
                        );
                        let bounding_range = inter.bounding_range().expect("inter is not empty");
                        for id in start..end {
                            let ver = &versions[id as usize];
                            let mat = bs_inter.contains(id);
                            assert_eq!(mat, inter.contains(ver));

                            let bb_contains = bounding_range.contains(&ver);
                            if mat && !bb_contains {
                                unreachable!("bounding_range thinks this can not match");
                            }
                        }
                    }
                }
            })
    }
}
