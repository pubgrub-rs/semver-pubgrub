use std::ops::RangeBounds;
use std::{cmp::min, collections::BTreeSet};

use hibitset::{BitSet, BitSetLike};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressFinish, ProgressStyle};
use pubgrub::SetRelation;
use rayon::prelude::*;
use semver::{Version, VersionReq};
use semver_pubgrub::{SemverCompatibility, SemverPubgrub, SmallVersion, VersionLike};
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

fn read_files() -> Option<(Vec<Version>, Vec<(VersionReq, SemverPubgrub<Version>)>)> {
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
            let pver: SemverPubgrub<Version> = (&req).into();
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
    let difference = arg.is_empty() || arg.contains(&"difference".to_string());
    let relation = arg.is_empty() || arg.contains(&"relation".to_string());

    let Some((versions, requirements)) = read_files() else {
        panic!("no files");
    };

    assert!(versions.is_sorted());
    assert!(versions.is_sorted_by_key(|v| SemverCompatibility::from(v)));

    let template =  "versions: [Time: {elapsed}, Rate: {per_sec}, Remaining: {eta}] {wide_bar} {pos:>6}/{len:6}: {percent:>3}%";
    let style = ProgressBar::new(versions.len() as u64)
        .with_style(ProgressStyle::with_template(template).unwrap())
        .with_finish(ProgressFinish::AndLeave);

    versions.par_iter().progress_with(style).for_each(|v1| {
        let s1: SmallVersion = v1.into();
        assert_eq!(v1.major, s1.major());
        assert_eq!(v1.minor, s1.minor());
        assert_eq!(v1.patch, s1.patch());
        assert_eq!(v1.pre.as_str(), s1.pre());
        // small version round trips
        let v: Version = s1.into_version();
        assert_eq!(v1, &v);
        for v2 in &versions {
            let s2: SmallVersion = v2.into();
            assert_eq!(s1.cmp(&s2), v1.cmp(v2), "{v1} {s1:#?} cmp {v2} {s2:#?} ");
            assert_eq!(s1 == s2, v1 == v2);
        }
    });

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
            let mut pver_matches = pver.contains_many(versions.iter());
            let mut neg_matches = neg.contains_many(versions.iter());
            for (id, ver) in versions.iter().enumerate() {
                let pver_mat = pver_matches.next().expect("missing membership result");
                let neg_mat = neg_matches
                    .next()
                    .expect("missing complement membership result");
                let mat = req.matches(ver);
                assert_eq!(pver.contains(ver), pver_mat);
                assert_eq!(neg.contains(ver), neg_mat);
                if mat != pver_mat {
                    eprintln!("{}", ver);
                    eprintln!("{}", req);
                    dbg!(&pver);
                    assert_eq!(mat, pver_mat);
                }
                if !mat != neg_mat {
                    eprintln!("{}", ver);
                    eprintln!("{}", req);
                    dbg!(&neg);
                    assert_eq!(!mat, neg_mat);
                }
                if mat {
                    bitset.add(id.try_into().unwrap());
                }
            }
            assert_eq!(pver_matches.count(), 0);
            assert_eq!(neg_matches.count(), 0);
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
                    let inter: SemverPubgrub<Version> = pver2.intersection(&pver);
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
                            assert_eq!(inter.contains(ver), inter_mat);
                            assert_eq!(mat, inter.contains(ver));
                        }
                    }
                }
            })
    }

    if relation {
        let template = "relation: [Time: {elapsed}, Rate: {per_sec}, Remaining: {eta}] {wide_bar} {pos:>6}/{len:6}: {percent:>3}%";
        let style = ProgressBar::new(requirements.len() as u64)
            .with_style(ProgressStyle::with_template(template).unwrap())
            .with_finish(ProgressFinish::AndLeave);

        requirements
            .par_iter()
            .progress_with(style)
            .for_each(|(pver, bs)| {
                for (pver2, bs2) in &requirements {
                    let expected = if pver.subset_of(pver2) {
                        SetRelation::Subset
                    } else if pver.is_disjoint(pver2) {
                        SetRelation::Disjoint
                    } else {
                        SetRelation::Overlapping
                    };
                    let relation = pver.relation(pver2);
                    assert_eq!(relation, expected, "{pver:?} relative to {pver2:?}");
                    match relation {
                        SetRelation::Subset => assert!((bs & !bs2).iter().next().is_none()),
                        SetRelation::Disjoint => assert!((bs & bs2).iter().next().is_none()),
                        SetRelation::Overlapping => {}
                    }
                }
            })
    }

    if difference {
        let template = "difference: [Time: {elapsed}, Rate: {per_sec}, Remaining: {eta}] {wide_bar} {pos:>6}/{len:6}: {percent:>3}%";
        let style = ProgressBar::new(requirements.len() as u64)
            .with_style(ProgressStyle::with_template(template).unwrap())
            .with_finish(ProgressFinish::AndLeave);

        requirements
            .par_iter()
            .progress_with(style)
            .for_each(|(pver, _)| {
                for (pver2, _) in &requirements {
                    // Test based on the definition of set difference, as checking versions is to slow.
                    let diff = pver.difference(pver2);
                    let diff_deff = pver.intersection(&pver2.complement());
                    assert_eq!(diff, diff_deff);
                }
            })
    }
}
