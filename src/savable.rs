use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Serialize};

pub struct Savable<T> {
    save_path: PathBuf,
    inner: T,
}

impl<T> Savable<T> {
    pub fn new(save_path: PathBuf, inner: T) -> Self {
        Self { save_path, inner }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> Savable<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn load<P: AsRef<Path>>(from: P) -> io::Result<Self> {
        let save_path = from.as_ref().to_owned();
        let inner: T = serde_json::from_reader(&fs::File::open(&save_path)?)?;

        Ok(Self { save_path, inner })
    }

    pub fn save(&self) -> io::Result<()> {
        let mut tmp_fn = self.save_path.file_name().unwrap_or_default().to_owned();
        tmp_fn.push(".new");

        let tmp_path = self.save_path.with_file_name(tmp_fn);

        {
            let mut out = fs::File::create(&tmp_path)?;
            serde_json::to_writer_pretty(&mut out, &self.inner)?;
            out.sync_all()?;
        }

        fs::rename(&tmp_path, &self.save_path)
    }
}
