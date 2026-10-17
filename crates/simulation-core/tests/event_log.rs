//! Event ledger (ADR-0010 2a-2): ordering, range queries, JSON Lines I/O.

use simulation_core::{
    AgentId, DeathCause, Event, EventLog, Intervention, LogError, PolicyKind, Pos, World,
    WorldConfig,
};
use std::io::Cursor;

fn logged_world(ticks: u64) -> (World, EventLog) {
    let mut w = World::new(WorldConfig {
        seed: 17,
        width: 32,
        height: 32,
        initial_agents: 100,
        patch_count: 3,
        ..WorldConfig::default()
    })
    .unwrap();
    let mut log = EventLog::new();
    for t in 0..ticks {
        if t == ticks / 2 {
            let e = w
                .intervene(Intervention::ScaleFood { permille: 500 })
                .unwrap();
            log.push(e).unwrap();
        }
        let r = w.step();
        log.record(&r).unwrap();
    }
    (w, log)
}

#[test]
fn ledger_matches_the_world_ledger_and_is_ordered() {
    let (w, log) = logged_world(600);
    let born = log
        .events()
        .iter()
        .filter(|e| matches!(e, Event::Born { .. }))
        .count() as u64;
    let died = log
        .events()
        .iter()
        .filter(|e| matches!(e, Event::Died { .. }))
        .count() as u64;
    assert!(born > 0 && died > 0, "need births and deaths for this test");
    assert_eq!(born, w.ledger.births);
    assert_eq!(died, w.ledger.deaths);
    assert_eq!(
        log.events()
            .iter()
            .filter(|e| matches!(e, Event::Intervened { .. }))
            .count(),
        1
    );
    assert!(log.events().windows(2).all(|p| p[0].tick() <= p[1].tick()));
    assert_eq!(log.last_tick().map(|t| t < 600), Some(true));
}

#[test]
fn range_queries_use_half_open_tick_ranges() {
    let (_, log) = logged_world(300);
    let manual: Vec<Event> = log
        .events()
        .iter()
        .copied()
        .filter(|e| (100..150).contains(&e.tick()))
        .collect();
    assert_eq!(log.range(100..150), manual.as_slice());
    let (hi, lo) = (150u64, 100u64);
    assert_eq!(
        log.range(hi..lo),
        &[] as &[Event],
        "reversed range is empty"
    );
    assert!(log.at(150).iter().all(|e| e.tick() == 150));
    assert_eq!(log.range(0..u64::MAX).len(), log.len());
    assert!(log.range(10_000..20_000).is_empty());
}

#[test]
fn jsonl_roundtrip_is_lossless_and_has_a_header() {
    let (_, log) = logged_world(400);
    let mut bytes = Vec::new();
    log.write_jsonl(&mut bytes).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let first = text.lines().next().unwrap();
    assert!(
        first.starts_with(r#"{"format":1,"kernel":"simulation-core "#),
        "{first}"
    );
    assert_eq!(text.lines().count(), log.len() + 1);
    let back = EventLog::read_jsonl(Cursor::new(bytes)).unwrap();
    assert_eq!(back, log);
}

#[test]
fn out_of_order_pushes_are_rejected() {
    let mut log = EventLog::new();
    let died = |tick| Event::Died {
        tick,
        agent: AgentId(1),
        cause: DeathCause::OldAge,
        pos: Pos::new(0, 0),
        age: 5,
        energy_written_off: 0,
        food_dropped: 0,
    };
    log.push(died(5)).unwrap();
    log.push(died(5)).unwrap(); // same tick is fine
    assert_eq!(
        log.push(died(4)),
        Err(LogError::OutOfOrder { tick: 4, last: 5 })
    );
    assert_eq!(log.len(), 2);
}

#[test]
fn bad_files_are_rejected_with_a_reason() {
    assert!(matches!(
        EventLog::read_jsonl(Cursor::new(b"")),
        Err(LogError::Header(_))
    ));
    assert!(matches!(
        EventLog::read_jsonl(Cursor::new(br#"{"format":99,"kernel":"x"}"#)),
        Err(LogError::Header(_))
    ));
    let text = concat!(
        r#"{"format":1,"kernel":"simulation-core 0.1.0"}"#,
        "\n",
        r#"{"kind":"born","tick":3,"child":9,"parent":1,"pos":{"x":1,"y":1},"energy":40000}"#,
        "\n",
        r#"{"kind":"unknown","tick":4}"#,
        "\n"
    );
    assert!(matches!(
        EventLog::read_jsonl(Cursor::new(text)),
        Err(LogError::Line { number: 3, .. })
    ));
    let text = concat!(
        r#"{"format":1,"kernel":"simulation-core 0.1.0"}"#,
        "\n",
        r#"{"kind":"intervened","tick":9,"intervention":{"op":"scale_food","permille":10}}"#,
        "\n",
        r#"{"kind":"intervened","tick":8,"intervention":{"op":"scale_food","permille":10}}"#,
        "\n"
    );
    assert_eq!(
        EventLog::read_jsonl(Cursor::new(text)),
        Err(LogError::OutOfOrder { tick: 8, last: 9 })
    );
}

#[test]
fn empty_log_roundtrips() {
    let log = EventLog::new();
    let mut bytes = Vec::new();
    log.write_jsonl(&mut bytes).unwrap();
    assert_eq!(EventLog::read_jsonl(Cursor::new(bytes)).unwrap(), log);
    assert_eq!(log.last_tick(), None);
    assert!(log.is_empty());
    let _ = PolicyKind::Forager; // keep the import list honest for future tests
}
