//! NTFS 볼륨 직접 읽기: `FSCTL_ENUM_USN_DATA`로 MFT 항목 열거, `FSCTL_READ_USN_JOURNAL`로 변경 따라잡기.
//! 볼륨 열기에 관리자 권한이 필요하다(없으면 `Error::AccessDenied`). 파일 내용은 열지 않는다(이름·부모·속성만).
use std::ffi::c_void;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_HANDLE_EOF, GENERIC_READ, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDriveTypeW, GetLogicalDrives,
    GetVolumeInformationW, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

use crate::build::Builder;
use crate::source::{Change, kind_from_reason, rec_of};
use crate::{Error, Result};

const FSCTL_ENUM_USN_DATA: u32 = 0x0009_00b3;
const FSCTL_QUERY_USN_JOURNAL: u32 = 0x0009_00f4;
const FSCTL_READ_USN_JOURNAL: u32 = 0x0009_00bb;
const ERROR_JOURNAL_DELETE_IN_PROGRESS: u32 = 1178;
const ERROR_JOURNAL_NOT_ACTIVE: u32 = 1179;
const ERROR_JOURNAL_ENTRY_DELETED: u32 = 1181;
const DRIVE_FIXED: u32 = 3;
/// NTFS 뿌리 폴더 레코드 번호.
pub const ROOT_REC: u64 = 5;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 고정 디스크 드라이브 문자와 파일 시스템 이름.
pub fn fixed_drives() -> Vec<(char, String)> {
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for k in 0..26u32 {
        if mask & (1 << k) == 0 {
            continue;
        }
        let letter = (b'A' + k as u8) as char;
        let root = wide(&format!("{letter}:\\"));
        if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
            continue;
        }
        let mut fs = [0u16; 64];
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        };
        let name = if ok != 0 {
            let l = fs.iter().position(|&c| c == 0).unwrap_or(fs.len());
            String::from_utf16_lossy(&fs[..l])
        } else {
            String::new()
        };
        out.push((letter, name));
    }
    out
}

/// 열린 볼륨 핸들.
pub struct Volume(HANDLE);

unsafe impl Send for Volume {}
unsafe impl Sync for Volume {}

impl Drop for Volume {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

impl Volume {
    pub fn open(letter: char) -> Result<Volume> {
        let p = wide(&format!("\\\\.\\{letter}:"));
        let h = unsafe {
            CreateFileW(
                p.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            let e = unsafe { GetLastError() };
            return Err(if e == ERROR_ACCESS_DENIED { Error::AccessDenied } else { Error::Io(std::io::Error::from_raw_os_error(e as i32)) });
        }
        Ok(Volume(h))
    }

    fn ioctl(&self, code: u32, input: &[u8], out: &mut [u8]) -> std::result::Result<usize, u32> {
        let mut got = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                code,
                input.as_ptr() as *const c_void,
                input.len() as u32,
                out.as_mut_ptr() as *mut c_void,
                out.len() as u32,
                &mut got,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 { Err(unsafe { GetLastError() }) } else { Ok(got as usize) }
    }

    /// USN 저널 정보. 저널이 없으면 None.
    pub fn journal(&self) -> Result<Option<Journal>> {
        let mut out = [0u8; 80];
        match self.ioctl(FSCTL_QUERY_USN_JOURNAL, &[], &mut out) {
            Ok(n) if n >= 24 => Ok(Some(Journal {
                id: u64::from_le_bytes(out[0..8].try_into().unwrap()),
                first_usn: i64::from_le_bytes(out[8..16].try_into().unwrap()),
                next_usn: i64::from_le_bytes(out[16..24].try_into().unwrap()),
            })),
            Ok(_) => Err(Error::Corrupt("short journal data".into())),
            Err(ERROR_JOURNAL_NOT_ACTIVE) | Err(ERROR_JOURNAL_DELETE_IN_PROGRESS) => Ok(None),
            Err(ERROR_ACCESS_DENIED) => Err(Error::AccessDenied),
            Err(e) => Err(Error::Io(std::io::Error::from_raw_os_error(e as i32))),
        }
    }

    /// MFT 항목을 모두 열거한다. `f(frn, parent_frn, name_utf16, attrs)`. 돌려주는 값은 항목 수.
    pub fn enum_mft(&self, high_usn: i64, mut f: impl FnMut(u64, u64, &[u16], u32)) -> Result<usize> {
        let mut buf = vec![0u8; 1 << 20];
        let mut start: u64 = 0;
        let mut count = 0usize;
        loop {
            let mut inp = [0u8; 24];
            inp[0..8].copy_from_slice(&start.to_le_bytes());
            inp[8..16].copy_from_slice(&0i64.to_le_bytes());
            inp[16..24].copy_from_slice(&high_usn.to_le_bytes());
            match self.ioctl(FSCTL_ENUM_USN_DATA, &inp, &mut buf) {
                Ok(n) if n >= 8 => {
                    start = u64::from_le_bytes(buf[0..8].try_into().unwrap());
                    count += parse_records(&buf[8..n], |r| f(r.frn, r.parent, r.name, r.attrs));
                }
                Ok(_) => break,
                Err(ERROR_HANDLE_EOF) => break,
                Err(ERROR_ACCESS_DENIED) => return Err(Error::AccessDenied),
                Err(e) => return Err(Error::Io(std::io::Error::from_raw_os_error(e as i32))),
            }
        }
        Ok(count)
    }

    /// `start`부터 USN 기록을 읽는다. `wait_secs`가 Some이면 새 기록이 올 때까지(최대 그 초) 막혀 기다린다(0 = 끝없이).
    /// 돌려주는 값: (다음 USN, 변경들). 저널이 잘려 나갔으면 `Error::Rescan`.
    pub fn read_usn(&self, vol: u16, j: &Journal, start: i64, wait_secs: Option<u64>, buf: &mut Vec<u8>) -> Result<(i64, Vec<Change>)> {
        if buf.len() < 64 * 1024 {
            buf.resize(64 * 1024, 0);
        }
        let mut inp = [0u8; 40];
        inp[0..8].copy_from_slice(&start.to_le_bytes());
        inp[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        inp[12..16].copy_from_slice(&0u32.to_le_bytes());
        inp[16..24].copy_from_slice(&wait_secs.unwrap_or(0).to_le_bytes());
        inp[24..32].copy_from_slice(&(if wait_secs.is_some() { 1u64 } else { 0 }).to_le_bytes());
        inp[32..40].copy_from_slice(&j.id.to_le_bytes());
        match self.ioctl(FSCTL_READ_USN_JOURNAL, &inp, buf) {
            Ok(n) if n >= 8 => {
                let next = i64::from_le_bytes(buf[0..8].try_into().unwrap());
                let mut out = Vec::new();
                parse_records(&buf[8..n], |r| {
                    out.push(Change {
                        vol,
                        rec: rec_of(r.frn),
                        parent: rec_of(r.parent),
                        name: String::from_utf16_lossy(r.name),
                        attrs: r.attrs,
                        kind: kind_from_reason(r.reason),
                    })
                });
                Ok((next, out))
            }
            Ok(_) => Ok((start, Vec::new())),
            Err(ERROR_JOURNAL_ENTRY_DELETED) => Err(Error::Rescan("journal entries deleted".into())),
            Err(ERROR_JOURNAL_NOT_ACTIVE) | Err(ERROR_JOURNAL_DELETE_IN_PROGRESS) => Err(Error::Rescan("journal not active".into())),
            Err(ERROR_ACCESS_DENIED) => Err(Error::AccessDenied),
            Err(e) => Err(Error::Io(std::io::Error::from_raw_os_error(e as i32))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Journal {
    pub id: u64,
    pub first_usn: i64,
    pub next_usn: i64,
}

/// USN_RECORD_V2/V3 한 개를 읽은 것.
pub struct Record<'a> {
    pub frn: u64,
    pub parent: u64,
    pub usn: i64,
    pub reason: u32,
    pub attrs: u32,
    pub name: &'a [u16],
    name_buf: std::marker::PhantomData<&'a ()>,
}

/// 기록 묶음(DeviceIoControl 출력의 앞 8바이트 뒤)을 읽는다. 길이·버전이 맞지 않으면 거기서 멈춘다. 읽은 수를 돌려준다.
pub fn parse_records(mut b: &[u8], mut f: impl FnMut(Record)) -> usize {
    let mut n = 0;
    let mut tmp: Vec<u16> = Vec::new();
    while b.len() >= 8 {
        let len = u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize;
        if len < 60 || len > b.len() || len % 8 != 0 && len != b.len() {
            break;
        }
        let r = &b[..len];
        let major = u16::from_le_bytes(r[4..6].try_into().unwrap());
        let u32at = |o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap());
        let u64at = |o: usize| u64::from_le_bytes(r[o..o + 8].try_into().unwrap());
        let (frn, parent, usn, reason, attrs, nl, no) = match major {
            2 => (u64at(8), u64at(16), u64at(24) as i64, u32at(40), u32at(52), u16::from_le_bytes(r[56..58].try_into().unwrap()), u16::from_le_bytes(r[58..60].try_into().unwrap())),
            3 if len >= 76 => (u64at(8), u64at(24), u64at(40) as i64, u32at(56), u32at(68), u16::from_le_bytes(r[72..74].try_into().unwrap()), u16::from_le_bytes(r[74..76].try_into().unwrap())),
            _ => {
                b = &b[len..];
                continue;
            }
        };
        let (no, nl) = (no as usize, nl as usize);
        if no + nl > len || nl % 2 != 0 {
            break;
        }
        tmp.clear();
        tmp.extend(r[no..no + nl].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])));
        f(Record { frn, parent, usn, reason, attrs, name: &tmp, name_buf: std::marker::PhantomData });
        n += 1;
        b = &b[len..];
    }
    n
}

/// 볼륨 하나를 MFT 열거로 만들기에 넣는다. 돌려주는 값: (볼륨 번호, 저널, 항목 수).
pub fn index_volume(b: &mut Builder, letter: char) -> Result<(u16, Option<Journal>, usize)> {
    let v = Volume::open(letter)?;
    let j = v.journal()?;
    let vol = b.add_volume(&format!("{letter}:"), ROOT_REC);
    let high = j.map(|j| j.next_usn).unwrap_or(i64::MAX);
    let mut s = String::new();
    let n = v.enum_mft(high, |frn, parent, name, attrs| {
        s.clear();
        for c in char::decode_utf16(name.iter().copied()) {
            s.push(c.unwrap_or('\u{FFFD}'));
        }
        b.push(vol, frn, parent, &s, attrs);
    })?;
    Ok((vol, j, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn rec_v2(frn: u64, parent: u64, usn: i64, reason: u32, attrs: u32, name: &str) -> Vec<u8> {
        let nm: Vec<u8> = name.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        let len = (60 + nm.len()).div_ceil(8) * 8;
        let mut r = vec![0u8; len];
        r[0..4].copy_from_slice(&(len as u32).to_le_bytes());
        r[4..6].copy_from_slice(&2u16.to_le_bytes());
        r[8..16].copy_from_slice(&frn.to_le_bytes());
        r[16..24].copy_from_slice(&parent.to_le_bytes());
        r[24..32].copy_from_slice(&usn.to_le_bytes());
        r[40..44].copy_from_slice(&reason.to_le_bytes());
        r[52..56].copy_from_slice(&attrs.to_le_bytes());
        r[56..58].copy_from_slice(&(nm.len() as u16).to_le_bytes());
        r[58..60].copy_from_slice(&60u16.to_le_bytes());
        r[60..60 + nm.len()].copy_from_slice(&nm);
        r
    }

    fn rec_v3(frn: u64, parent: u64, reason: u32, name: &str) -> Vec<u8> {
        let nm: Vec<u8> = name.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        let len = (76 + nm.len()).div_ceil(8) * 8;
        let mut r = vec![0u8; len];
        r[0..4].copy_from_slice(&(len as u32).to_le_bytes());
        r[4..6].copy_from_slice(&3u16.to_le_bytes());
        r[8..16].copy_from_slice(&frn.to_le_bytes());
        r[24..32].copy_from_slice(&parent.to_le_bytes());
        r[56..60].copy_from_slice(&reason.to_le_bytes());
        r[72..74].copy_from_slice(&(nm.len() as u16).to_le_bytes());
        r[74..76].copy_from_slice(&76u16.to_le_bytes());
        r[76..76 + nm.len()].copy_from_slice(&nm);
        r
    }

    #[test]
    fn parses_v2_v3_and_stops_on_garbage() {
        let mut b = rec_v2(0x0001_0000_0000_0010, 5, 7, 0x100, 0x20, "예산.hwp");
        b.extend(rec_v3(0x11, 0x10, 0x200, "b.txt"));
        b.extend([0xFFu8; 12]);
        let mut got = Vec::new();
        let n = parse_records(&b, |r| got.push((r.frn, r.parent, r.reason, String::from_utf16_lossy(r.name))));
        assert_eq!(n, 2);
        assert_eq!(got[0], (0x0001_0000_0000_0010, 5, 0x100, "예산.hwp".to_string()));
        assert_eq!(got[1], (0x11, 0x10, 0x200, "b.txt".to_string()));
        // 잘린 기록
        let short = &rec_v2(1, 5, 0, 0, 0, "abc")[..40];
        assert_eq!(parse_records(short, |_| {}), 0);
    }

    #[test]
    fn fixed_drives_lists_something() {
        // 이 시험은 드라이브 문자만 본다(파일을 열지 않음).
        let d = fixed_drives();
        assert!(!d.is_empty());
    }

    /// 관리자 권한이 있을 때만 실제 MFT를 읽는다(없으면 건너뜀).
    #[test]
    fn real_mft_when_admin() {
        let Some((letter, _)) = fixed_drives().into_iter().find(|(_, fs)| fs == "NTFS") else { return };
        match Volume::open(letter) {
            Err(Error::AccessDenied) => eprintln!("skip: no admin"),
            Err(e) => eprintln!("skip: {e}"),
            Ok(_) => {
                let mut b = Builder::new();
                let (_, _, n) = index_volume(&mut b, letter).expect("enum");
                assert!(n > 1000);
                let names = b.finish();
                assert!(names.len() > 1000);
            }
        }
    }
}
