extern crate core;

use std::cmp;
use std::collections::HashMap;
use itertools::{Itertools};
use serde::{Deserialize, Serialize};
use tinyvec::{tiny_vec, TinyVec};
use std::mem;

#[derive(Default, Clone)]
#[repr(u8)]
enum PieceType {
    #[default]
    Item,
    Book,
}

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
    ptype: PieceType,
}

#[derive(Default, Clone)]
struct TraceRecord {
    left: Piece,
    right: Piece,
    cost: MC,
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
        test_xp = calc_xp(level)
    }
    level
}

fn calc_penalty(work_count: MC) -> MC {
    (1 << work_count) - 1
}

fn anvil(books_free: bool, left: &Piece, right: &Piece) -> (Piece, MC) {
    let new_type = match (&left.ptype, &right.ptype) {
        (PieceType::Book, PieceType::Book) => PieceType::Book,
        _ => PieceType::Item,
    };
    if books_free && matches!(new_type, PieceType::Book) {
        return (Piece {
            name_mask: left.name_mask | right.name_mask,
            value: left.value + right.value,
            work_count: 0,
            extra_cost: left.extra_cost + right.extra_cost,
            ptype: new_type,
        }, 0);
    }
    let cost = calc_xp(MC::from(right.value) + calc_penalty(MC::from(left.work_count)) +
        calc_penalty(MC::from(right.work_count)) + MC::from(left.extra_cost) + MC::from(right.extra_cost));
    (Piece {
        name_mask: left.name_mask | right.name_mask,
        value: left.value + right.value,
        work_count: cmp::max(left.work_count, right.work_count) + 1,
        extra_cost: left.extra_cost + right.extra_cost,
        ptype: new_type,
    }, cost)
}

fn solve(permutations: &HashMap<usize, Vec<Vec<usize>>>, books_free: bool, queue: &[Piece], total_cost: MC, trace: &[TraceRecord]) -> (MC, Box<[TraceRecord]>) {
    let mut best_trace: Option<Box<[TraceRecord]>> = None;
    let mut best_cost = 4_294_967_295;
    for order in permutations.get(&queue.len()).expect("need to precompute more permuations") {
        let left = &queue[order[0]];
        let right = &queue[order[1]];
        if matches!(right.ptype, PieceType::Item) {
            continue;
        }
        let (combined, cost) = anvil(books_free, left, right);
        if total_cost + cost > best_cost {
            continue;
        }
        let new_queue: TinyVec<[Piece; MS]> = (if order[0] < order[1] {
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
        }).collect();
        if new_queue.len() > 1 {
            let new_trace: TinyVec<[TraceRecord; MS]> = trace.iter()
                .cloned()
                .chain(std::iter::once(TraceRecord {
                    left: left.clone(),
                    right: right.clone(),
                    cost,
                })).collect();
            let (result_cost, result_trace) = solve(permutations, books_free, &new_queue, total_cost + cost, &new_trace);
            if best_trace.is_none() || result_cost < best_cost {
                best_trace = Some(result_trace);
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
                        cost,
                    })).collect::<Vec<TraceRecord>>()));
                best_cost = result_cost;
            }
        }
    }
    (best_cost, best_trace.unwrap_or_else(|| panic!("trace is null")))
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
        .map(|item| (item, PieceType::Item));
    let book_iter = config.input.books.iter()
        .map(|item| (item, PieceType::Book));
    for (i, (piece, ptype)) in item_iter.chain(book_iter).enumerate() {
        let (name, value, work_count, extra_cost) = piece.clone();
        names.push(name);
        pieces.push(Piece {
            name_mask: 1 << i,
            value,
            work_count,
            extra_cost,
            ptype,
        })
    }
    let mut permutations = HashMap::new();
    for ceil in 2..=MS {
        permutations.insert(ceil, (0..ceil).permutations(2)
            .map(|v| v.iter().copied().collect_vec())
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
        let xp_cost = best_order[i].cost;
        let level_cost = calc_level(xp_cost);
        total_level_cost += level_cost;
        if xp_cost > max_xp_cost {
            max_xp_cost = xp_cost
        }
        println!("{}. [{}: {} {}] + [{}: {} {}] = {} ({}xp)", i + 1, get_name(&names, left.name_mask), left.value, left.extra_cost,
                 get_name(&names, right.name_mask), right.value, right.extra_cost,
                 level_cost, xp_cost);
    }
    println!("Max step cost: {} ({max_xp_cost}xp)", calc_level(max_xp_cost));
    println!("Final best cost: {} ({best_cost}xp)", calc_level(best_cost));
    println!("Final worst cost: {total_level_cost} ({}xp)", calc_xp(total_level_cost));
}
