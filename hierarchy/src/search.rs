//! Small, allocation-conscious document aggregation helpers for hierarchy search.
//! Tantivy supplies paragraph candidates; this module keeps ranking policy in Rust.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub document: usize,
    pub score: f32,
    pub hierarchy_rank: u8,
    pub recency: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DocumentAggregate {
    pub document: usize,
    pub best_rank: u8,
    pub mentions: usize,
    pub newest: i64,
    pub best_score: f32,
}

/// Aggregate paragraph hits by document and apply the stable hierarchy search order.
/// Rank 0 is ordinary body text; smaller positive ranks are higher in the outline.
pub fn aggregate(candidates: &[Candidate], limit: usize) -> Vec<DocumentAggregate> {
    let mut out: Vec<DocumentAggregate> = Vec::new();
    let mut positions: HashMap<usize, usize> = HashMap::new();
    for candidate in candidates {
        if let Some(&position) = positions.get(&candidate.document) {
            let row = &mut out[position];
            row.mentions += 1;
            row.best_rank = best_rank(row.best_rank, candidate.hierarchy_rank);
            row.newest = row.newest.max(candidate.recency);
            row.best_score = row.best_score.max(candidate.score);
        } else {
            positions.insert(candidate.document, out.len());
            out.push(DocumentAggregate {
                document: candidate.document,
                best_rank: candidate.hierarchy_rank,
                mentions: 1,
                newest: candidate.recency,
                best_score: candidate.score,
            });
        }
    }
    out.sort_by(|a, b| {
        b.best_score.total_cmp(&a.best_score)
            .then_with(|| rank_key(a.best_rank).cmp(&rank_key(b.best_rank)))
            .then_with(|| b.mentions.cmp(&a.mentions))
            .then_with(|| b.newest.cmp(&a.newest))
            .then_with(|| a.document.cmp(&b.document))
    });
    out.truncate(limit);
    out
}

fn best_rank(a: u8, b: u8) -> u8 {
    match (a, b) { (0, x) => x, (x, 0) => x, (x, y) => x.min(y) }
}

fn rank_key(rank: u8) -> u8 { if rank == 0 { u8::MAX } else { rank } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_ranks_then_mentions_date_and_score() {
        let hits = [
            Candidate { document: 0, score: 8.0, hierarchy_rank: 3, recency: 100 },
            Candidate { document: 1, score: 20.0, hierarchy_rank: 0, recency: 999 },
            Candidate { document: 0, score: 4.0, hierarchy_rank: 1, recency: 200 },
            Candidate { document: 2, score: 3.0, hierarchy_rank: 1, recency: 500 },
        ];
        let rows = aggregate(&hits, 20);
        assert_eq!(rows.iter().map(|r| r.document).collect::<Vec<_>>(), [1, 0, 2]);
        assert_eq!((rows[1].best_rank, rows[1].mentions, rows[1].newest), (1, 2, 200));
    }

    #[test]
    fn limits_after_aggregation() {
        let hits = [
            Candidate { document: 7, score: 2.0, hierarchy_rank: 0, recency: 1 },
            Candidate { document: 7, score: 1.0, hierarchy_rank: 0, recency: 1 },
            Candidate { document: 8, score: 3.0, hierarchy_rank: 2, recency: 1 },
        ];
        assert_eq!(aggregate(&hits, 1)[0].document, 8);
    }

    #[test]
    fn aggregates_interleaved_sparse_document_ids() {
        let hits = [
            Candidate { document: usize::MAX, score: 2.0, hierarchy_rank: 0, recency: 9 },
            Candidate { document: 42, score: 2.0, hierarchy_rank: 3, recency: 5 },
            Candidate { document: usize::MAX, score: 1.0, hierarchy_rank: 2, recency: 4 },
            Candidate { document: 42, score: 3.0, hierarchy_rank: 1, recency: 8 },
            Candidate { document: usize::MAX, score: 4.0, hierarchy_rank: 0, recency: 7 },
        ];
        assert_eq!(aggregate(&hits, 10), vec![
            DocumentAggregate { document: usize::MAX, best_rank: 2, mentions: 3, newest: 9, best_score: 4.0 },
            DocumentAggregate { document: 42, best_rank: 1, mentions: 2, newest: 8, best_score: 3.0 },
        ]);
        assert!(aggregate(&hits, 0).is_empty());
        assert!(aggregate(&[], 10).is_empty());
    }
}
