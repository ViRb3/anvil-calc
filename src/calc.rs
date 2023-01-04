extern crate core;

use std::cmp;
use std::collections::HashMap;
use itertools::{Itertools};
use serde::{Deserialize, Serialize};
use tinyvec::{tiny_vec, TinyVec};
use std::mem;

const PIECE_TYPE_BOOK: MA = 0;
const PIECE_TYPE_ITEM: MA = 1;

const MS: usize = mem::size_of::<MB>() * 8;

type MA = u8;
type MB = u16;
type MC = u32;

#[derive(Default, Clone)]
struct Piece {
    name_mask: MB,
    value: MA,
    work_count: MA,
    extra_cost: MA,
    ptype: MA,
}

#[derive(Default, Clone)]
struct TraceRecord {
    left: Piece,
    right: Piece,
    cost: MC,
}

fn calc_xp(level: MC) -> MC {
    return if level < 16 {
        level.pow(2) + 6 * level
    } else if level < 32 {
        (2.5 * level.pow(2) as f64 - 40.5 * level as f64 + 360.0) as MC
    } else {
        (4.5 * level.pow(2) as f64 - 162.5 * level as f64 + 2220.0) as MC
    };
}

fn calc_level(xp: MC) -> MC {
    let mut test_xp = 0;
    let mut level = 0;
    while test_xp < xp {
        level += 1;
        test_xp = calc_xp(level)
    }
    return level;
}

fn calc_penalty(work_count: MC) -> MC {
    return (1 << work_count) - 1;
}

fn anvil(books_free: bool, left: &Piece, right: &Piece) -> (Piece, MC) {
    let new_type = match (left.ptype, right.ptype) {
        (PIECE_TYPE_BOOK, PIECE_TYPE_BOOK) => PIECE_TYPE_BOOK,
        _ => PIECE_TYPE_ITEM,
    };
    if books_free && new_type == PIECE_TYPE_BOOK {
        return (Piece {
            name_mask: left.name_mask | right.name_mask,
            value: left.value + right.value,
            work_count: 0,
            extra_cost: left.extra_cost + right.extra_cost,
            ptype: new_type,
        }, 0);
    }
    let cost = calc_xp(right.value as MC + calc_penalty(left.work_count as MC) +
        calc_penalty(right.work_count as MC) + left.extra_cost as MC + right.extra_cost as MC);
    return (Piece {
        name_mask: left.name_mask | right.name_mask,
        value: left.value + right.value,
        work_count: cmp::max(left.work_count, right.work_count) + 1,
        extra_cost: left.extra_cost + right.extra_cost,
        ptype: new_type,
    }, cost);
}

fn solve(permutations: &HashMap<usize, Vec<Vec<usize>>>, books_free: bool, queue: &[Piece], total_cost: MC, trace: &[TraceRecord]) -> (MC, Box<[TraceRecord]>) {
    let mut best_trace: Option<Box<[TraceRecord]>> = None;
    let mut best_cost = 4_294_967_295;
    for order in permutations.get(&queue.len()).expect("need to precompute more permuations") {
        let left = &queue[order[0]];
        let right = &queue[order[1]];
        if right.ptype == PIECE_TYPE_ITEM {
            continue;
        }
        let (combined, cost) = anvil(books_free, left, right);
        if total_cost + cost > best_cost {
            continue;
        }
        let new_queue = TinyVec::<[Piece; MS]>::from_iter(
            if order[0] < order[1] {
                queue[..order[0]].iter()
                    .chain(queue[order[0] + 1..order[1]].iter())
                    .chain(queue[order[1] + 1..].iter())
                    .cloned()
                    .chain(std::iter::once(combined))
            } else {
                queue[..order[1]].iter()
                    .chain(queue[order[1] + 1..order[0]].iter())
                    .chain(queue[order[0] + 1..].iter())
                    .cloned()
                    .chain(std::iter::once(combined))
            });
        if new_queue.len() > 1 {
            let new_trace = TinyVec::<[TraceRecord; MS]>::from_iter(trace.iter()
                .cloned()
                .chain(std::iter::once(TraceRecord {
                    left: left.clone(),
                    right: right.clone(),
                    cost,
                })));
            let (result_cost, result_trace) = solve(&permutations, books_free, &new_queue, total_cost + cost, &new_trace);
            if best_trace.is_none() || result_cost < best_cost {
                best_trace = Some(result_trace);
                best_cost = result_cost;
            }
        } else {
            let result_cost = total_cost + cost;
            if best_trace.is_none() || result_cost < best_cost {
                best_trace = Some(Box::from(Vec::from_iter(trace.iter()
                    .cloned()
                    .chain(std::iter::once(TraceRecord {
                        left: left.clone(),
                        right: right.clone(),
                        cost,
                    })))));
                best_cost = result_cost;
            }
        }
    }
    return (best_cost, match best_trace {
        Some(a) => a,
        _ => panic!("trace is null")
    });
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    books_free: bool,
}

type InputPiece = (String, MA, MA, MA);

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
        .filter(|(i, _)| (name_mask >> i) & 1 == 1)
        .map(|(_, n)| n).join(" + ");
}

pub fn process(config: ConfigSchema) {
    let mut pieces = Vec::new();
    let mut names = Vec::new();
    let item_iter = config.input.items.iter()
        .map(|item| (item, PIECE_TYPE_ITEM));
    let book_iter = config.input.books.iter()
        .map(|item| (item, PIECE_TYPE_BOOK));
    for (i, (piece, ptype)) in item_iter.chain(book_iter).enumerate() {
        let (name, value, work_count, extra_cost) = piece.clone();
        names.push(name);
        pieces.push(Piece {
            name_mask: 0 | 1 << i,
            value,
            work_count,
            extra_cost,
            ptype,
        })
    }
    let mut permutations = HashMap::new();
    for ceil in 2..MS + 1 {
        permutations.insert(ceil, (0..ceil).permutations(2)
            .map(|v| v.iter().cloned().map(|x| x as usize).collect_vec())
            .collect_vec());
    }
    println!("Calculating...");
    let trace = tiny_vec!([TraceRecord; 0]);
    let (best_cost, best_order) = solve(&permutations, config.config.books_free, &pieces, 0, &trace);
    println!("Done");
    let mut total_level_cost = 0;
    let mut max_xp_cost = 0;
    for i in 0..best_order.len() {
        let left = &best_order[i].left;
        let right = &best_order[i].right;
        let xp_cost = best_order[i].cost.clone();
        let level_cost = calc_level(xp_cost.clone());
        total_level_cost += level_cost;
        if xp_cost > max_xp_cost {
            max_xp_cost = xp_cost
        }
        println!("{}. [{}: {} {}] + [{}: {} {}] = {} ({}xp)", i + 1, get_name(&names, left.name_mask), left.value, left.extra_cost,
                 get_name(&names, right.name_mask), right.value, right.extra_cost,
                 level_cost, xp_cost);
    }
    println!("Max step cost: {} ({}xp)", calc_level(max_xp_cost), max_xp_cost);
    println!("Final best cost: {} ({}xp)", calc_level(best_cost), best_cost);
    println!("Final worst cost: {} ({}xp)", total_level_cost, calc_xp(total_level_cost));
}
