//! Where things live. Portable mode: everything next to the executable.
//!
//! <exe folder>\fits\          input .fit files (read only)
//! <exe folder>\data\          fitledger.db + automatic backups
//!
//! FITLEDGER_HOME overrides the base folder (development and tests).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub base: PathBuf,
    pub fits: PathBuf,
    pub data: PathBuf,
    pub db: PathBuf,
    pub backups: PathBuf,
}

impl Paths {
    pub fn resolve() -> Paths {
        let base = std::env::var_os("FITLEDGER_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(Path::to_path_buf))
            })
            .unwrap_or_else(|| PathBuf::from("."));
        Paths::at(base)
    }

    pub fn at(base: PathBuf) -> Paths {
        let data = base.join("data");
        Paths {
            fits: base.join("fits"),
            db: data.join("fitledger.db"),
            backups: data.join("backups"),
            data,
            base,
        }
    }

    /// Creates the folders and checks that the data folder is writable.
    pub fn prepare(&self) -> Result<(), String> {
        let explain = |p: &Path, e: std::io::Error| {
            trf!(
                "Não consegui gravar em \"{}\" ({e}).\n\nO FitLedger guarda os dados na pasta do \
                 próprio aplicativo. Coloque o FitLedger.exe numa pasta onde você possa gravar \
                 arquivos (por exemplo, Documentos ou a Área de Trabalho), e não em \
                 \"Arquivos de Programas\".",
                "Could not write to \"{}\" ({e}).\n\nFitLedger keeps its data in the app's own \
                 folder. Put FitLedger.exe in a folder where you can write files (for example \
                 Documents or the Desktop), not in \"Program Files\".",
                p.display()
            )
        };
        for dir in [&self.fits, &self.data, &self.backups] {
            std::fs::create_dir_all(dir).map_err(|e| explain(dir, e))?;
        }
        let probe = self.data.join(".write-test");
        std::fs::write(&probe, b"ok").map_err(|e| explain(&self.data, e))?;
        let _ = std::fs::remove_file(probe);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_folders() {
        let base = std::env::temp_dir().join(format!("fitledger-paths-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let p = Paths::at(base.clone());
        p.prepare().unwrap();
        assert!(p.fits.is_dir() && p.data.is_dir() && p.backups.is_dir());
        assert_eq!(p.db, base.join("data").join("fitledger.db"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
