//! Windows 경계: 재활용 전용 IFileOperation과 Explorer CF_HDROP 복사.
use crate::file_ops::Backend;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{System::Com::*, UI::Shell::*},
    core::{HRESULT, PCWSTR, Ref, implement},
};

pub struct SystemBackend {
    pub owner: isize,
}
impl Backend for SystemBackend {
    fn recycle(&self, path: &Path) -> Result<(), String> {
        recycle(path)
    }
    fn clipboard(&self, paths: &[PathBuf]) -> Result<(), String> {
        clipboard(self.owner, paths)
    }
}

/// 영구 삭제로 전환되는 호출은 반드시 거절한다. 별도 시험에서 실제 파일 없이 이 경계를 검사한다.
pub fn allow_recycle(flags: u32) -> bool {
    flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 != 0
}

#[implement(IFileOperationProgressSink)]
struct RecycleOnly {
    refused: Arc<AtomicBool>,
}

#[allow(non_snake_case, unused_variables)]
impl IFileOperationProgressSink_Impl for RecycleOnly_Impl {
    fn PreDeleteItem(&self, flags: u32, item: Ref<IShellItem>) -> windows::core::Result<()> {
        if allow_recycle(flags) {
            Ok(())
        } else {
            self.refused.store(true, Ordering::Relaxed);
            Err(windows::core::Error::from_hresult(HRESULT(
                0x80004004u32 as i32,
            )))
        }
    }
    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, result: HRESULT) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        name: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostRenameItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        name: &PCWSTR,
        result: HRESULT,
        new: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreMoveItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
        result: HRESULT,
        new: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreCopyItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
        result: HRESULT,
        new: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostDeleteItem(
        &self,
        flags: u32,
        item: Ref<IShellItem>,
        result: HRESULT,
        new: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreNewItem(
        &self,
        flags: u32,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        flags: u32,
        dest: Ref<IShellItem>,
        name: &PCWSTR,
        template: &PCWSTR,
        attributes: u32,
        result: HRESULT,
        new: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, total: u32, done: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

fn recycle(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() < 3 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return Err("휴지통을 쓸 수 없는 위치라 삭제하지 않았습니다".into());
    }
    let root = format!("{}:\\", bytes[0] as char);
    let root: Vec<_> = root.encode_utf16().chain(Some(0)).collect();
    if unsafe { GetDriveTypeW(root.as_ptr()) } != 3
    /* DRIVE_FIXED */
    {
        return Err("네트워크·이동식 드라이브는 휴지통 삭제를 할 수 없습니다".into());
    }
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        let result = (|| -> windows::core::Result<()> {
            let operation: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
            operation.SetOperationFlags(
                FOF_ALLOWUNDO
                    | FOF_NOCONFIRMATION
                    | FOF_SILENT
                    | FOF_NOERRORUI
                    | FOFX_EARLYFAILURE
                    | FOFX_RECYCLEONDELETE,
            )?;
            let wide: Vec<_> = text.encode_utf16().chain(Some(0)).collect();
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
            let refused = Arc::new(AtomicBool::new(false));
            let sink: IFileOperationProgressSink = RecycleOnly {
                refused: refused.clone(),
            }
            .into();
            operation.DeleteItem(&item, &sink)?;
            let performed = operation.PerformOperations();
            if refused.load(Ordering::Relaxed) {
                return Err(windows::core::Error::new(
                    HRESULT(0x80004004u32 as i32),
                    "휴지통으로 보낼 수 없어 삭제하지 않았습니다",
                ));
            }
            performed?;
            if operation.GetAnyOperationsAborted()?.as_bool() || path.exists() {
                return Err(windows::core::Error::new(
                    HRESULT(0x80004005u32 as i32),
                    "휴지통으로 보내지 못했습니다",
                ));
            }
            Ok(())
        })();
        CoUninitialize();
        result.map_err(|e| e.to_string())
    }
}

fn clipboard(owner: isize, paths: &[PathBuf]) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        System::{DataExchange::*, Memory::*},
    };
    unsafe fn data(format: u32, bytes: &[u8]) -> Result<(), String> {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if handle.is_null() {
                return Err("클립보드 메모리를 할당하지 못했습니다".into());
            }
            let memory = GlobalLock(handle);
            if memory.is_null() {
                GlobalFree(handle);
                return Err("클립보드 메모리를 잠그지 못했습니다".into());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), memory.cast(), bytes.len());
            GlobalUnlock(handle);
            if SetClipboardData(format, handle).is_null() {
                GlobalFree(handle);
                return Err("클립보드에 파일을 넣지 못했습니다".into());
            }
            Ok(()) // 성공 시 소유권은 Windows로 넘어간다.
        }
    }
    let payload = crate::native_ops::hdrop_payload(paths);
    unsafe {
        let effect =
            RegisterClipboardFormatW(crate::native_ops::wide("Preferred DropEffect").as_ptr());
        if effect == 0 {
            return Err("클립보드 형식을 등록하지 못했습니다".into());
        }
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(owner as _) != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        if !opened {
            return Err("클립보드를 사용할 수 없습니다. 잠시 후 다시 시도해 주세요".into());
        }
        let result = (|| {
            if EmptyClipboard() == 0 {
                return Err("클립보드를 비우지 못했습니다".into());
            }
            data(15, &payload)?;
            data(effect, &1u32.to_le_bytes())
        })();
        CloseClipboard();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sink_aborts_permanent_delete_and_allows_only_recycling() {
        let refused = Arc::new(AtomicBool::new(false));
        let sink: IFileOperationProgressSink = RecycleOnly {
            refused: refused.clone(),
        }
        .into();
        unsafe {
            assert!(sink.PreDeleteItem(0, None).is_err());
            assert!(refused.load(Ordering::Relaxed));
            assert!(
                sink.PreDeleteItem(TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32, None)
                    .is_ok()
            );
        }
    }
}
