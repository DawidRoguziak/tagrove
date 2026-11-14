use std::{
    fs::{File, OpenOptions},
    path::Path,
};

use anyhow::Context;
use fs2::FileExt;

const INSTANCE_LOCK_FILE_NAME: &str = "instance.lock";

pub struct InstanceLock {
    file: File,
}

impl InstanceLock {
    pub fn acquire(app_data_dir: &Path) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(app_data_dir.join(INSTANCE_LOCK_FILE_NAME))
            .context("cannot open the data profile lock")?;

        match file.try_lock_exclusive() {
            Ok(()) => Ok(Self { file }),
            Err(error) if error.kind() == fs2::lock_contended_error().kind() => {
                anyhow::bail!("another Tagrove instance is already using this data profile")
            }
            Err(error) => Err(error).context("cannot lock the data profile"),
        }
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}
