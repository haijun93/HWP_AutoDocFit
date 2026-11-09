//! Small, allocation-conscious document aggregation helpers for hierarchy search.
//! Tantivy supplies paragraph candidates; this module keeps ranking policy in Rust.

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

/// Candidate rows already restricted by SQL to the winning document set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RankedDocument {
    pub document: usize,
    pub total_matches: usize,
    pub best_rank: u8,
    pub best_score: f64,
    pub document_date: f64,
}

/// Preserve the established Python `hierarchy_relevance` ordering.
pub fn rank_documents(rows: &mut [RankedDocument], limit: usize) {
    rows.sort_by(|a, b| {
        a.best_rank.cmp(&b.best_rank)
            .then_with(|| b.total_matches.cmp(&a.total_matches))
            .then_with(|| (a.document_date <= 0.0).cmp(&(b.document_date <= 0.0)))
            .then_with(|| b.document_date.total_cmp(&a.document_date))
            .then_with(|| a.best_score.total_cmp(&b.best_score))
            .then_with(|| a.document.cmp(&b.document))
    });
    rows.truncate(limit);
}

/// Aggregate paragraph hits by document and apply the stable hierarchy search order.
/// Rank 0 is ordinary body text; smaller positive ranks are higher in the outline.
pub fn aggregate(candidates: &[Candidate], limit: usize) -> Vec<DocumentAggregate> {
    let mut out: Vec<DocumentAggregate> = Vec::new();
    for candidate in candidates {
        if let Some(row) = out.iter_mut().find(|row| row.document == candidate.document) {
            row.mentions += 1;
            row.best_rank = best_rank(row.best_rank, candidate.hierarchy_rank);
            row.newest = row.newest.max(candidate.recency);
            row.best_score = row.best_score.max(candidate.score);
        } else {
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
    fn preserves_existing_hierarchy_score_order() {
        let mut rows = [
            RankedDocument { document: 0, total_matches: 100, best_rank: 2, best_score: -0.8, document_date: 1.0 },
            RankedDocument { document: 1, total_matches: 2, best_rank: 1, best_score: -0.1, document_date: 1.0 },
            RankedDocument { document: 2, total_matches: 2, best_rank: 1, best_score: -0.5, document_date: 0.0 },
        ];
        rank_documents(&mut rows, 2);
        assert_eq!(rows.iter().map(|r| r.document).collect::<Vec<_>>(), [2, 1]);
    }
}
