use std::ops::RangeBounds;
use std::{cmp::min, collections::BTreeSet};

use hibitset::{BitSet, BitSetLike};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressFinish, ProgressStyle};
use pubgrub::VersionSet as _;
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
    let mut versions: Vec<Version> = std::fs::read_to_string("./data/versions.csv")
        .ok()?
        .lines()
        .map(|ver| ver.parse().unwrap())
        .collect();
    versions.sort();
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
    let contains = arg.is_empty() || arg.contains(&"contains".to_string());

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
            for (((id, ver), pver_mat), neg_mat) in versions
                .iter()
                .enumerate()
                .zip(pver.contains_many(versions.iter()))
                .zip(neg.contains_many(versions.iter()))
            {
                let mat = req.matches(ver);
                if contains {
                    assert_eq!(pver.contains(ver), pver_mat);
                    assert_eq!(neg.contains(ver), neg_mat);
                }
                if mat != pver_mat {
                    eprintln!("{}", ver);
                    eprintln!("{}", req);
                    dbg!(&pver);
                    assert_eq!(req.matches(ver), pver.contains(ver));
                }
                if !mat != neg_mat {
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
                if let Some(com) = pver.only_one_compatibility_range() {
                    let s_com: SemverCompatibility = s.into();
                    let e_com: SemverCompatibility = e.into();
                    if s_com != com || e_com != com {
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
            (pver, bitset)
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
            .for_each(|(i, (pver, bs))| {
                for (pver2, bs2) in &requirements[(i + 1)..] {
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
                        let min_mat = (&bs_inter).iter().next().map(|x| x as usize);
                        let max_mat = (&bs_inter).iter().last().map(|x| x as usize);

                        let bounding_range = inter.bounding_range().expect("inter is not empty");
                        if let (Some(s), Some(e)) = (min_mat, max_mat) {
                            assert!(s <= e);
                            assert!(bounding_range.contains(&versions[s]));
                            assert!(bounding_range.contains(&versions[e]));
                        }
                        let start = min_mat.unwrap_or(0).saturating_sub(30);
                        let end = min(max_mat.unwrap_or(!0).saturating_add(30), versions.len() - 1);
                        for (id, inter_mat) in
                            (start..=end).zip(inter.contains_many(versions[start..=end].iter()))
                        {
                            let ver = &versions[id as usize];
                            let mat = bs_inter.contains(id as u32);
                            if contains {
                                assert_eq!(inter.contains(ver), inter_mat);
                            }
                            assert_eq!(mat, inter.contains(ver));
                        }
                    }
                }
            })
    }
}
