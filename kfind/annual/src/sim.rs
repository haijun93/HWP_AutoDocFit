//! 유사도 계산 및 MinHash(64) + LSH(16밴드×4) 후보 추리기 모듈.
use std::collections::{HashMap, HashSet};

/// 64개의 MinHash를 위한 소수 모듈러 (2^32보다 작은 큰 소수).
const HASH_PRIME: u64 = 4294967311;

/// 64개의 고정된 (a, b) 계수 쌍. a는 0이 아니며 큰 홀수.
const HASH_COEFFS: [(u64, u64); 64] = [
    (1000000007, 1234567), (1000000009, 2345678), (1000000021, 3456789), (1000000033, 4567890),
    (1000000087, 5678901), (1000000093, 6789012), (1000000097, 7890123), (1000000103, 8901234),
    (1000000123, 9012345), (1000000181, 123456),  (1000000207, 234567),  (1000000223, 345678),
    (1000000241, 456789),  (1000000271, 567890),  (1000000289, 678901),  (1000000297, 789012),
    (1000000321, 890123),  (1000000349, 901234),  (1000000363, 112233),  (1000000403, 223344),
    (1000000409, 334455),  (1000000411, 445566),  (1000000427, 556677),  (1000000433, 667788),
    (1000000439, 778899),  (1000000453, 889900),  (1000000591, 990011),  (1000000613, 135791),
    (1000000643, 246802),  (1000000649, 357913),  (1000000663, 468024),  (1000000753, 579135),
    (1000000777, 680246),  (1000000823, 791357),  (1000000829, 802468),  (1000000861, 913579),
    (1000000871, 147258),  (1000000877, 258369),  (1000000889, 369470),  (1000000893, 470581),
    (1000000901, 581692),  (1000000921, 692703),  (1000000933, 703814),  (1000000957, 814925),
    (1000000981, 925036),  (1000000997, 159263),  (1000001011, 260374),  (1000001053, 371485),
    (1000001087, 482596),  (1000001137, 593607),  (1000001149, 604718),  (1000001173, 715829),
    (1000001177, 826930),  (1000001191, 937041),  (1000001211, 160482),  (1000001221, 271593),
    (1000001239, 382604),  (1000001257, 493715),  (1000001283, 504826),  (1000001309, 615937),
    (1000001323, 726048),  (1000001339, 837159),  (1000001351, 948260),  (1000001357, 172839),
];

/// 텍스트에서 비교형(fold) 2글자 조각(bigram) 집합을 추출한다.
pub fn extract_bigrams(text: &str) -> HashSet<String> {
    let folded = kfind_core::fold(text);
    let chars: Vec<char> = folded.chars().collect();
    let mut bigrams = HashSet::new();

    if chars.is_empty() {
        return bigrams;
    }

    if chars.len() == 1 {
        bigrams.insert(chars[0].to_string());
        return bigrams;
    }

    for w in chars.windows(2) {
        let mut s = String::with_capacity(8);
        s.push(w[0]);
        s.push(w[1]);
        bigrams.insert(s);
    }

    bigrams
}

/// 제목 및 중소제목 목록으로부터 2글자 조각 집합들을 합친다.
pub fn extract_headings_bigrams(headings: &[String]) -> HashSet<String> {
    let mut set = HashSet::new();
    for h in headings.iter().take(20) {
        set.extend(extract_bigrams(h));
    }
    set
}

/// 두 조각 집합 간의 자카드(Jaccard) 유사도를 계산한다 (0.0 ~ 1.0).
pub fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let intersection_count = a.iter().filter(|x| b.contains(*x)).count();
    let union_count = a.len() + b.len() - intersection_count;

    if union_count == 0 {
        0.0
    } else {
        intersection_count as f64 / union_count as f64
    }
}

/// 문서 특성 조각 집합들.
#[derive(Clone, Debug)]
pub struct DocFeatures {
    pub name_bigrams: HashSet<String>,
    pub title_bigrams: HashSet<String>,
    pub headings_bigrams: HashSet<String>,
    /// 통합 조각 집합 (MinHash 계산용)
    pub all_bigrams: HashSet<String>,
}

impl DocFeatures {
    pub fn from_doc(name_norm: &str, title: &str, headings: &[String]) -> Self {
        let name_bigrams = extract_bigrams(name_norm);
        let title_bigrams = if title.trim().is_empty() {
            // 제목이 비어있으면 정규화 파일명을 fallback으로 활용
            name_bigrams.clone()
        } else {
            extract_bigrams(title)
        };
        let headings_bigrams = extract_headings_bigrams(headings);

        let mut all_bigrams = HashSet::new();
        all_bigrams.extend(name_bigrams.clone());
        all_bigrams.extend(title_bigrams.clone());
        all_bigrams.extend(headings_bigrams.clone());

        Self {
            name_bigrams,
            title_bigrams,
            headings_bigrams,
            all_bigrams,
        }
    }
}

/// 두 문서 간의 가중 유사도를 계산한다.
/// 가중치: 파일명 0.4 + 제목 0.4 + 중소제목 0.2
pub fn calculate_weighted_similarity(d1: &DocFeatures, d2: &DocFeatures) -> f64 {
    let sim_name = jaccard_similarity(&d1.name_bigrams, &d2.name_bigrams);
    let sim_title = jaccard_similarity(&d1.title_bigrams, &d2.title_bigrams);
    let sim_headings = jaccard_similarity(&d1.headings_bigrams, &d2.headings_bigrams);

    0.4 * sim_name + 0.4 * sim_title + 0.2 * sim_headings
}

/// 64차원 MinHash 서명을 계산한다.
pub fn compute_minhash_64(features: &HashSet<String>) -> [u32; 64] {
    let mut sig = [u32::MAX; 64];

    if features.is_empty() {
        return [0; 64];
    }

    // 각 조각 문자열을 32비트 정수로 해싱 (FNV-1a)
    let token_hashes: Vec<u32> = features
        .iter()
        .map(|token| {
            let mut h: u32 = 0x811c9dc5;
            for b in token.as_bytes() {
                h ^= *b as u32;
                h = h.wrapping_mul(0x01000193);
            }
            h
        })
        .collect();

    for (k, &(a, b)) in HASH_COEFFS.iter().enumerate() {
        let mut min_val = u32::MAX;
        for &th in &token_hashes {
            let x = th as u64;
            let val = ((a.wrapping_mul(x).wrapping_add(b)) % HASH_PRIME) as u32;
            if val < min_val {
                min_val = val;
            }
        }
        sig[k] = min_val;
    }

    sig
}

/// LSH(16밴드 × 4행 = 64차원)를 사용하여 후보 문서 쌍(Candidate Pairs)을 추린다.
pub fn lsh_candidate_pairs(signatures: &[[u32; 64]]) -> HashSet<(usize, usize)> {
    let n = signatures.len();
    let mut candidate_pairs = HashSet::new();

    // 16개 밴드, 밴드당 4개 해시
    const BANDS: usize = 16;
    const ROWS: usize = 4;

    for b in 0..BANDS {
        // (band_id, r0, r1, r2, r3) -> 문서 인덱스 목록
        let mut buckets: HashMap<(u32, u32, u32, u32), Vec<usize>> = HashMap::new();

        let row_start = b * ROWS;
        for (i, sig) in signatures.iter().enumerate() {
            let key = (
                sig[row_start],
                sig[row_start + 1],
                sig[row_start + 2],
                sig[row_start + 3],
            );
            buckets.entry(key).or_default().push(i);
        }

        // 버킷에 2개 이상 들어있는 문서들 간 쌍 추가
        for docs in buckets.values() {
            if docs.len() >= 2 {
                for i in 0..docs.len() {
                    for j in i + 1..docs.len() {
                        let u = docs[i];
                        let v = docs[j];
                        let pair = if u < v { (u, v) } else { (v, u) };
                        candidate_pairs.insert(pair);
                    }
                }
            }
        }
    }

    // 문서 수가 비교적 적은 경우(100개 미만), 누락 방지를 위해 모든 쌍도 검사 대상에 포함할 수 있도록 함
    if n <= 100 {
        for i in 0..n {
            for j in i + 1..n {
                candidate_pairs.insert((i, j));
            }
        }
    }

    candidate_pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_bigrams() {
        let set = extract_bigrams("예산 집행 계획");
        // fold("예산 집행 계획") = "예산집행계획"
        // bigrams: "예산", "산집", "집행", "행계", "계획"
        assert!(set.contains("예산"));
        assert!(set.contains("집행"));
        assert!(set.contains("계획"));
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn test_jaccard_similarity() {
        let set1 = extract_bigrams("예산 집행 계획");
        let set2 = extract_bigrams("예산 집행 실적");
        // set1: 예산, 산집, 집행, 행계, 계획 (5)
        // set2: 예산, 산집, 집행, 행실, 실적 (5)
        // 교집합: 예산, 산집, 집행 (3)
        // 합집합: 7
        let j = jaccard_similarity(&set1, &set2);
        let expected = 3.0 / 7.0;
        assert!((j - expected).abs() < 1e-6);
    }

    #[test]
    fn test_weighted_similarity() {
        let d1 = DocFeatures::from_doc(
            "예산 집행 계획",
            "2026년도 예산 집행 계획서",
            &["개요".to_string(), "집행 절차".to_string()],
        );
        let d2 = DocFeatures::from_doc(
            "예산 집행 실적",
            "2026년도 예산 집행 실적보고",
            &["개요".to_string(), "실적 집계".to_string()],
        );

        let sim = calculate_weighted_similarity(&d1, &d2);
        assert!(sim > 0.4, "Similarity should be significant: {sim}");
        assert!(sim <= 1.0);
    }

    #[test]
    fn test_minhash_and_lsh() {
        let d1 = DocFeatures::from_doc("정보화사업 추진계획", "정보화사업 계획", &[]);
        let d2 = DocFeatures::from_doc("정보화사업 추진계획", "정보화사업 실적", &[]);
        let d3 = DocFeatures::from_doc("청사 환경 정비", "청사 환경 개선", &[]);

        let sig1 = compute_minhash_64(&d1.all_bigrams);
        let sig2 = compute_minhash_64(&d2.all_bigrams);
        let sig3 = compute_minhash_64(&d3.all_bigrams);

        let signatures = vec![sig1, sig2, sig3];
        let candidates = lsh_candidate_pairs(&signatures);

        // d1과 d2는 높은 유사도로 후보에 포함되어야 함
        assert!(candidates.contains(&(0, 1)));
    }
}
