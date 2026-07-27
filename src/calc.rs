use std::cmp;
use std::mem;

use serde::{Deserialize, Serialize};

type WorkCount = u8;
type Value = u32;
type Cost = u64;

const PIECE_TYPE_BOOK: bool = false;
const PIECE_TYPE_ITEM: bool = true;
const SATURATED_WORK_COUNT: WorkCount = 64;
const MAX_DP_WORK_STATES: usize = 1 << WorkCount::BITS;
const MAX_XP_LOOKUP_ENTRIES: usize = 1_000_000;

#[derive(Default, Debug, Clone)]
struct Piece {
    name_indices: Vec<usize>,
    is_item: bool,
    value: Value,
    work_count: WorkCount,
}

#[derive(Default, Debug, Clone)]
struct TraceRecord {
    left: Piece,
    right: Piece,
}

#[derive(Default, Debug, Clone, Copy)]
struct DpEntry {
    work_count: WorkCount,
    cost: Cost,
    left_state: usize,
    left_work_count: WorkCount,
    right_work_count: WorkCount,
}

#[derive(Default, Debug, Clone, Copy)]
struct DpRange {
    start: usize,
    len: usize,
}

#[derive(Debug)]
struct PieceGroup {
    value: Value,
    work_count: WorkCount,
    is_item: bool,
    members: Vec<Piece>,
    stride: usize,
}

#[inline]
fn dp_entries(arena: &[DpEntry], range: DpRange) -> &[DpEntry] {
    &arena[range.start..range.start + range.len]
}

#[derive(Debug)]
struct XpLookup {
    costs: Vec<Cost>,
    work_state_count: usize,
}

impl XpLookup {
    #[inline]
    fn get(&self, value: Value, left_work: WorkCount, right_work: WorkCount) -> Cost {
        let value = usize::try_from(value).expect("piece value exceeds addressable memory");
        let index = (value * self.work_state_count + usize::from(left_work))
            * self.work_state_count
            + usize::from(right_work);
        self.costs[index]
    }
}

#[inline]
const fn calc_xp(level: Cost) -> Cost {
    if level < 16 {
        level.pow(2) + 6 * level
    } else if level < 32 {
        (5 * level.pow(2) + 720 - 81 * level) / 2
    } else {
        (9 * level.pow(2) + 4_440 - 325 * level) / 2
    }
}

const fn calc_level(xp: Cost) -> Cost {
    let mut test_xp = 0;
    let mut level = 0;
    while test_xp < xp {
        level += 1;
        test_xp = calc_xp(level);
    }
    level
}

const fn calc_penalty(work_count: WorkCount) -> Cost {
    if work_count >= SATURATED_WORK_COUNT {
        Cost::MAX
    } else {
        (1_u64 << work_count) - 1
    }
}

const fn calc_work_count(penalty: WorkCount) -> WorkCount {
    let mut test_penalty = 0_u64;
    let mut work_count = 0;
    while test_penalty < penalty as Cost {
        work_count += 1;
        test_penalty = calc_penalty(work_count);
    }
    work_count
}

fn anvil(config: &Config, left: &Piece, right: &Piece) -> (Piece, Cost) {
    let mut name_indices = Vec::with_capacity(left.name_indices.len() + right.name_indices.len());
    name_indices.extend_from_slice(&left.name_indices);
    name_indices.extend_from_slice(&right.name_indices);
    let is_item = left.is_item || right.is_item;
    if config.books_free && !is_item {
        return (
            Piece {
                name_indices,
                is_item,
                value: left.value + right.value,
                work_count: 0,
            },
            0,
        );
    }

    let level_cost = Cost::from(right.value)
        .saturating_add(calc_penalty(left.work_count))
        .saturating_add(calc_penalty(right.work_count));
    let cost = if config.optimize_per_step {
        calc_xp(level_cost)
    } else {
        level_cost
    };
    (
        Piece {
            name_indices,
            is_item,
            value: left.value + right.value,
            work_count: cmp::max(left.work_count, right.work_count).saturating_add(1),
        },
        cost,
    )
}

fn orient_split(
    mut left: usize,
    mut right: usize,
    state_values: &[Value],
    state_has_item: &[u8],
) -> (usize, usize, Value, bool) {
    let left_is_item = state_has_item[left] != 0;
    let right_is_item = state_has_item[right] != 0;
    let both_books = !left_is_item && !right_is_item;
    let (swap, right_value) = if left_is_item == right_is_item {
        let left_value = state_values[left];
        let candidate_right_value = state_values[right];
        let swap = left_value < candidate_right_value;
        (
            swap,
            if swap {
                left_value
            } else {
                candidate_right_value
            },
        )
    } else {
        let swap = !left_is_item;
        (swap, state_values[if swap { left } else { right }])
    };
    if swap {
        mem::swap(&mut left, &mut right);
    }
    (left, right, right_value, both_books)
}

fn group_pieces(pieces: &[Piece]) -> (Vec<PieceGroup>, usize) {
    let mut groups: Vec<PieceGroup> = Vec::new();
    for piece in pieces {
        let is_item = piece.is_item;
        if let Some(group) = groups.iter_mut().find(|group| {
            group.value == piece.value
                && group.work_count == piece.work_count
                && group.is_item == is_item
        }) {
            group.members.push(piece.clone());
        } else {
            groups.push(PieceGroup {
                value: piece.value,
                work_count: piece.work_count,
                is_item,
                members: vec![piece.clone()],
                stride: 0,
            });
        }
    }

    // A state is a mixed-radix integer. Its digit for each group is the number
    // of interchangeable members present, so equal pieces never become
    // separately labelled DP dimensions.
    let mut state_count = 1;
    for group in &mut groups {
        group.stride = state_count;
        state_count = state_count
            .checked_mul(group.members.len() + 1)
            .expect("grouped DP state count exceeds addressable memory");
    }
    (groups, state_count)
}

fn build_state_metadata(groups: &[PieceGroup], state_count: usize) -> (Vec<Value>, Vec<u8>) {
    let mut state_values = vec![0; state_count];
    let mut state_has_item = vec![0; state_count];

    for state in 1..state_count {
        let group = groups
            .iter()
            .find(|group| (state / group.stride) % (group.members.len() + 1) != 0)
            .expect("nonempty grouped state has no pieces");
        let remaining = state - group.stride;
        state_values[state] = state_values[remaining] + group.value;
        state_has_item[state] = state_has_item[remaining] | u8::from(group.is_item);
    }
    (state_values, state_has_item)
}

fn next_substate(
    state: &mut usize,
    counts: &mut [usize],
    maximum_counts: &[usize],
    groups: &[PieceGroup],
) -> bool {
    for (index, group) in groups.iter().enumerate() {
        if counts[index] < maximum_counts[index] {
            counts[index] += 1;
            *state += group.stride;
            return true;
        }
        *state -= counts[index] * group.stride;
        counts[index] = 0;
    }
    false
}

fn build_xp_lookup(max_value: Value, work_state_count: usize) -> Option<XpLookup> {
    let value_count = usize::try_from(max_value).ok()?.checked_add(1)?;
    let entry_count = value_count
        .checked_mul(work_state_count)?
        .checked_mul(work_state_count)?;
    if entry_count > MAX_XP_LOOKUP_ENTRIES {
        return None;
    }

    let penalties = (0..work_state_count)
        .map(|work| {
            calc_penalty(WorkCount::try_from(work).expect("work count exceeds supported size"))
        })
        .collect::<Vec<_>>();
    let mut costs = Vec::with_capacity(entry_count);
    for value in 0..value_count {
        for &left_penalty in &penalties {
            for &right_penalty in &penalties {
                costs.push(calc_xp(value as Cost + left_penalty + right_penalty));
            }
        }
    }
    Some(XpLookup {
        costs,
        work_state_count,
    })
}

struct TraceReconstructor<'a> {
    config: &'a Config,
    groups: &'a [PieceGroup],
    used_members: Vec<usize>,
    dp: &'a [DpRange],
    arena: &'a [DpEntry],
    trace: Vec<TraceRecord>,
}

impl TraceReconstructor<'_> {
    fn reconstruct(&mut self, state: usize, work_count: WorkCount) -> Piece {
        let entry = dp_entries(self.arena, self.dp[state])
            .iter()
            .find(|entry| entry.work_count == work_count)
            .expect("missing grouped-DP reconstruction entry");
        if entry.left_state == 0 {
            // Group members are mechanically interchangeable. Assign their
            // concrete names only when replaying the selected merge tree.
            let group_index = self
                .groups
                .iter()
                .position(|group| group.stride == state)
                .expect("grouped-DP leaf does not identify a piece group");
            let member_index = self.used_members[group_index];
            self.used_members[group_index] += 1;
            return self.groups[group_index].members[member_index].clone();
        }

        let left_state = entry.left_state;
        let right_state = state - left_state;
        let left = self.reconstruct(left_state, entry.left_work_count);
        let right = self.reconstruct(right_state, entry.right_work_count);
        let combined = anvil(self.config, &left, &right).0;
        self.trace.push(TraceRecord { left, right });
        combined
    }
}

#[allow(clippy::too_many_lines)]
fn solve(config: &Config, pieces: &[Piece]) -> Option<(Cost, Box<[TraceRecord]>)> {
    if pieces.is_empty() {
        return None;
    }

    let (groups, state_count) = group_pieces(pieces);
    let (state_values, state_has_item) = build_state_metadata(&groups, state_count);
    let work_state_count = pieces
        .iter()
        .map(|piece| usize::from(piece.work_count))
        .max()
        .unwrap_or(0)
        .saturating_add(pieces.len())
        .min(MAX_DP_WORK_STATES);
    let xp_lookup = config
        .optimize_per_step
        .then(|| build_xp_lookup(state_values[state_count - 1], work_state_count))
        .flatten();
    let penalties = std::array::from_fn::<_, MAX_DP_WORK_STATES, _>(|work| {
        calc_penalty(WorkCount::try_from(work).expect("work count exceeds supported size"))
    });

    let mut dp = vec![DpRange::default(); state_count];
    let mut arena = Vec::with_capacity(state_count);
    for group in &groups {
        let start = arena.len();
        arena.push(DpEntry {
            work_count: group.work_count,
            cost: 0,
            left_state: 0,
            left_work_count: 0,
            right_work_count: 0,
        });
        dp[group.stride] = DpRange { start, len: 1 };
    }

    let all_groups_unique = groups.iter().all(|group| group.members.len() == 1);
    let mut maximum_counts = vec![0; groups.len()];
    let mut candidate_counts = vec![0; groups.len()];
    for state in 1..state_count {
        // Leaf states were initialized above. Every proper substate has a
        // smaller mixed-radix encoding, so increasing numeric order satisfies
        // all remaining DP dependencies.
        if dp[state].len != 0 {
            continue;
        }

        if !all_groups_unique {
            for (index, group) in groups.iter().enumerate() {
                maximum_counts[index] = (state / group.stride) % (group.members.len() + 1);
                candidate_counts[index] = 0;
            }
        }

        let mut best_by_work: [Option<DpEntry>; MAX_DP_WORK_STATES] = [None; MAX_DP_WORK_STATES];
        let mut candidate_right = 0usize;
        loop {
            if all_groups_unique {
                // For singleton groups, the mixed-radix state is a bitset.
                // This advances to the next submask in ascending order.
                candidate_right = candidate_right.wrapping_sub(state) & state;
            } else if !next_substate(
                &mut candidate_right,
                &mut candidate_counts,
                &maximum_counts,
                &groups,
            ) {
                break;
            }

            // Candidates are increasing, so this visits exactly one side
            // of every unordered split and then terminates.
            if candidate_right > state / 2 {
                break;
            }
            let candidate_left = state - candidate_right;
            let (left_state, right_state, right_value, both_books) = orient_split(
                candidate_left,
                candidate_right,
                &state_values,
                &state_has_item,
            );
            let left_index = left_state;
            let right_index = right_state;
            let books_are_free = config.books_free && both_books;

            for left_entry in dp_entries(&arena, dp[left_index]) {
                for right_entry in dp_entries(&arena, dp[right_index]) {
                    let (work_count, merge_cost) = if books_are_free {
                        (0, 0)
                    } else {
                        let work_count = cmp::max(left_entry.work_count, right_entry.work_count)
                            .saturating_add(1);
                        let level_cost = Cost::from(right_value)
                            .saturating_add(penalties[usize::from(left_entry.work_count)])
                            .saturating_add(penalties[usize::from(right_entry.work_count)]);
                        let merge_cost = if config.optimize_per_step {
                            xp_lookup.as_ref().map_or_else(
                                || calc_xp(level_cost),
                                |lookup| {
                                    lookup.get(
                                        right_value,
                                        left_entry.work_count,
                                        right_entry.work_count,
                                    )
                                },
                            )
                        } else {
                            level_cost
                        };
                        (work_count, merge_cost)
                    };
                    let total_cost = left_entry
                        .cost
                        .saturating_add(right_entry.cost)
                        .saturating_add(merge_cost);
                    let slot = &mut best_by_work[usize::from(work_count)];
                    if slot.is_none_or(|entry| total_cost < entry.cost) {
                        *slot = Some(DpEntry {
                            work_count,
                            cost: total_cost,
                            left_state,
                            left_work_count: left_entry.work_count,
                            right_work_count: right_entry.work_count,
                        });
                    }
                }
            }
        }

        let start = arena.len();
        let mut len = 0;
        let mut cheapest_lower_work = Cost::MAX;
        for entry in best_by_work.into_iter().flatten() {
            if entry.cost < cheapest_lower_work {
                cheapest_lower_work = entry.cost;
                arena.push(entry);
                len += 1;
            }
        }
        dp[state] = DpRange { start, len };
    }

    let full_state = state_count - 1;
    let best_entry = *dp_entries(&arena, dp[full_state])
        .iter()
        .min_by_key(|entry| entry.cost)?;
    let mut reconstructor = TraceReconstructor {
        config,
        groups: &groups,
        used_members: vec![0; groups.len()],
        dp: &dp,
        arena: &arena,
        trace: Vec::with_capacity(pieces.len() - 1),
    };
    reconstructor.reconstruct(full_state, best_entry.work_count);
    Some((best_entry.cost, reconstructor.trace.into_boxed_slice()))
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    books_free: bool,
    optimize_per_step: bool,
}

type InputPiece = (String, String, WorkCount);

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

fn get_name(names: &[String], name_indices: &[usize]) -> String {
    name_indices
        .iter()
        .map(|&index| names[index].as_str())
        .collect::<Vec<_>>()
        .join(" + ")
}

const fn expand_cost(config: &Config, cost: Cost) -> (Cost, Cost) {
    if config.optimize_per_step {
        (calc_level(cost), cost)
    } else {
        (cost, calc_xp(cost))
    }
}

pub fn process(schema: ConfigSchema) -> String {
    let (input, config) = (schema.input, schema.config);

    let mut pieces = Vec::new();
    let mut names = Vec::new();
    let item_iter = input.items.iter().map(|item| (item, PIECE_TYPE_ITEM));
    let book_iter = input.books.iter().map(|item| (item, PIECE_TYPE_BOOK));
    for (i, (piece, piece_type)) in item_iter.chain(book_iter).enumerate() {
        let (name, level_multiplier, penalty) = piece.clone();
        names.push(name);
        pieces.push(Piece {
            name_indices: vec![i],
            is_item: piece_type,
            value: level_multiplier
                .split('x')
                .map(|component| component.trim().parse::<Value>().unwrap())
                .product(),
            work_count: calc_work_count(penalty),
        });
    }

    let Some((best_cost, order)) = solve(&config, &pieces) else {
        return String::from("No inputs, calculation not possible.\n");
    };
    let mut max_xp_cost = 0;
    let mut total_level_cost = 0;
    let mut separately_funded_xp_cost = 0;
    let mut result = String::new();
    for (index, record) in order.iter().enumerate() {
        let left = &record.left;
        let right = &record.right;
        let (_, cost) = anvil(&config, left, right);
        let (level_cost, xp_cost) = expand_cost(&config, cost);
        total_level_cost += level_cost;
        separately_funded_xp_cost += xp_cost;
        max_xp_cost = cmp::max(max_xp_cost, xp_cost);
        result += format!(
            "{}. [{}: {},{}] + [{}: {},{}] = {} levels ({} XP)\n",
            index + 1,
            get_name(&names, &left.name_indices),
            left.value,
            calc_penalty(left.work_count),
            get_name(&names, &right.name_indices),
            right.value,
            calc_penalty(right.work_count),
            level_cost,
            xp_cost
        )
        .as_str();
    }
    result += "\n";
    result += format!(
        "Max step cost: {} levels ({max_xp_cost} XP)\n",
        calc_level(max_xp_cost)
    )
    .as_str();
    let upfront_xp_cost = calc_xp(total_level_cost);
    let objective = if config.optimize_per_step {
        debug_assert_eq!(best_cost, separately_funded_xp_cost);
        "exact levels for each step"
    } else {
        debug_assert_eq!(best_cost, total_level_cost);
        "all levels at once"
    };
    result += format!("Optimized for: {objective}\n").as_str();
    result += format!("Total anvil cost: {total_level_cost} levels\n").as_str();
    result += format!("XP if all levels at once: {upfront_xp_cost}\n").as_str();
    result += format!("XP if exact levels for each step: {separately_funded_xp_cost}\n")
        .as_str();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(index: usize, is_item: bool, value: Value, work_count: WorkCount) -> Piece {
        Piece {
            name_indices: vec![index],
            is_item,
            value,
            work_count,
        }
    }

    fn brute_force(config: &Config, pieces: &[Piece]) -> Cost {
        if pieces.len() == 1 {
            return 0;
        }

        let mut best = Cost::MAX;
        for first in 0..pieces.len() {
            for second in first + 1..pieces.len() {
                let mut left = pieces[first].clone();
                let mut right = pieces[second].clone();
                let same_type = left.is_item == right.is_item;
                let swap = if same_type {
                    left.value < right.value
                } else {
                    !left.is_item
                };
                if swap {
                    mem::swap(&mut left, &mut right);
                }

                let (combined, merge_cost) = anvil(config, &left, &right);
                let mut next = Vec::with_capacity(pieces.len() - 1);
                next.extend(
                    pieces
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| *index != first && *index != second)
                        .map(|(_, piece)| piece.clone()),
                );
                next.push(combined);
                best = cmp::min(best, merge_cost.saturating_add(brute_force(config, &next)));
            }
        }
        best
    }

    fn next_random(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        *state
    }

    #[test]
    fn grouped_dp_matches_brute_force() {
        let mut random_state = 0xA11C_E5EED;
        for case in 0..24 {
            let piece_count = 2 + usize::try_from(next_random(&mut random_state) % 6).unwrap();
            let mut pieces = Vec::with_capacity(piece_count);
            for index in 0..piece_count {
                let is_item = index == 0 || next_random(&mut random_state) % 5 == 0;
                let piece_type = if is_item {
                    PIECE_TYPE_ITEM
                } else {
                    PIECE_TYPE_BOOK
                };
                pieces.push(piece(
                    index,
                    piece_type,
                    Value::try_from(next_random(&mut random_state) % 13).unwrap(),
                    WorkCount::try_from(next_random(&mut random_state) % 3).unwrap(),
                ));
            }

            for books_free in [false, true] {
                for optimize_per_step in [false, true] {
                    let config = Config {
                        books_free,
                        optimize_per_step,
                    };
                    let expected = brute_force(&config, &pieces);
                    let (actual, trace) = solve(&config, &pieces).unwrap();
                    let trace_cost = trace
                        .iter()
                        .map(|record| anvil(&config, &record.left, &record.right).1)
                        .sum::<Cost>();
                    assert_eq!(actual, expected, "random case {case}, {config:?}");
                    assert_eq!(trace_cost, actual, "random case {case}, {config:?}");
                    assert_eq!(trace.len(), piece_count - 1);
                }
            }
        }
    }

    #[test]
    fn grouped_dp_collapses_and_solves_the_21_piece_workload() {
        let values = [
            0, 12, 12, 4, 6, 4, 3, 2, 12, 8, 8, 4, 4, 4, 4, 6, 2, 5, 6, 4, 6,
        ];
        let pieces = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| piece(index, index == 0, value, 0))
            .collect::<Vec<_>>();
        let config = Config {
            books_free: false,
            optimize_per_step: true,
        };

        let (groups, state_count) = group_pieces(&pieces);
        assert_eq!(groups.len(), 8);
        assert_eq!(state_count, 11_520);

        let (cost, trace) = solve(&config, &pieces).unwrap();
        assert_eq!(cost, 12_415);
        assert_eq!(trace.len(), pieces.len() - 1);
        let final_record = trace.last().unwrap();
        let mut final_names = final_record.left.name_indices.clone();
        final_names.extend_from_slice(&final_record.right.name_indices);
        final_names.sort_unstable();
        assert_eq!(final_names, (0..pieces.len()).collect::<Vec<_>>());
    }

    #[test]
    fn grouped_dp_supports_piece_counts_beyond_fixed_width_encodings() {
        let pieces = (0..300)
            .map(|index| piece(index, index == 0, u32::from(index != 0), 0))
            .collect::<Vec<_>>();
        let config = Config {
            books_free: false,
            optimize_per_step: false,
        };

        let (groups, state_count) = group_pieces(&pieces);
        assert_eq!(groups.len(), 2);
        assert_eq!(state_count, 600);

        let (_, trace) = solve(&config, &pieces).unwrap();
        assert_eq!(trace.len(), pieces.len() - 1);
        let final_record = trace.last().unwrap();
        let mut final_names = final_record.left.name_indices.clone();
        final_names.extend_from_slice(&final_record.right.name_indices);
        final_names.sort_unstable();
        assert_eq!(final_names, (0..pieces.len()).collect::<Vec<_>>());
    }

    #[test]
    fn public_api_accepts_more_than_21_pieces() {
        let schema = ConfigSchema {
            config: Config {
                books_free: false,
                optimize_per_step: false,
            },
            input: Input {
                items: vec![(String::from("item"), String::from("0x0"), 0)],
                books: (0..21)
                    .map(|index| (format!("book {index}"), String::from("1x1"), 0))
                    .collect(),
            },
        };

        let result = process(schema);
        assert!(result.contains("Total anvil cost:"));
        assert!(result.contains("book 20"));
    }

    #[test]
    fn xp_point_optimization_can_choose_a_different_order() {
        let pieces = [
            piece(0, PIECE_TYPE_ITEM, 0, 0),
            piece(1, PIECE_TYPE_BOOK, 1, 0),
            piece(2, PIECE_TYPE_BOOK, 4, 0),
            piece(3, PIECE_TYPE_BOOK, 4, 0),
        ];
        let levels_config = Config {
            books_free: false,
            optimize_per_step: false,
        };
        let xp_points_config = Config {
            books_free: false,
            optimize_per_step: true,
        };

        let (level_objective, level_trace) = solve(&levels_config, &pieces).unwrap();
        let (xp_objective, xp_trace) = solve(&xp_points_config, &pieces).unwrap();
        let trace_costs = |trace: &[TraceRecord]| {
            trace
                .iter()
                .map(|record| anvil(&levels_config, &record.left, &record.right).1)
                .fold((0, 0), |(levels, xp_points), level_cost| {
                    (levels + level_cost, xp_points + calc_xp(level_cost))
                })
        };

        assert_eq!(trace_costs(&level_trace), (12, 138));
        assert_eq!(trace_costs(&xp_trace), (13, 135));
        assert_eq!(level_objective, 12);
        assert_eq!(xp_objective, 135);
    }
}
