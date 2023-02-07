extern crate core;

use std::cmp;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet};
use std::hash::{Hash, Hasher};
use itertools::{Itertools};
use serde::{Deserialize, Serialize};
use tinyvec::{tiny_vec, TinyVec};
use std::mem;

const PIECE_TYPE_BOOK: MB = 0;
const PIECE_TYPE_ITEM: MB = 1;

const MS: usize = mem::size_of::<MB>() * 8;

type MA = u8;
type MB = u16;
type MC = u32;

#[derive(Default, Debug, Clone, Hash)]
struct Piece {
    name_mask: MB,
    value: MA,
    work_count: MA,
}

#[derive(Default, Debug, Clone, Hash)]
struct TraceRecord {
    left: Piece,
    right: Piece,
}

fn calc_xp(level: MC) -> MC {
    if level < 16 {
        level.pow(2) + 6 * level
    } else if level < 32 {
        (2.5f64.mul_add(f64::from(level.pow(2)), -40.5 * f64::from(level)) + 360.0) as MC
    } else {
        (4.5f64.mul_add(f64::from(level.pow(2)), -162.5 * f64::from(level)) + 2220.0) as MC
    }
}

fn calc_level(xp: MC) -> MC {
    let mut test_xp = 0;
    let mut level = 0;
    while test_xp < xp {
        level += 1;
        test_xp = calc_xp(level);
    }
    level
}

const fn calc_penalty(work_count: MC) -> MC {
    (1 << work_count) - 1
}

const fn calc_work_count(penalty: MC) -> MC {
    let mut test_penalty = 0;
    let mut work_count = 0;
    while test_penalty < penalty {
        work_count += 1;
        test_penalty = calc_penalty(work_count);
    }
    work_count
}

fn anvil(config: &Config, left: &Piece, right: &Piece) -> (Piece, MC) {
    let new_name_mask = left.name_mask | right.name_mask;
    if config.books_free && new_name_mask & 1 == PIECE_TYPE_BOOK {
        return (Piece {
            name_mask: new_name_mask,
            value: left.value + right.value,
            work_count: 0,
        }, 0);
    }
    let mut cost = MC::from(right.value) + calc_penalty(MC::from(left.work_count)) +
        calc_penalty(MC::from(right.work_count));
    if config.optimize_xp {
        cost = calc_xp(cost);
    }
    (Piece {
        name_mask: new_name_mask,
        value: left.value + right.value,
        work_count: cmp::max(left.work_count, right.work_count) + 1,
    }, cost)
}

fn solve(config: &Config, null_paths: &mut HashSet<u64>, queue: &[Piece], total_cost: MC, mut best_cost: MC, trace: &[TraceRecord]) -> (MC, Option<Box<[TraceRecord]>>) {
    let mut hasher = DefaultHasher::new();
    queue.iter().sorted_by(|a, b|
        a.value.cmp(&b.value).then(a.work_count.cmp(&b.work_count))
    ).for_each(|x| {
        x.value.hash(&mut hasher);
        x.work_count.hash(&mut hasher);
    });
    total_cost.hash(&mut hasher);
    let queue_hash = hasher.finish();
    if null_paths.get(&queue_hash).is_some() {
        return (best_cost, None);
    }
    let mut best_trace: Option<Box<[TraceRecord]>> = None;
    let lefts = 0..queue.len();
    let pairs = lefts.flat_map(|l| {
        let rights = 0..queue.len();
        rights.filter(move |&r| r != l).map(move |r| (l, r))
    });
    for (o1, o2) in pairs {
        let left = &queue[o1];
        let right = &queue[o2];
        if left.name_mask & 1 == PIECE_TYPE_BOOK && right.name_mask & 1 == PIECE_TYPE_ITEM {
            continue;
        }
        // if both items are books, we will always want to minimize cost
        if left.name_mask & 1 == PIECE_TYPE_BOOK && right.value > left.value {
            continue;
        }
        let (combined, cost) = anvil(config, left, right);
        if total_cost + cost > best_cost {
            continue;
        }
        let (i1, i2) = if o1 < o2 { (o1, o2) } else { (o2, o1) };
        let new_queue = queue[..i1].iter()
            .chain(queue[i1 + 1..i2].iter())
            .chain(queue[i2 + 1..].iter())
            .cloned()
            .chain(std::iter::once(combined))
            .collect::<TinyVec<[Piece; MS]>>();
        if new_queue.len() > 1 {
            let new_trace = trace.iter()
                .cloned()
                .chain(std::iter::once(TraceRecord {
                    left: left.clone(),
                    right: right.clone(),
                })).collect::<TinyVec<[TraceRecord; MS]>>();
            let (result_cost, result_trace) = solve(config, null_paths, &new_queue, total_cost + cost, best_cost, &new_trace);
            if best_trace.is_none() || result_cost < best_cost {
                best_trace = result_trace;
                best_cost = result_cost;
            }
        } else {
            let result_cost = total_cost + cost;
            if best_trace.is_none() || result_cost < best_cost {
                best_trace = Some(Box::from(trace.iter()
                    .cloned()
                    .chain(std::iter::once(TraceRecord {
                        left: left.clone(),
                        right: right.clone(),
                    })).collect::<Vec<TraceRecord>>()));
                best_cost = result_cost;
            }
        }
    }
    if best_trace.is_none() {
        null_paths.insert(queue_hash);
    }
    (best_cost, best_trace)
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    books_free: bool,
    optimize_xp: bool,
}

type InputPiece = (String, String, MA);

#[derive(Debug, Serialize, Deserialize)]
struct Input {
    items: Vec<InputPiece>,
    books: Vec<InputPiece>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigSchema {
    config: Config,
    input: Input,
}

fn get_name(names: &[String], name_mask: MB) -> String {
    return names.iter()
        .enumerate()
        .filter(|(i, _)| (name_mask >> (i + 1)) & 1 == 1)
        .map(|(_, n)| n).join(" + ");
}

fn expand_cost(config: &Config, cost: MC) -> (MC, MC) {
    if config.optimize_xp {
        (calc_level(cost), cost)
    } else {
        (cost, calc_xp(cost))
    }
}

pub fn process(schema: ConfigSchema) -> String {
    let (input, config) = (schema.input, schema.config);
    let mut pieces = Vec::new();
    let mut names = Vec::new();
    let item_iter = input.items.iter()
        .map(|item| (item, PIECE_TYPE_ITEM));
    let book_iter = input.books.iter()
        .map(|item| (item, PIECE_TYPE_BOOK));
    for (i, (piece, ptype)) in item_iter.chain(book_iter).enumerate() {
        let (name, level_multiplier, penalty) = piece.clone();
        names.push(name);
        pieces.push(Piece {
            // last bit carries piece type
            name_mask: (1 << (i + 1)) | ptype,
            value: level_multiplier.split('x').map(|x| x.trim().parse::<MA>().unwrap()).product(),
            work_count: calc_work_count(MC::from(penalty)) as MA,
        });
    }

    let trace = tiny_vec!([TraceRecord; 0]);
    let mut null_paths: HashSet<u64> = HashSet::new();
    let (best_cost, best_order) = solve(&config, &mut null_paths, &pieces, 0, 4_294_967_295, &trace);
    let (best_level_cost, best_xp_cost) = expand_cost(&config, best_cost);
    let order = best_order.unwrap();
    let mut max_xp_cost = 0;
    let mut result = String::new();
    for i in 0..order.len() {
        let left = &order[i].left;
        let right = &order[i].right;
        let (_, cost) = anvil(&config, left, right);
        let (level_cost, xp_cost) = expand_cost(&config, cost);
        if xp_cost > max_xp_cost {
            max_xp_cost = xp_cost;
        }
        result += format!("{}. [{}: {},{}] + [{}: {},{}] = {} ({}xp)\n", i + 1,
                          get_name(&names, left.name_mask), left.value, calc_penalty(MC::from(left.work_count)),
                          get_name(&names, right.name_mask), right.value, calc_penalty(MC::from(right.work_count)),
                          level_cost, xp_cost).as_str();
    }
    result += "\n";
    result += format!("Max step cost: {} ({max_xp_cost}xp)\n", calc_level(max_xp_cost)).as_str();
    result += format!("Total cost: {best_level_cost} ({best_xp_cost}xp)\n").as_str();
    result
}
