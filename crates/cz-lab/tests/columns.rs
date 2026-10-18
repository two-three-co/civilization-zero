//! The CSV columns are a contract (ADR-0015 completion condition 5): the
//! headers `cz sweep` / `cz lab run` write are pinned here as literal
//! strings, and the registered experiments' files must carry them (the
//! older ones as a prefix: every column added since went before
//! `ticks_per_s`, so EXP-001 … 004's `summarize.py` keep reading). 5-2
//! appended the eleven cohort columns to the summary (so EXP-004's summary
//! header is now a prefix too) and added the `agents` table.

use cz_lab::{
    CohortMetrics, Tally, AGENTS_COLUMNS, QPOLICY_COLUMNS, SERIES_COLUMNS, SUMMARY_COLUMNS,
};

const SUMMARY: &str = "seed,policy,final_alive,extinct_at,win_mean,win_min,win_max,win_max_over_min,births,deaths_starvation,deaths_old_age,deaths_violence,mutation,learner_states,learner_epsilon,win_ticks,win_agent_ticks,win_consumed_per_agent_tick,win_gathered_per_agent_tick,win_births_per_tick,win_transfers_per_tick,win_harms_per_tick,win_signals_per_tick,win_proposals_per_tick,win_acceptances_per_tick,win_rejections_per_tick,win_deaths_starvation,win_deaths_old_age,win_deaths_violence,win_act_move,win_act_gather,win_act_consume,win_act_rest,win_act_transfer,win_act_store,win_act_harm,win_act_signal,hunger_permille_mean,q_abs_mean,q_delta_window,reflex_weight_drift,nomos,learner_health_weight,win_gathers_per_tick,win_encroachments_per_tick,win_encroach_rate,win_encroach_rate_forager,win_encroach_rate_random,win_encroach_rate_reflex,win_encroach_rate_learner,win_promises_due_per_tick,win_promises_broken_per_tick,win_food_standing_mean,win_regrown_per_tick,rules,win_violations_per_tick,win_sanctions_per_tick,win_episodes_closed,win_episodes_sanctioned,win_s,win_s_prime,harm_cost,win_encroach_rate_enforcer,motions,motions_carried,motions_defeated,win_votes_per_tick,enforcer_share,see_standing,signal_radius,memory_slots,win_alive_learner_mean,win_alive_enforcer_mean,win_sanctions_learner_per_tick,win_sanctions_enforcer_per_tick,win_births_reassigned_per_tick,win_cohort_learners,win_cohort_survived,win_cohort_died_violence,win_e_learner_survivors,win_e_learner_removed,win_encroach_share_removed,win_e_survivors_first_half,win_e_survivors_second_half,win_e_survivors_delta_mean,win_sanctioned_learners,win_sanctioned_then_died,ticks_per_s";

const SERIES: &str = "seed,tick,alive,food,gathered,consumed,births,transfers,harms,signals,proposals,acceptances,rejections,deaths_starvation,deaths_old_age,deaths_violence,gathers,encroachments,promises_due,promises_broken,regrown,gathers_forager,encroachments_forager,gathers_random,encroachments_random,gathers_reflex,encroachments_reflex,gathers_learner,encroachments_learner,violations,sanctions,episodes_closed,episodes_sanctioned,gathers_enforcer,encroachments_enforcer,motions,votes,motions_carried,motions_defeated,births_forager,births_random,births_reflex,births_learner,births_enforcer,births_reassigned,sanctions_forager,sanctions_random,sanctions_reflex,sanctions_learner,sanctions_enforcer,alive_forager,alive_random,alive_reflex,alive_learner,alive_enforcer";

const QPOLICY: &str = "seed,row,hungry,has_food,food_here,food_in_sight,adjacent,occupied,violator_adjacent,accused,upper,heard,learners,move,gather,consume,rest,transfer,store,harm,signal,tie,visits,visits_window";

const AGENTS: &str = "seed,id,policy,born,died,cause,cohort,gathers,encroachments,sanctions_received,sanctions_given,win_gathers,win_encroachments,win_sanctions_received,win_sanctions_given,win_gathers_first_half,win_encroachments_first_half,win_gathers_second_half,win_encroachments_second_half,last_sanctioned";

fn data(path: &str) -> String {
    let full = format!(
        "{}/../../docs/research/experiments/data/{path}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{full}: {e}"))
}

fn header(path: &str) -> String {
    data(path).lines().next().unwrap().to_string()
}

#[test]
fn the_summary_columns_are_pinned() {
    assert_eq!(SUMMARY_COLUMNS, SUMMARY);
    assert!(SUMMARY_COLUMNS.ends_with(",ticks_per_s"));
    // 5-2: the cohort columns sit right before the clock.
    assert!(SUMMARY_COLUMNS.ends_with(&format!(",{},ticks_per_s", CohortMetrics::COLUMNS)));
    assert_eq!(SUMMARY_COLUMNS.split(',').count(), 88);
}

#[test]
fn the_agents_columns_are_pinned() {
    assert_eq!(AGENTS_COLUMNS, AGENTS);
}

#[test]
fn the_series_columns_are_pinned() {
    assert_eq!(SERIES_COLUMNS, SERIES);
    assert_eq!(
        SERIES_COLUMNS,
        format!(
            "seed,tick,alive,food,{},{}",
            Tally::COUNT_COLUMNS,
            Tally::ALIVE_COLUMNS
        )
    );
}

#[test]
fn the_qpolicy_columns_are_pinned() {
    assert_eq!(QPOLICY_COLUMNS, QPOLICY);
}

/// EXP-004's files (the latest registered experiment) carry exactly the
/// series and qpolicy headers written today; its summary header is the
/// 77-column one of 4-6, a prefix of today's 88 (5-2's cohort columns).
#[test]
fn exp004_files_carry_todays_headers() {
    let summary = header("EXP-004/le_summary.csv");
    assert_eq!(summary.split(',').count(), 77);
    assert!(SUMMARY_COLUMNS.starts_with(summary.trim_end_matches(",ticks_per_s")));
    assert_eq!(header("EXP-004/le_series.csv"), SERIES_COLUMNS);
    assert_eq!(header("EXP-004/le_qpolicy.csv"), QPOLICY_COLUMNS);
    assert_eq!(header("EXP-004/n_rand_series.csv"), SERIES_COLUMNS);
}

/// Every older experiment's columns — all but `ticks_per_s` — are a prefix
/// of today's: nothing moved, everything new went before `ticks_per_s`.
#[test]
fn older_experiments_columns_are_prefixes_of_todays() {
    let todays: Vec<&str> = SUMMARY_COLUMNS.split(',').collect();
    for file in [
        "EXP-001/forager_summary.csv",
        "EXP-002/forager_summary.csv",
        "EXP-002/learner32_summary.csv",
        "EXP-003/forager_summary.csv",
        "EXP-003/grid_e10_h2000_w0_summary.csv",
        "EXP-004/l0_summary.csv",
        "EXP-004/le_summary.csv",
    ] {
        let cols: Vec<String> = header(file).split(',').map(String::from).collect();
        assert_eq!(cols.last().unwrap(), "ticks_per_s", "{file}");
        let body = &cols[..cols.len() - 1];
        assert!(
            body.len() < todays.len() && body.iter().zip(&todays).all(|(a, b)| a == b),
            "{file}: columns are not a prefix of today's"
        );
    }
    let todays: Vec<&str> = SERIES_COLUMNS.split(',').collect();
    for file in [
        "EXP-002/forager_series.csv",
        "EXP-003/forager_series.csv",
        "EXP-003/learner128_series.csv",
    ] {
        let cols: Vec<String> = header(file).split(',').map(String::from).collect();
        assert!(
            cols.len() <= todays.len() && cols.iter().zip(&todays).all(|(a, b)| a == b),
            "{file}: columns are not a prefix of today's"
        );
    }
    assert_eq!(header("EXP-003/learner128_qpolicy.csv"), QPOLICY_COLUMNS);
}
