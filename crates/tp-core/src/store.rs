//! Shared data synchronization. The desktop host supplies its paste-target type.
use crate::common::{err, Result};
use rusqlite::Connection;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard,
    },
};
pub struct Store<T = ()> {
    /// Transfer operations exclude capture, file workers, previews and mutations.
    pub data_gate: RwLock<()>,
    pub recovery_required: AtomicBool,
    pub db: Mutex<Connection>,
    pub dir: PathBuf,
    pub media_lock: Mutex<()>,
    pub paste_target: Mutex<Option<T>>,
    pub suppressed_digest: Mutex<Option<(String, Option<u64>)>>,
}

impl<T> Store<T> {
    /// Recover interrupted imports before opening/upgrading the database.
    pub fn open(dir: PathBuf) -> Result<Self> {
        crate::transfer::recover_interrupted_restore(&dir)?;
        let conn = crate::storage::open_database(&dir)?;
        Ok(Self::from_connection(conn, dir))
    }
    pub fn read_data(&self) -> Result<RwLockReadGuard<'_, ()>> {
        let guard = self.data_gate.read().map_err(err)?;
        if self.recovery_required.load(Ordering::SeqCst) {
            return Err(crate::i18n::tr(
                "恢复回退尚未完成，数据操作已暂停，请退出并重新打开 TP 自动恢复",
            )
            .into());
        }
        Ok(guard)
    }
    pub fn write_data(&self) -> Result<RwLockWriteGuard<'_, ()>> {
        let guard = self.data_gate.write().map_err(err)?;
        if self.recovery_required.load(Ordering::SeqCst) {
            return Err(crate::i18n::tr(
                "恢复回退尚未完成，数据操作已暂停，请退出并重新打开 TP 自动恢复",
            )
            .into());
        }
        Ok(guard)
    }
}

impl<T> Store<T> {
    pub fn from_connection(conn: Connection, dir: PathBuf) -> Self {
        Self {
            data_gate: RwLock::new(()),
            recovery_required: AtomicBool::new(false),
            db: Mutex::new(conn),
            dir,
            media_lock: Mutex::new(()),
            paste_target: Mutex::new(None),
            suppressed_digest: Mutex::new(None),
        }
    }
    pub fn snapshot(&self) -> Result<crate::Snapshot> {
        let _data = self.read_data()?;
        let conn = self.db.lock().map_err(err)?;
        crate::storage::read_snapshot(&conn, &self.dir)
    }
}
