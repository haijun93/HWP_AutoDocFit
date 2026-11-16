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
    // 위계 우선(사용자 결정, 2026-11-10): Python 위계검색(`hierarchy_relevance`)과 같은 순서로
    // 위계 수준 → 언급 수 → 최신 → 점수. 본문(rank 0)은 모든 위계 수준 뒤에 온다.
    out.sort_by(|a, b| {
        rank_key(a.best_rank).cmp(&rank_key(b.best_rank))
            .then_with(|| b.mentions.cmp(&a.mentions))
            .then_with(|| b.newest.cmp(&a.newest))
            .then_with(|| b.best_score.total_cmp(&a.best_score))
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
        // 위계 우선: 수준 1인 문서 0(언급 2회)·2가 먼저, 점수가 가장 높아도 본문뿐인 문서 1은 마지막.
        assert_eq!(rows.iter().map(|r| r.document).collect::<Vec<_>>(), [0, 2, 1]);
        assert_eq!((rows[0].best_rank, rows[0].mentions, rows[0].newest), (1, 2, 200));
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
            // 위계 우선: 수준 1인 문서 42가 점수가 더 높은 수준 2 문서보다 먼저 온다.
            DocumentAggregate { document: 42, best_rank: 1, mentions: 2, newest: 8, best_score: 3.0 },
            DocumentAggregate { document: usize::MAX, best_rank: 2, mentions: 3, newest: 9, best_score: 4.0 },
        ]);
        assert!(aggregate(&hits, 0).is_empty());
        assert!(aggregate(&[], 10).is_empty());
    }

    #[test]
    fn same_file_three_paragraphs_aggregate_to_single_doc_with_three_mentions() {
        let hits = [
            Candidate { document: 10, score: 1.5, hierarchy_rank: 3, recency: 100 },
            Candidate { document: 10, score: 4.0, hierarchy_rank: 1, recency: 300 },
            Candidate { document: 10, score: 2.0, hierarchy_rank: 2, recency: 200 },
        ];
        let rows = aggregate(&hits, 10);
        assert_eq!(rows.len(), 1, "3개 문단 일치 시 단일 문서 1건으로 묶여야 함");
        assert_eq!(rows[0].document, 10);
        assert_eq!(rows[0].mentions, 3, "언급 수가 3으로 합산되어야 함");
        assert_eq!(rows[0].best_rank, 1, "가장 높은 위계 수준(1)이 반영되어야 함");
        assert_eq!(rows[0].best_score, 4.0, "최고 점수가 반영되어야 함");
        assert_eq!(rows[0].newest, 300, "최신 시각이 반영되어야 함");
    }

    /// O(N) 선형 탐색 참조 구현과 HashMap O(1) 조회 구현의 순위·집계 결과 동등성 검증.
    fn reference_linear_aggregate(candidates: &[Candidate], limit: usize) -> Vec<DocumentAggregate> {
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
            rank_key(a.best_rank).cmp(&rank_key(b.best_rank))
                .then_with(|| b.mentions.cmp(&a.mentions))
                .then_with(|| b.newest.cmp(&a.newest))
                .then_with(|| b.best_score.total_cmp(&a.best_score))
                .then_with(|| a.document.cmp(&b.document))
        });
        out.truncate(limit);
        out
    }

    #[test]
    fn aggregate_hashmap_matches_linear_scan_equivalence() {
        // 복합 패턴: 중복 문서, 순서 뒤섞임, 희소 문서 ID, 다양한 rank·score·recency
        let hits = [
            Candidate { document: 4, score: 1.0, hierarchy_rank: 0, recency: 10 },
            Candidate { document: 2, score: 5.5, hierarchy_rank: 2, recency: 20 },
            Candidate { document: 4, score: 3.0, hierarchy_rank: 1, recency: 50 },
            Candidate { document: 99, score: 8.0, hierarchy_rank: 0, recency: 5 },
            Candidate { document: 2, score: 2.0, hierarchy_rank: 0, recency: 80 },
            Candidate { document: 7, score: 4.0, hierarchy_rank: 3, recency: 100 },
            Candidate { document: 4, score: 0.5, hierarchy_rank: 4, recency: 15 },
            Candidate { document: 2, score: 6.0, hierarchy_rank: 1, recency: 30 },
            Candidate { document: 99, score: 1.0, hierarchy_rank: 2, recency: 90 },
            Candidate { document: 500, score: 10.0, hierarchy_rank: 0, recency: 1000 },
        ];

        for limit in [0, 1, 2, 3, 5, 10, 20] {
            let actual = aggregate(&hits, limit);
            let expected = reference_linear_aggregate(&hits, limit);
            assert_eq!(actual, expected, "limit={}에서 HashMap 최적화 결과와 선형 탐색 결과가 일치해야 함", limit);
        }

        // 대량 합성 후보(100건, 15개 고유 문서) 동등성 검증
        let mut large_hits = Vec::new();
        for i in 0..100 {
            large_hits.push(Candidate {
                document: (i * 7) % 15,
                score: (i as f32 * 0.3) % 10.0,
                hierarchy_rank: (i % 5) as u8,
                recency: ((i * 13) % 200) as i64,
            });
        }
        for limit in [1, 5, 15, 50] {
            let actual = aggregate(&large_hits, limit);
            let expected = reference_linear_aggregate(&large_hits, limit);
            assert_eq!(actual, expected, "대량 후보 limit={}에서 결과가 완전히 같아야 함", limit);
        }
    }
}
