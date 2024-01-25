use std::ops::RangeBounds;
use std::{cmp::min, collections::BTreeSet};

use hibitset::{BitSet, BitSetLike};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressFinish, ProgressStyle};
use pubgrub::version_set::VersionSet;
use rayon::prelude::*;
use semver::{Version, VersionReq};
use semver_pubgrub::{SemverCompatibility, SemverPubgrub};
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
    let arg: Vec<_> = std::env::args().skip(1).collect();

    if arg.is_empty() || arg.contains(&"get-from-index".to_string()) {
        get_files_from_index();
    }

    let intersection = arg.is_empty() || arg.contains(&"intersection".to_string());

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
                    bitset.add(id.try_into().unwrap());
                }
            }
            if !bitset.is_empty() {
                let s = &versions[(&bitset).iter().next().unwrap() as usize];
                let e = &versions[(&bitset).iter().last().unwrap() as usize];
                if !pver.more_then_one_compatibility_range() {
                    let s_com: SemverCompatibility = s.into();
                    let e_com: SemverCompatibility = e.into();
                    if s_com != e_com {
                        eprintln!("req: {}", req);
                        eprintln!("s: {}", s);
                        eprintln!("e: {}", e);
                        dbg!(&pver);
                        assert_eq!(s_com, e_com);
                    }
                }
                let bounding_range = pver.bounding_range().expect("inter is not empty");
                assert!(bounding_range.contains(s));
                assert!(bounding_range.contains(e));
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
                        assert!(bs_inter.is_empty());
                        assert!(inter.bounding_range().is_none());
                    } else if &inter == pver {
                        assert_eq!(&bs_inter, bs);
                    } else if &inter == pver2 {
                        assert_eq!(&bs_inter, bs2);
                    } else {
                        let min_mat = (&bs_inter).iter().next();
                        let max_mat = (&bs_inter).iter().last();

                        let bounding_range = inter.bounding_range().expect("inter is not empty");
                        if let (Some(s), Some(e)) = (min_mat, max_mat) {
                            assert!(s <= e);
                            assert!(bounding_range.contains(&versions[s as usize]));
                            assert!(bounding_range.contains(&versions[e as usize]));
                        }
                        let start = min_mat.unwrap_or(0).saturating_sub(30);
                        let end = min(
                            max_mat.unwrap_or(!0).saturating_add(30),
                            versions.len() as u32 - 1,
                        );
                        for id in start..=end {
                            let ver = &versions[id as usize];
                            let mat = bs_inter.contains(id);
                            assert_eq!(mat, inter.contains(ver));
                        }
                    }
                }
            })
    }
}
