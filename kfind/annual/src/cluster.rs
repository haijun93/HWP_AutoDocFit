//! 주제 묶기 모듈 (Union-Find, 대표 문서 및 2글자 핵심어 추출).
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::sim::{DocFeatures, calculate_weighted_similarity};

/// 분리 집합 (Union-Find) 자료구조.
pub struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    pub fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    pub fn find(&mut self, mut x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        // 경로 압축 (Path compression)
        while self.parent[x] != root {
            let next = self.parent[x];
            self.parent[x] = root;
            x = next;
        }
        root
    }

    pub fn union(&mut self, x: usize, y: usize) -> bool {
        let root_x = self.find(x);
        let root_y = self.find(y);
        if root_x == root_y {
            return false;
        }
        if self.rank[root_x] < self.rank[root_y] {
            self.parent[root_x] = root_y;
        } else if self.rank[root_x] > self.rank[root_y] {
            self.parent[root_y] = root_x;
        } else {
            self.parent[root_y] = root_x;
            self.rank[root_x] += 1;
        }
        true
    }
}

/// 묶인 주제(클러스터) 정보.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Topic {
    /// 주제 식별 번호 (0부터 시작).
    pub id: usize,
    /// 주제 표시 이름 (대표 제목 + 핵심어 2~3개).
    pub name: String,
    /// 가장 대표 문서의 정규화 제목.
    pub rep_title: String,
    /// 대표 문서 인덱스.
    pub rep_doc_index: usize,
    /// 자주 나온 2글자 조각 기반 핵심어 (2~3개).
    pub keywords: Vec<String>,
    /// 주제에 포함된 문서 인덱스 목록.
    pub doc_indices: Vec<usize>,
}

/// 유사도 행렬을 바탕으로 문서를 주제로 묶는다.
pub fn cluster_topics(
    doc_features: &[DocFeatures],
    doc_titles: &[String],
    doc_name_norms: &[String],
    candidate_pairs: &[(usize, usize)],
    threshold: f64,
) -> Vec<Topic> {
    let n = doc_features.len();
    if n == 0 {
        return Vec::new();
    }

    let mut uf = UnionFind::new(n);

    // 후보 쌍 간의 가중 유사도를 계산하여 threshold 이상인 경우 union
    // 빠른 조회를 위해 계산된 유사도를 캐싱
    let mut sim_cache: HashMap<(usize, usize), f64> = HashMap::new();

    for &(i, j) in candidate_pairs {
        if i >= n || j >= n {
            continue;
        }
        let sim = calculate_weighted_similarity(&doc_features[i], &doc_features[j]);
        let pair = if i < j { (i, j) } else { (j, i) };
        sim_cache.insert(pair, sim);

        if sim >= threshold {
            uf.union(i, j);
        }
    }

    // 각 루트별로 클러스터 문서 목록 모으기
    let mut clusters: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let root = uf.find(i);
        clusters.entry(root).or_default().push(i);
    }

    let mut topics = Vec::new();

    // 클러스터들을 크기(문서 수) 내림차순으로 정렬
    let mut cluster_list: Vec<Vec<usize>> = clusters.into_values().collect();
    cluster_list.sort_by(|a, b| b.len().cmp(&a.len()));

    for (topic_id, indices) in cluster_list.into_iter().enumerate() {
        // 대표 문서 선정: 클러스터 내 평균 유사도 최대인 문서
        let (rep_index, _avg_sim) = if indices.len() == 1 {
            (indices[0], 1.0)
        } else {
            let mut best_doc = indices[0];
            let mut max_avg_sim = -1.0;

            for &i in &indices {
                let mut sum_sim = 0.0;
                let mut count = 0;
                for &j in &indices {
                    if i == j {
                        continue;
                    }
                    let pair = if i < j { (i, j) } else { (j, i) };
                    let sim = sim_cache.get(&pair).copied().unwrap_or_else(|| {
                        calculate_weighted_similarity(&doc_features[i], &doc_features[j])
                    });
                    sum_sim += sim;
                    count += 1;
                }
                let avg = if count > 0 { sum_sim / count as f64 } else { 0.0 };
                if avg > max_avg_sim {
                    max_avg_sim = avg;
                    best_doc = i;
                }
            }
            (best_doc, max_avg_sim)
        };

        // 대표 문서의 정규화 제목
        let rep_title = if !doc_titles[rep_index].trim().is_empty() {
            doc_titles[rep_index].trim().to_string()
        } else {
            doc_name_norms[rep_index].trim().to_string()
        };

        // 클러스터 내 2글자 조각 빈도 분석 -> 핵심어 2~3개 추출
        let mut bigram_freq: HashMap<String, usize> = HashMap::new();
        for &idx in &indices {
            for bg in &doc_features[idx].all_bigrams {
                *bigram_freq.entry(bg.clone()).or_insert(0) += 1;
            }
        }

        // 빈도 높은 순으로 정렬
        let mut sorted_bigrams: Vec<(String, usize)> = bigram_freq.into_iter().collect();
        sorted_bigrams.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut keywords = Vec::new();
        for (bg, _count) in sorted_bigrams {
            if keywords.len() >= 3 {
                break;
            }
            // 너무 일반적이거나 공백/기호가 섞인 것 제외 (fold된 것이므로 한글/영숫자)
            if bg.chars().count() == 2 {
                keywords.push(bg);
            }
        }

        // 주제 이름 구성: "대표제목 (키워드1, 키워드2)"
        let topic_name = if !keywords.is_empty() {
            format!("{} ({})", rep_title, keywords.join(", "))
        } else {
            rep_title.clone()
        };

        topics.push(Topic {
            id: topic_id,
            name: topic_name,
            rep_title,
            rep_doc_index: rep_index,
            keywords,
            doc_indices: indices,
        });
    }

    topics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_union_find() {
        let mut uf = UnionFind::new(5);
        assert_eq!(uf.find(0), 0);
        uf.union(0, 1);
        uf.union(1, 2);
        assert_eq!(uf.find(0), uf.find(2));
        assert_ne!(uf.find(0), uf.find(3));
    }

    #[test]
    fn test_cluster_topics() {
        let titles = vec![
            "2026 예산 편성 계획".to_string(),
            "2026 예산 집행 계획".to_string(),
            "청사 방호 보안 지침".to_string(),
            "청사 출입 보안 수칙".to_string(),
        ];
        let name_norms = vec![
            "예산 편성 계획".to_string(),
            "예산 집행 계획".to_string(),
            "청사 방호 지침".to_string(),
            "청사 출입 수칙".to_string(),
        ];
        let features: Vec<DocFeatures> = (0..4)
            .map(|i| DocFeatures::from_doc(&name_norms[i], &titles[i], &[]))
            .collect();

        let pairs = vec![(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
        let topics = cluster_topics(&features, &titles, &name_norms, &pairs, 0.45);

        // 2개의 주제로 묶여야 함 (예산 관련 0, 1 / 청사 보안 관련 2, 3)
        assert_eq!(topics.len(), 2);
        assert_eq!(topics[0].doc_indices.len(), 2);
        assert_eq!(topics[1].doc_indices.len(), 2);
    }
}
