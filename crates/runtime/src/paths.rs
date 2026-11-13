use anyhow::{Context, Result, ensure};
use directories::BaseDirs;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize)]
pub struct AppPaths {
    pub data: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
    pub state: PathBuf,
}

impl AppPaths {
    pub fn native() -> Result<Self> {
        let base = BaseDirs::new().context("无法确定用户目录")?;
        #[cfg(target_os = "linux")]
        {
            Self::linux(base.home_dir(), &std::env::vars().collect())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self {
                data: base.data_local_dir().join("reader"),
                config: base.config_dir().join("reader"),
                cache: base.cache_dir().join("reader"),
                state: base.data_local_dir().join("reader/state"),
            })
        }
    }
    pub fn linux(home: &Path, env: &HashMap<String, String>) -> Result<Self> {
        ensure!(home.is_absolute(), "用户主目录必须为绝对路径");
        let resolve = |key: &str, fallback: &str| {
            env.get(key)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(fallback))
                .join("reader")
        };
        Ok(Self {
            data: resolve("XDG_DATA_HOME", ".local/share"),
            config: resolve("XDG_CONFIG_HOME", ".config"),
            cache: resolve("XDG_CACHE_HOME", ".cache"),
            state: resolve("XDG_STATE_HOME", ".local/state"),
        })
    }
    pub fn isolated(root: &Path) -> Result<Self> {
        ensure!(root.is_absolute(), "测试根目录必须为绝对路径");
        Ok(Self {
            data: root.join("data"),
            config: root.join("config"),
            cache: root.join("cache"),
            state: root.join("state"),
        })
    }
    pub fn books(&self) -> PathBuf {
        self.data.join("books")
    }
    pub fn create(&self) -> Result<()> {
        for p in [
            &self.data,
            &self.config,
            &self.cache,
            &self.state,
            &self.books(),
            &self.data.join("staging"),
        ] {
            std::fs::create_dir_all(p)?;
        }
        Ok(())
    }
}
