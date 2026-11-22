//! HWPX(zip) 안전 읽기·쓰기. 기준: docfit_core/hwpx/__init__.py 의 HwpxLimits·_check_archive_limits·validate_hwpx·open_hwpx_safely.
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_TOTAL_SIZE: u64 = 512 * 1024 * 1024;
pub const MAX_FILE_SIZE: u64 = 128 * 1024 * 1024;
pub const MAX_COMPRESSION_RATIO: u64 = 1_000;

/// (항목 이름, 내용) 목록 — 압축 파일 안 순서 그대로.
pub type Contents = Vec<(String, Vec<u8>)>;

fn safe_member_name(name: &str, unix_mode: Option<u32>) -> Result<(), String> {
    let n = name.replace('\\', "/");
    let bytes = n.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if n.starts_with('/') || n.split('/').any(|p| p == "..") || drive {
        return Err(format!("안전하지 않은 압축 경로입니다: {name}"));
    }
    if let Some(m) = unix_mode {
        if m & 0o170000 == 0o120000 {
            return Err(format!("심볼릭 링크가 포함되어 있습니다: {name}"));
        }
    }
    Ok(())
}

/// 중앙 디렉터리 한 항목(Python zipfile.ZipInfo 가 읽는 값과 같다).
struct CdEntry {
    name: String,
    flags: u16,
    comp_size: u64,
    size: u64,
    ext_attr: u32,
}

/// ZIP 중앙 디렉터리를 직접 읽는다. zip 크레이트는 같은 이름 항목을 하나로 합쳐 버리므로(파이썬은 둘 다 본다)
/// 안전 검사는 원본 목록으로 한다.
fn central_directory(path: &Path) -> Result<Vec<CdEntry>, String> {
    use std::io::{Seek, SeekFrom};
    let bad = || "올바른 HWPX ZIP 문서가 아닙니다.".to_string();
    let mut f = File::open(path).map_err(|e| format!("열 수 없음: {e}"))?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let tail_len = len.min(22 + 65_535);
    f.seek(SeekFrom::Start(len - tail_len)).map_err(|_| bad())?;
    let mut tail = vec![0u8; tail_len as usize];
    f.read_exact(&mut tail).map_err(|_| bad())?;
    let eocd = (0..tail.len().saturating_sub(21)).rev().find(|&i| tail[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).ok_or_else(bad)?;
    let e = &tail[eocd..];
    let u16at = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as u64;
    let u32at = |b: &[u8], o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as u64;
    let count = u16at(e, 10);
    let cd_size = u32at(e, 12);
    let cd_off = u32at(e, 16);
    if count == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
        return Err("ZIP64 문서는 지원하지 않습니다(크기 제한을 넘는 문서).".into());
    }
    if cd_off + cd_size > len || cd_size > 64 * 1024 * 1024 {
        return Err(bad());
    }
    f.seek(SeekFrom::Start(cd_off)).map_err(|_| bad())?;
    let mut cd = vec![0u8; cd_size as usize];
    f.read_exact(&mut cd).map_err(|_| bad())?;
    let mut out = Vec::new();
    let mut o = 0usize;
    for _ in 0..count {
        if o + 46 > cd.len() || cd[o..o + 4] != [0x50, 0x4b, 0x01, 0x02] {
            return Err(bad());
        }
        let (n, x, c) = (u16at(&cd, o + 28) as usize, u16at(&cd, o + 30) as usize, u16at(&cd, o + 32) as usize);
        if o + 46 + n > cd.len() {
            return Err(bad());
        }
        let raw = &cd[o + 46..o + 46 + n];
        let flags = u16at(&cd, o + 8) as u16;
        // 파이썬: 표시 비트(0x800)가 있으면 UTF-8, 없으면 cp437(ASCII 구간은 같다)
        let name = if flags & 0x800 != 0 { String::from_utf8_lossy(raw).into_owned() } else { raw.iter().map(|&b| b as char).collect() };
        out.push(CdEntry { name, flags, comp_size: u32at(&cd, o + 20), size: u32at(&cd, o + 24), ext_attr: u32at(&cd, o + 38) as u32 });
        o += 46 + n + x + c;
    }
    Ok(out)
}

pub fn read_hwpx_safely(path: &Path) -> Result<Contents, String> {
    let entries = central_directory(path)?;
    let f = File::open(path).map_err(|e| format!("열 수 없음: {e}"))?;
    let mut ar = zip::ZipArchive::new(f).map_err(|_| "올바른 HWPX ZIP 문서가 아닙니다.".to_string())?;
    if entries.len() > MAX_ENTRIES {
        return Err(format!("압축 항목이 너무 많습니다({} > {MAX_ENTRIES}).", entries.len()));
    }
    // 1) 이름·크기·압축률 검사(내용을 풀기 전에). 기준: _check_archive_limits
    let mut names: HashSet<String> = HashSet::new();
    let mut total: u64 = 0;
    let mut meta = Vec::new();
    for e in &entries {
        let _ = e.flags;
        safe_member_name(&e.name, Some(e.ext_attr >> 16))?;
        let norm = e.name.replace('\\', "/");
        if !names.insert(norm) {
            return Err(format!("중복 압축 항목입니다: {}", e.name));
        }
        total = total.saturating_add(e.size);
        if e.size > MAX_FILE_SIZE {
            return Err(format!("압축 내부 파일이 너무 큽니다: {} ({}바이트)", e.name, e.size));
        }
        if total > MAX_TOTAL_SIZE {
            return Err(format!("압축 해제 예상 크기가 제한을 넘습니다({total}바이트)."));
        }
        if e.size > 0 && e.comp_size == 0 {
            return Err(format!("비정상 압축 항목입니다: {}", e.name));
        }
        if e.comp_size > 0 && (e.size as f64 / e.comp_size as f64) > MAX_COMPRESSION_RATIO as f64 {
            return Err(format!("비정상 압축률이 감지되었습니다: {}", e.name));
        }
        meta.push((e.name.clone(), e.size));
    }
    if !names.contains("Contents/header.xml") {
        return Err("HWPX 필수 파일 Contents/header.xml이 없습니다.".into());
    }
    if !names.iter().any(|n| crate::rules::is_section_name(n)) {
        return Err("HWPX 본문 섹션을 찾지 못했습니다.".into());
    }
    // 2) 내용 읽기: 선언 크기+1 로 끊어 거짓 선언을 막고, 읽는 동안 CRC 가 검사된다(testzip 에 해당).
    let mut out = Vec::with_capacity(ar.len());
    for (i, (name, size)) in meta.into_iter().enumerate() {
        let e = ar.by_index(i).map_err(|e| format!("CRC 검사가 실패했습니다: {name}: {e}"))?;
        let mut data = Vec::with_capacity(size.min(1 << 20) as usize);
        let n = e.take(size + 1).read_to_end(&mut data).map_err(|e| format!("CRC 검사가 실패했습니다: {name}: {e}"))?;
        if n as u64 != size {
            return Err(format!("압축 항목 크기가 선언과 다릅니다: {name}"));
        }
        out.push((name, data));
    }
    Ok(out)
}

/// 항목을 임시 파일에 쓴 뒤 바꿔 넣는다(_hwpx_안전_저장 과 같음: mimetype 은 무압축, 나머지 deflate, 20회 재시도).
pub fn write_hwpx_safely(contents: &Contents, target: &Path) -> Result<(), String> {
    let tmp = tmp_path(target);
    let res = (|| -> Result<(), String> {
        let f = File::create(&tmp).map_err(|e| format!("임시 파일 생성 실패: {e}"))?;
        let mut z = zip::ZipWriter::new(f);
        for (name, data) in contents {
            let method = if name == "mimetype" { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
            let opts = zip::write::SimpleFileOptions::default().compression_method(method);
            if name.ends_with('/') {
                z.add_directory(name.trim_end_matches('/'), opts).map_err(|e| e.to_string())?;
            } else {
                z.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
                z.write_all(data).map_err(|e| e.to_string())?;
            }
        }
        z.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    replace_file(&tmp, target)
}

pub fn copy_file_safely(source: &Path, target: &Path) -> Result<(), String> {
    let tmp = tmp_path(target);
    std::fs::copy(source, &tmp).map_err(|e| format!("복사 실패: {e}"))?;
    replace_file(&tmp, target)
}

fn tmp_path(target: &Path) -> PathBuf {
    let mut s = target.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

fn replace_file(tmp: &Path, target: &Path) -> Result<(), String> {
    for attempt in 0..20 {
        match std::fs::rename(tmp, target) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 19 => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => {
                let _ = std::fs::remove_file(tmp);
                return Err(format!("파일 교체 실패: {e}"));
            }
        }
    }
    unreachable!()
}
