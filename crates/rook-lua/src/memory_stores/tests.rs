use serde_json::json;

use super::*;

const T0: Time = Time(0.0);

fn keys(items: &[sorted_map::RangeItem]) -> Vec<&str> {
    items.iter().map(|i| i.key.as_str()).collect()
}

/// The creator-docs example of a sorted map's ascending order.
fn docs_example() -> SortedMap {
    let mut map = SortedMap::default();
    let num = |n| Some(SortKey::Number(n));
    for (key, sort) in [
        ("player7", None),
        ("player3", num(2.5)),
        ("player6", Some(SortKey::String("someString".into()))),
        ("player5", num(1.0)),
        ("player0", None),
        ("player1", num(-1.0)),
        ("player4", num(1.0)),
        ("player2", num(0.0)),
    ] {
        map.set(T0, key, json!(key), 60.0, sort).unwrap();
    }
    map
}

#[test]
fn sorted_map_orders_numbers_then_strings_then_none_then_keys() {
    let mut map = docs_example();
    let all = map.range(T0, true, 200, &Bound::default(), &Bound::default());
    assert_eq!(
        keys(&all),
        [
            "player1", "player2", "player4", "player5", "player3", "player6", "player0", "player7"
        ]
    );
    let down = map.range(T0, false, 3, &Bound::default(), &Bound::default());
    assert_eq!(keys(&down), ["player7", "player0", "player6"]);
}

#[test]
fn sorted_map_ranges_are_exclusive_at_both_bounds() {
    let mut map = docs_example();
    let after_player2 = Bound {
        key: Some("player2".into()),
        sort: Some(SortKey::Number(0.0)),
    };
    let before_strings = Bound {
        key: None,
        sort: Some(SortKey::String(String::new())),
    };
    let r = map.range(T0, true, 200, &after_player2, &before_strings);
    assert_eq!(keys(&r), ["player4", "player5", "player3"]);
    let r = map.range(T0, false, 200, &after_player2, &before_strings);
    assert_eq!(keys(&r), ["player3", "player5", "player4"]);
    // A sort key alone is past every item that has it.
    let past_one = Bound {
        key: None,
        sort: Some(SortKey::Number(1.0)),
    };
    let r = map.range(T0, true, 200, &past_one, &Bound::default());
    assert_eq!(keys(&r)[0], "player3");
    let r = map.range(T0, false, 200, &Bound::default(), &past_one);
    assert_eq!(keys(&r), ["player2", "player1"]);
    // A key alone stands among the items without a sort key.
    let after_player0 = Bound {
        key: Some("player0".into()),
        sort: None,
    };
    let r = map.range(T0, true, 200, &after_player0, &Bound::default());
    assert_eq!(keys(&r), ["player7"]);
}

#[test]
fn sorted_map_set_reports_overwrites_and_moves_the_item() {
    let mut map = SortedMap::default();
    assert!(
        !map.set(T0, "a", json!(1), 60.0, Some(SortKey::Number(5.0)))
            .unwrap()
    );
    assert!(map.set(T0, "a", json!(2), 60.0, None).unwrap());
    assert_eq!(map.get(T0, "a").unwrap(), Some((json!(2), None)));
    assert_eq!(map.len(T0), 1);
    map.remove(T0, "a").unwrap();
    assert_eq!(map.get(T0, "a").unwrap(), None);
}

#[test]
fn items_expire_on_the_clock_and_overwrites_reset_it() {
    let mut map = SortedMap::default();
    map.set(T0, "a", json!(1), 10.0, None).unwrap();
    map.set(T0, "b", json!(1), 0.0, None).unwrap();
    assert_eq!(map.get(T0, "b").unwrap(), None);
    map.set(Time(5.0), "a", json!(2), 10.0, None).unwrap();
    assert!(map.get(Time(12.0), "a").unwrap().is_some());
    assert_eq!(map.get(Time(15.0), "a").unwrap(), None);
    assert_eq!(map.len(Time(15.0)), 0);

    let mut hash = HashMap::default();
    hash.set(T0, "k", json!("v"), 3.0).unwrap();
    assert_eq!(hash.get(Time(2.9), "k").unwrap(), Some(json!("v")));
    assert_eq!(hash.get(Time(3.0), "k").unwrap(), None);
}

#[test]
fn keys_values_and_expirations_are_checked() {
    let mut map = SortedMap::default();
    assert_eq!(
        map.set(T0, "", json!(1), 1.0, None),
        Err(MemoryStoreError::EmptyKey)
    );
    let long = "k".repeat(129);
    assert_eq!(
        map.set(T0, &long, json!(1), 1.0, None),
        Err(MemoryStoreError::KeyTooLong)
    );
    let big = json!("x".repeat(MAX_VALUE_SIZE));
    assert_eq!(
        map.set(T0, "k", big, 1.0, None),
        Err(MemoryStoreError::ValueTooLarge)
    );
    assert_eq!(
        SortKey::string("s".repeat(129)),
        Err(MemoryStoreError::SortKeyTooLong)
    );
    assert_eq!(expiration(-1.0), Err(MemoryStoreError::Expiration));
    assert_eq!(expiration(3_888_001.0), Err(MemoryStoreError::Expiration));
    assert_eq!(expiration(3_888_000.5), Ok(3_888_000.0));
    assert_eq!(
        count(0.0, MAX_RANGE_COUNT),
        Err(MemoryStoreError::Count(200))
    );
    assert_eq!(count(100.0, MAX_READ_COUNT), Ok(100));
}

#[test]
fn queues_read_by_priority_then_order_and_hide_what_they_read() {
    let mut q = Queue::default();
    for (v, p) in [("a", 0.0), ("b", 0.0), ("urgent", 5.0)] {
        q.add(T0, json!(v), 60.0, p).unwrap();
    }
    let (values, first) = q.read(T0, 2, false, 30.0).unwrap();
    assert_eq!(values, [json!("urgent"), json!("a")]);
    assert_eq!(q.len(T0, false), 3);
    assert_eq!(q.len(T0, true), 1);
    let (values, second) = q.read(T0, 5, false, 30.0).unwrap();
    assert_eq!(values, [json!("b")]);
    assert!(q.read(T0, 1, false, 30.0).is_none());
    q.remove(T0, &second);
    assert_eq!(q.len(T0, false), 2);
    // The first read's hiding runs out: its items show again, and its id
    // no longer removes them.
    let later = Time(30.0);
    q.remove(later, &first);
    assert_eq!(q.len(later, true), 2);
    let (values, _) = q.read(later, 2, false, 30.0).unwrap();
    assert_eq!(values, [json!("urgent"), json!("a")]);
}

#[test]
fn queue_all_or_nothing_and_expiry() {
    let mut q = Queue::default();
    q.add(T0, json!(1), 10.0, 0.0).unwrap();
    assert!(q.read(T0, 2, true, 30.0).is_none());
    assert_eq!(
        q.read(T0, 2, false, 30.0).map(|r| r.0),
        Some(vec![json!(1)])
    );
    assert_eq!(q.len(Time(10.0), false), 0);
}

#[test]
fn hash_map_pages_resume_after_the_last_key() {
    let mut hash = HashMap::default();
    for k in ["c", "a", "b"] {
        hash.set(T0, k, json!(k), 60.0).unwrap();
    }
    let (page, more) = hash.page(T0, None, 2);
    assert_eq!(
        page.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert!(more);
    let (page, more) = hash.page(T0, Some("b"), 2);
    assert_eq!(page, [("c".to_owned(), json!("c"))]);
    assert!(!more);
}
