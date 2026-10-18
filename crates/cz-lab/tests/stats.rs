//! `paired_stats` against EXP-004's `summarize.py` (ADR-0015 5-1): the
//! same rows, the same pairs, the mean and d_z to rounding, the bootstrap
//! interval's end points within the Monte Carlo error of two different
//! random sequences. The reference values were printed by `summarize.py`'s
//! `paired()` on 2026-10-18 (`python3 -I`, `random.Random(20261018)`,
//! 10,000 resamples) over `data/EXP-004/`.

use cz_lab::{paired_stats, CellTables, Columns, Contrast, Metric, Table};

fn data(path: &str) -> String {
    let full = format!(
        "{}/../../docs/research/experiments/data/{path}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{full}: {e}"))
}

fn tables(exp: &str, cell: &str) -> CellTables {
    CellTables {
        summary: Table::parse(&data(&format!("{exp}/{cell}_summary.csv"))).unwrap(),
        series: Table::parse(&data(&format!("{exp}/{cell}_series.csv"))).unwrap(),
    }
}

fn ratio(num: &str, den: &[&str]) -> Metric {
    Metric::Ratio {
        num: Columns(vec![num.to_string()]),
        den: Columns(den.iter().map(|s| s.to_string()).collect()),
    }
}

/// One reference line of `summarize.py`'s `paired()`.
struct Reference {
    treat: &'static str,
    control: &'static str,
    metric: Metric,
    window: (u64, u64),
    n: usize,
    mean: f64,
    lo: f64,
    hi: f64,
    d_z: f64,
    neg: usize,
    pos: usize,
    ratio: f64,
    mean_a: f64,
    mean_b: f64,
}

fn references() -> Vec<Reference> {
    let el = || ratio("encroachments_learner", &["gathers_learner"]);
    let late = (10_000, 20_000);
    vec![
        Reference {
            treat: "le",
            control: "le0",
            metric: el(),
            window: late,
            n: 30,
            mean: -0.06689397255351673,
            lo: -0.06844871014082582,
            hi: -0.06540259784819796,
            d_z: -15.427821146322533,
            neg: 30,
            pos: 0,
            ratio: 0.16361924807917932,
            mean_a: 0.013086314414935067,
            mean_b: 0.07998028696845179,
        },
        Reference {
            treat: "le",
            control: "le0",
            metric: el(),
            window: (2_000, 4_000),
            n: 30,
            mean: -0.08173691486736868,
            lo: -0.08420351268585415,
            hi: -0.0792471392740663,
            d_z: -11.607093365775103,
            neg: 30,
            pos: 0,
            ratio: 0.14840854038672283,
            mean_a: 0.014244455007438268,
            mean_b: 0.09598136987480695,
        },
        Reference {
            treat: "le",
            control: "le0",
            metric: Metric::Mean("alive_learner".to_string()),
            window: late,
            n: 30,
            mean: -456.22866666666664,
            lo: -503.98266666666666,
            hi: -406.907,
            d_z: -3.3695080600138367,
            neg: 30,
            pos: 0,
            ratio: 0.703958884752591,
            mean_a: 1084.8703333333333,
            mean_b: 1541.099,
        },
        Reference {
            treat: "le",
            control: "le0",
            metric: ratio(
                "deaths_violence",
                &["deaths_violence", "deaths_starvation", "deaths_old_age"],
            ),
            window: late,
            n: 30,
            mean: 0.43793408828922464,
            lo: 0.4190575844082243,
            hi: 0.4573098724223902,
            d_z: 8.116166996552657,
            neg: 0,
            pos: 30,
            ratio: 4.274319254820277,
            mean_a: 0.5716822216285505,
            mean_b: 0.1337481333393259,
        },
        Reference {
            treat: "le0",
            control: "l0",
            metric: el(),
            window: late,
            n: 30,
            mean: 0.006424777146736097,
            lo: 0.005011229922046276,
            hi: 0.007823187319189517,
            d_z: 1.6047970932989652,
            neg: 2,
            pos: 28,
            ratio: 1.087345967179189,
            mean_a: 0.07998028696845179,
            mean_b: 0.0735555098217157,
        },
        Reference {
            treat: "l1",
            control: "l0",
            metric: el(),
            window: late,
            n: 10,
            mean: -0.027055383788123333,
            lo: -0.03076141202094243,
            hi: -0.023173753696737586,
            d_z: -4.151964632120462,
            neg: 10,
            pos: 0,
            ratio: 0.6341785701637278,
            mean_a: 0.046902513648974034,
            mean_b: 0.07395789743709737,
        },
        Reference {
            treat: "n_blind",
            control: "le",
            metric: ratio("sanctions", &["violations"]),
            window: late,
            n: 10,
            mean: -1.0275813024171647,
            lo: -1.100670642385883,
            hi: -0.953577577335289,
            d_z: -8.200113971266953,
            neg: 10,
            pos: 0,
            ratio: 0.6886201968473517,
            mean_a: 2.272508465811674,
            mean_b: 3.3000897682288386,
        },
        Reference {
            treat: "n_nomem",
            control: "le",
            metric: ratio("episodes_sanctioned", &["episodes_closed"]),
            window: late,
            n: 10,
            mean: -0.00012895259810842276,
            lo: -0.0067531397185073725,
            hi: 0.0056366371665526604,
            d_z: -0.012405328241759836,
            neg: 3,
            pos: 7,
            ratio: 0.9998662249821707,
            mean_a: 0.9638223157392728,
            mean_b: 0.9639512683373812,
        },
        Reference {
            treat: "w0",
            control: "le0",
            metric: Metric::Summary("win_act_rest".to_string()),
            window: late,
            n: 10,
            mean: 0.041753000000000005,
            lo: 0.036484,
            hi: 0.047088000000000005,
            d_z: 4.554937144326917,
            neg: 0,
            pos: 10,
            ratio: 1.4367285887619767,
            mean_a: 0.137357,
            mean_b: 0.095604,
        },
        Reference {
            treat: "n_nocomm",
            control: "le",
            metric: Metric::Summary("win_consumed_per_agent_tick".to_string()),
            window: late,
            n: 10,
            mean: -3.899999999999737e-05,
            lo: -0.0009990000000000027,
            hi: 0.0007680000000000076,
            d_z: -0.025967775071233757,
            neg: 4,
            pos: 6,
            ratio: 0.9997774518811022,
            mean_a: 0.175204,
            mean_b: 0.175243,
        },
    ]
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn paired_stats_agree_with_exp004_summarize_py() {
    let mut worst_ci = 0.0f64;
    for r in references() {
        let contrast = Contrast {
            name: format!("{} - {}", r.treat, r.control),
            treat: r.treat.to_string(),
            control: r.control.to_string(),
            metric: r.metric.clone(),
            window: Some(r.window),
        };
        let p = paired_stats(
            &tables("EXP-004", r.treat),
            &tables("EXP-004", r.control),
            &contrast,
            (10_000, 20_000),
        )
        .unwrap();
        let what = format!("{} − {} {}", r.treat, r.control, r.metric);
        assert_eq!((p.n, p.dropped), (r.n, 0), "{what}");
        assert_eq!((p.negative, p.positive), (r.neg, r.pos), "{what}");
        // The mean and the two cell means are sums of the same numbers:
        // equal to floating-point rounding.
        let scale = |x: f64| 1e-9 * x.abs().max(1.0);
        assert!(
            close(p.mean, r.mean, scale(r.mean)),
            "{what}: mean {} vs {}",
            p.mean,
            r.mean
        );
        assert!(
            close(p.mean_treat, r.mean_a, scale(r.mean_a)),
            "{what}: mean_treat"
        );
        assert!(
            close(p.mean_control, r.mean_b, scale(r.mean_b)),
            "{what}: mean_control"
        );
        assert!(
            close(p.d_z, r.d_z, scale(r.d_z)),
            "{what}: d_z {} vs {}",
            p.d_z,
            r.d_z
        );
        assert!(close(p.ratio, r.ratio, scale(r.ratio)), "{what}: ratio");
        // The interval: two generators, one Monte Carlo error. A quantile
        // of 10,000 resampled means has a standard error of about 2.7% of
        // the bootstrap sd, i.e. 0.7% of the interval's width; 4% of the
        // width is five of those, and ±1e-4 the floor the ADR names.
        let tol = (0.04 * (r.hi - r.lo)).max(1e-4);
        let (dlo, dhi) = ((p.lo - r.lo).abs(), (p.hi - r.hi).abs());
        worst_ci = worst_ci.max(dlo / tol).max(dhi / tol);
        assert!(
            dlo <= tol && dhi <= tol,
            "{what}: CI [{}, {}] vs [{}, {}] (tolerance {tol})",
            p.lo,
            p.hi,
            r.lo,
            r.hi
        );
        println!(
            "{what}: mean {:+.6} d_z {:+.3} CI [{:+.6}, {:+.6}] vs py [{:+.6}, {:+.6}] (Δlo {:.2e}, Δhi {:.2e})",
            p.mean, p.d_z, p.lo, p.hi, r.lo, r.hi, p.lo - r.lo, p.hi - r.hi
        );
    }
    println!("worst CI deviation: {:.2} of the tolerance", worst_ci);
}

/// N-nosanction and LE0 are physically identical worlds (EXP-004 P3): the
/// paired difference is exactly 0 in every seed, with an empty d_z.
#[test]
fn an_identical_pair_has_a_zero_interval_and_no_d_z() {
    let contrast = Contrast {
        name: "N-nosanction − LE0".to_string(),
        treat: "n_nosanction".to_string(),
        control: "le0".to_string(),
        metric: ratio("encroachments_learner", &["gathers_learner"]),
        window: None,
    };
    let p = paired_stats(
        &tables("EXP-004", "n_nosanction"),
        &tables("EXP-004", "le0"),
        &contrast,
        (10_000, 20_000),
    )
    .unwrap();
    assert_eq!(p.window, (10_000, 20_000));
    assert_eq!(p.n, 10);
    assert_eq!((p.mean, p.lo, p.hi), (0.0, 0.0, 0.0));
    assert!(p.d_z.is_nan());
    assert_eq!(p.ratio, 1.0);
    assert_eq!((p.negative, p.positive), (0, 0));
    assert!(p.pairs.iter().all(|&(_, d)| d == 0.0));
    assert_eq!(p.ci_text(4), "0.0000 [0.0000, 0.0000], d_z —, n 10");
}

/// Extinct seeds leave the pairs: EXP-004's random null model is extinct
/// in every seed, so against any arm there is nothing to pair.
#[test]
fn extinct_seeds_are_dropped_from_the_pairs() {
    let contrast = Contrast {
        name: "N-rand − LE".to_string(),
        treat: "n_rand".to_string(),
        control: "le".to_string(),
        metric: Metric::Summary("win_mean".to_string()),
        window: None,
    };
    let p = paired_stats(
        &tables("EXP-004", "n_rand"),
        &tables("EXP-004", "le"),
        &contrast,
        (10_000, 20_000),
    )
    .unwrap();
    assert_eq!((p.n, p.dropped), (0, 10));
    assert!(p.mean.is_nan() && p.lo.is_nan() && p.hi.is_nan());
    assert_eq!(p.ci_text(4), "—");
}

/// A column the rows do not have is an error, not a silent zero.
#[test]
fn an_unknown_column_is_an_error() {
    let contrast = Contrast {
        name: "typo".to_string(),
        treat: "le".to_string(),
        control: "le0".to_string(),
        metric: ratio("encroachments_leaner", &["gathers_learner"]),
        window: None,
    };
    let err = paired_stats(
        &tables("EXP-004", "le"),
        &tables("EXP-004", "le0"),
        &contrast,
        (10_000, 20_000),
    )
    .unwrap_err();
    assert!(err.0.contains("encroachments_leaner"), "{err}");
}

/// The metrics read from a hand-written table.
#[test]
fn metrics_read_the_window_as_defined() {
    let summary =
        Table::parse("seed,extinct_at,win_mean,win_act_rest\n0,-,100.5,0.25\n1,300,0,\n").unwrap();
    let series = Table::parse(
        "seed,tick,alive,a,b\n0,0,10,0,0\n0,100,8,1,4\n0,200,6,2,4\n0,300,4,3,0\n1,0,9,5,5\n",
    )
    .unwrap();
    let t = CellTables { summary, series };
    assert_eq!(t.seeds().unwrap(), vec![0, 1]);
    assert_eq!(t.extinct(0).unwrap(), Some(false));
    assert_eq!(t.extinct(1).unwrap(), Some(true));
    assert_eq!(t.extinct(2).unwrap(), None);
    let w = (0, 300);
    // (0, 300]: rows at 100, 200, 300.
    assert_eq!(
        t.metric(0, &ratio("a", &["b"]), w).unwrap(),
        Some(6.0 / 8.0)
    );
    assert_eq!(t.metric(0, &ratio("a", &["b"]), (200, 300)).unwrap(), None);
    assert_eq!(
        t.metric(0, &Metric::Mean("alive".to_string()), w).unwrap(),
        Some(6.0)
    );
    assert_eq!(
        t.metric(0, &Metric::PerTick("a".to_string()), (100, 300))
            .unwrap(),
        Some(5.0 / 200.0)
    );
    assert_eq!(
        t.metric(0, &Metric::Summary("win_mean".to_string()), w)
            .unwrap(),
        Some(100.5)
    );
    assert_eq!(
        t.metric(1, &Metric::Summary("win_act_rest".to_string()), w)
            .unwrap(),
        None
    );
    assert_eq!(
        t.metric(
            0,
            &Metric::SummaryRatio {
                num: Columns(vec!["win_act_rest".to_string()]),
                den: Columns(vec!["win_mean".to_string(), "win_act_rest".to_string()]),
            },
            w
        )
        .unwrap(),
        Some(0.25 / 100.75)
    );
    assert!(Table::parse("a,b\n1,2,3\n").is_err());
}
