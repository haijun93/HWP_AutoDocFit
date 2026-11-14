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

/// 문자열에서 조사·어미를 분리/제거하고 유효한 어간(줄기)을 추출한다.
pub fn stem_korean_word(word: &str) -> String {
    let f = kfind_core::fold(word);
    let char_count = f.chars().count();
    if char_count < 2 {
        return f;
    }

    let mut best_suffix_len = 0;
    for &suffix in kfind_core::stem_suffixes() {
        if f.ends_with(suffix) {
            let s_count = suffix.chars().count();
            // 조사를 제거하고 남은 어간이 최소 2글자 이상이어야 유효
            if char_count >= s_count + 2 && s_count > best_suffix_len {
                best_suffix_len = s_count;
            }
        }
    }

    if best_suffix_len > 0 {
        let keep_chars = char_count - best_suffix_len;
        f.chars().take(keep_chars).collect()
    } else {
        f
    }
}

/// 공문서에서 흔히 쓰이는 서식/유형어 (SEMI3 불용어 확장)
pub fn is_common_word(w: &str) -> bool {
    let f = kfind_core::fold(w);
    if kfind_core::is_common_stopword(&f) {
        return true;
    }
    if f.ends_with("본") && f.chars().count() >= 3 {
        let base = &f[..f.len() - "본".len()];
        if matches!(base, "최종" | "수정" | "초" | "정본" | "사") {
            return true;
        }
    }
    matches!(
        f.as_str(),
        "수칙"
            | "매뉴얼"
            | "서식"
            | "대장"
            | "목록"
            | "명부"
            | "일지"
            | "회의록"
            | "사본"
            | "최종"
            | "초안"
            | "수정본"
            | "최종본"
            | "배포용"
            | "제출용"
            | "pdf"
            | "hwp"
            | "hwpx"
            | "docx"
            | "xlsx"
            | "txt"
    )
}

/// 단어의 SEMI3 가중치를 계산한다 (흔한 말 '계획·보고·추진' 등은 가중치를 낮춤).
pub fn get_word_weight(w: &str) -> f64 {
    let f = kfind_core::fold(w);
    if f.is_empty() {
        return 0.0;
    }
    if f.chars().all(|c| c.is_ascii_digit()) {
        return 0.1;
    }
    if is_common_word(&f) {
        0.10
    } else {
        1.0
    }
}

/// 텍스트에서 공백·조사 분리를 거쳐 가중치 낱말 맵을 추출한다.
pub fn extract_weighted_words(text: &str) -> HashMap<String, f64> {
    let mut words = HashMap::new();
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || (0xAC00..=0xD7A3).contains(&(c as u32)) {
                c
            } else {
                ' '
            }
        })
        .collect();

    for tok in cleaned.split_whitespace() {
        let f = kfind_core::fold(tok);
        if f.chars().count() < 2 {
            continue;
        }
        let stem = stem_korean_word(&f);
        let char_count = stem.chars().count();
        if char_count >= 2 {
            let weight = get_word_weight(&stem);
            let entry = words.entry(stem.clone()).or_insert(0.0f64);
            if weight > *entry {
                *entry = weight;
            }

            // 4글자 이상 복합 명사 분해: 2글자 복합 한자어 단위 서브토큰 추출
            // 예: "정보보안" -> "정보", "보안" / "기본계획" -> "기본", "계획"
            if char_count == 4 {
                let chars: Vec<char> = stem.chars().collect();
                let sub1: String = chars[0..2].iter().collect();
                let sub2: String = chars[2..4].iter().collect();
                for sub in [sub1, sub2] {
                    let sw = get_word_weight(&sub);
                    let e = words.entry(sub).or_insert(0.0f64);
                    if sw > *e {
                        *e = sw;
                    }
                }
            } else if char_count == 5 || char_count == 6 {
                let chars: Vec<char> = stem.chars().collect();
                let sub_front: String = chars[0..2].iter().collect();
                let sub_back: String = chars[char_count - 2..char_count].iter().collect();
                for sub in [sub_front, sub_back] {
                    let sw = get_word_weight(&sub);
                    let e = words.entry(sub).or_insert(0.0f64);
                    if sw > *e {
                        *e = sw;
                    }
                }
            }
        }
    }
    words
}

/// 두 낱말 가중치 맵 간의 가중 Dice 유사도를 계산한다 (0.0 ~ 1.0).
pub fn weighted_word_similarity(
    w1: &HashMap<String, f64>,
    w2: &HashMap<String, f64>,
) -> f64 {
    if w1.is_empty() || w2.is_empty() {
        return 0.0;
    }

    let mut intersection_weight = 0.0;
    let mut sum1 = 0.0;
    let mut sum2 = 0.0;

    for (k, &wt1) in w1 {
        sum1 += wt1;
        if let Some(&wt2) = w2.get(k) {
            intersection_weight += wt1.min(wt2);
        }
    }
    for wt2 in w2.values() {
        sum2 += wt2;
    }

    let total = sum1 + sum2;
    if total <= 1e-9 {
        return 0.0;
    }

    (2.0 * intersection_weight) / total
}

/// 2글자 Jaccard 유사도와 낱말 단위 가중 유사도를 섞어 판정 (15% bigram + 85% word).
pub fn blend_field_similarity(
    bigrams1: &HashSet<String>,
    bigrams2: &HashSet<String>,
    words1: &HashMap<String, f64>,
    words2: &HashMap<String, f64>,
) -> f64 {
    let bigram_sim = jaccard_similarity(bigrams1, bigrams2);
    let word_sim = weighted_word_similarity(words1, words2);

    0.15 * bigram_sim + 0.85 * word_sim
}

/// 문서 특성 조각 집합들.
#[derive(Clone, Debug)]
pub struct DocFeatures {
    pub name_bigrams: HashSet<String>,
    pub title_bigrams: HashSet<String>,
    pub headings_bigrams: HashSet<String>,
    /// 통합 조각 집합 (MinHash 계산용)
    pub all_bigrams: HashSet<String>,
    /// 낱말 단위 가중치 맵
    pub name_words: HashMap<String, f64>,
    pub title_words: HashMap<String, f64>,
    pub headings_words: HashMap<String, f64>,
}

impl DocFeatures {
    pub fn from_doc(name_norm: &str, title: &str, headings: &[String]) -> Self {
        let name_bigrams = extract_bigrams(name_norm);
        // title을 정규화하여 순수 제목 조각 추출 (비어있으면 name_norm 활용)
        let title_clean = if title.trim().is_empty() {
            name_norm.to_string()
        } else {
            let t_stripped = crate::norm::strip_extension(title);
            let t_norm = crate::norm::normalize_name(t_stripped);
            if t_norm.trim().is_empty() {
                t_stripped.to_string()
            } else {
                t_norm
            }
        };
        let title_bigrams = extract_bigrams(&title_clean);
        let headings_bigrams = extract_headings_bigrams(headings);

        let name_words = extract_weighted_words(name_norm);
        let title_words = extract_weighted_words(&title_clean);

        let mut headings_words = HashMap::new();
        for h in headings.iter().take(20) {
            let hw = extract_weighted_words(h);
            for (k, v) in hw {
                let e = headings_words.entry(k).or_insert(0.0);
                if v > *e {
                    *e = v;
                }
            }
        }

        let mut all_bigrams = HashSet::new();
        all_bigrams.extend(name_bigrams.clone());
        all_bigrams.extend(title_bigrams.clone());
        all_bigrams.extend(headings_bigrams.clone());
        all_bigrams.extend(name_words.keys().cloned());
        all_bigrams.extend(title_words.keys().cloned());

        Self {
            name_bigrams,
            title_bigrams,
            headings_bigrams,
            all_bigrams,
            name_words,
            title_words,
            headings_words,
        }
    }
}

/// 두 문서 간의 가중 유사도를 계산한다.
/// 파일명 0.4 + 제목 0.4 + 중소제목 0.2 (headings 부재 시 0.5 : 0.5 정규화)
pub fn calculate_weighted_similarity(d1: &DocFeatures, d2: &DocFeatures) -> f64 {
    let sim_name = blend_field_similarity(
        &d1.name_bigrams,
        &d2.name_bigrams,
        &d1.name_words,
        &d2.name_words,
    );
    let sim_title = blend_field_similarity(
        &d1.title_bigrams,
        &d2.title_bigrams,
        &d1.title_words,
        &d2.title_words,
    );

    let has_headings = !d1.headings_words.is_empty() && !d2.headings_words.is_empty();
    if has_headings {
        let sim_headings = blend_field_similarity(
            &d1.headings_bigrams,
            &d2.headings_bigrams,
            &d1.headings_words,
            &d2.headings_words,
        );
        0.4 * sim_name + 0.4 * sim_title + 0.2 * sim_headings
    } else {
        0.5 * sim_name + 0.5 * sim_title
    }
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

    #[test]
    fn test_stem_korean_word() {
        assert_eq!(stem_korean_word("예산을"), "예산");
        assert_eq!(stem_korean_word("예산에서는"), "예산");
        assert_eq!(stem_korean_word("계획으로"), "계획");
        assert_eq!(stem_korean_word("청사로"), "청사");
        assert_eq!(stem_korean_word("편성하는"), "편성");
        assert_eq!(stem_korean_word("집행된"), "집행");
        // 어간이 1글자가 되는 경우는 조사를 떼지 않고 유지
        assert_eq!(stem_korean_word("회의"), "회의");
    }

    #[test]
    fn test_threshold_calibration_20_pairs() {
        use crate::norm::normalize_name;

        // 실제 파일명 형태 예시 20쌍 (동일 주제 10쌍, 이종 주제 10쌍)
        // 로컬 경로 금지: 파일명 또는 가상 상대 경로만 사용
        let same_topic_pairs = [
            (
                "2026년도_예산_편성_계획(안).hwp",
                "2026년_예산_집행_계획서_최종.hwpx",
                "예산 편성 vs 집행",
            ),
            (
                "청사 방호 및 보안 관리 지침.pdf",
                "청사 출입 통제 보안 수칙 v1.2.hwpx",
                "청사 방호 vs 출입 보안",
            ),
            (
                "2026_정보보안_기본계획(배포용).docx",
                "정보보안 종합계획 수립 보고.hwpx",
                "정보보안 기본계획 vs 종합계획",
            ),
            (
                "디지털 혁신 추진계획.hwpx",
                "디지털 혁신 사업 추진 실적 보고서.docx",
                "디지털 혁신 계획 vs 실적",
            ),
            (
                "주유소 유류탱크 안전점검 결과보고_수정.hwp",
                "주유소 안전점검 계획서(사본).hwpx",
                "주유소 안전점검 결과 vs 계획",
            ),
            (
                "인사위원회 운영 계획안.hwpx",
                "인사위원회 개최 및 심의결과 보고.hwp",
                "인사위원회 운영 vs 심의결과",
            ),
            (
                "민원 콜센터 운영 매뉴얼_v2.0.pdf",
                "민원 콜센터 상담 응대 지침(2026).hwpx",
                "민원 콜센터 매뉴얼 vs 지침",
            ),
            (
                "공유재산 실태조사 추진계획.hwp",
                "공유재산 관리 실태조사 결과 보고서.hwpx",
                "공유재산 실태조사 계획 vs 결과",
            ),
            (
                "도로 시설물 유지보수 시행계획.hwpx",
                "도로 시설물 정밀점검 및 유지보수 결과.docx",
                "도로 시설물 유지보수 계획 vs 결과",
            ),
            (
                "을지연습 추진계획서_최종본.hwp",
                "2026 을지연습 종합상황 보고.hwpx",
                "을지연습 계획 vs 보고",
            ),
        ];

        let diff_topic_pairs = [
            (
                "2026년도 예산 편성 계획.hwp",
                "공유재산 실태조사 추진계획.hwp",
                "예산 vs 공유재산 ('계획' 불용어 중복)",
            ),
            (
                "청사 방호 및 보안 관리 지침.pdf",
                "민원 콜센터 운영 매뉴얼.pdf",
                "청사보안 vs 민원콜센터 ('지침/매뉴얼' 중복)",
            ),
            (
                "정보보안 종합계획 수립 보고.hwpx",
                "인사위원회 개최 및 심의결과 보고.hwp",
                "정보보안 vs 인사위원회 ('보고' 불용어 중복)",
            ),
            (
                "디지털 혁신 추진계획.hwpx",
                "을지연습 추진계획서.hwp",
                "디지털혁신 vs 을지연습 ('추진계획' 중복)",
            ),
            (
                "주유소 안전점검 계획서.hwpx",
                "도로 시설물 유지보수 시행계획.hwpx",
                "주유소 vs 도로시설물 ('계획' 중복)",
            ),
            (
                "인사위원회 운영 계획안.hwpx",
                "청사 방호 지침.hwpx",
                "인사위원회 vs 청사방호",
            ),
            (
                "민원 콜센터 상담 응대 지침.hwpx",
                "2026년 예산 집행 계획서.hwpx",
                "민원콜센터 vs 예산집행",
            ),
            (
                "도로 시설물 유지보수 시행계획.hwpx",
                "디지털 혁신 사업 추진 실적.docx",
                "도로시설물 vs 디지털혁신",
            ),
            (
                "을지연습 종합상황 보고.hwpx",
                "주유소 유류탱크 안전점검 결과보고.hwp",
                "을지연습 vs 주유소안전점검 ('보고' 중복)",
            ),
            (
                "공유재산 관리 실태조사 결과.hwpx",
                "2026 정보보안 기본계획.docx",
                "공유재산 vs 정보보안",
            ),
        ];

        const THRESHOLD: f64 = 0.45;

        // 1. 동일 주제 10쌍 검증 (모두 임계값 0.45 이상이어야 함)
        for (f1, f2, desc) in same_topic_pairs {
            let n1 = normalize_name(f1);
            let n2 = normalize_name(f2);
            let d1 = DocFeatures::from_doc(&n1, f1, &[]);
            let d2 = DocFeatures::from_doc(&n2, f2, &[]);
            let sim = calculate_weighted_similarity(&d1, &d2);
            assert!(
                sim >= THRESHOLD,
                "[{desc}] Same topic similarity should be >= {THRESHOLD}, got {sim:.4}. ({f1} vs {f2})"
            );
        }

        // 2. 이종 주제 10쌍 검증 (모두 임계값 0.45 미만이어야 함 - 흔한 말 가중치 낮춤 효과 검증)
        for (f1, f2, desc) in diff_topic_pairs {
            let n1 = normalize_name(f1);
            let n2 = normalize_name(f2);
            let d1 = DocFeatures::from_doc(&n1, f1, &[]);
            let d2 = DocFeatures::from_doc(&n2, f2, &[]);
            let sim = calculate_weighted_similarity(&d1, &d2);
            assert!(
                sim < THRESHOLD,
                "[{desc}] Different topic similarity should be < {THRESHOLD}, got {sim:.4}. ({f1} vs {f2})"
            );
        }
    }
}
