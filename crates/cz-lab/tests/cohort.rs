//! The per-agent frame (ADR-0015 D4, 5-1's skeleton): births, deaths and
//! sanctions read from every tick's report agree with the ledger and the
//! living.

use cz_lab::cohort::CohortTally;
use cz_lab::policy_mix;
use simulation_core::{Intervention, Norm, NormKind, Sanction, World, WorldConfig};

#[test]
fn the_frame_tracks_every_agent_seen() {
    let mut w = World::generate(
        WorldConfig {
            seed: 7,
            width: 48,
            height: 48,
            initial_agents: 150,
            patch_count: 6,
            ..WorldConfig::institution_preset()
        },
        policy_mix("institution").unwrap(),
    )
    .unwrap();
    w.intervene(Intervention::Enact {
        norm: Norm {
            kind: NormKind::Encroach,
            standing_ticks: 32,
            sanction: Sanction::HARM,
            except_sanction: false,
        },
    })
    .unwrap();
    let mut t = CohortTally::new(&w);
    assert_eq!(t.len(), 150);
    for _ in 0..400 {
        let r = w.step();
        t.observe(&r, &w);
    }
    assert_eq!(t.len(), w.next_agent_id as usize, "founders + births");
    assert_eq!(
        t.iter().filter(|a| a.born > 0).count() as u64,
        w.ledger.births
    );
    assert_eq!(
        t.iter().filter(|a| !a.alive()).count() as u64,
        w.ledger.deaths
    );
    let living: Vec<_> = t.iter().filter(|a| a.alive()).map(|a| a.id).collect();
    assert_eq!(
        living,
        w.agents.iter().map(|a| a.id).collect::<Vec<_>>(),
        "the living are exactly the untallied deaths, in id order"
    );
    for a in &w.agents {
        assert_eq!(t.get(a.id).unwrap().policy, a.policy);
    }
    let given: u64 = t.iter().map(|a| a.sanctions_given).sum();
    let received: u64 = t.iter().map(|a| a.sanctions_received).sum();
    let ledger: u64 = w.ledger.sanctions.iter().sum();
    assert_eq!((given, received), (ledger, ledger));
    assert!(
        ledger > 0,
        "the ruled world sanctioned someone in 400 ticks"
    );
    // A death carries its tick and cause.
    let dead = t.iter().find(|a| !a.alive()).expect("someone died");
    let (tick, _) = dead.died.unwrap();
    assert!(tick < 400 && tick >= dead.born.saturating_sub(1));
}
