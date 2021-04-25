use crate::args::Args;
use crate::errors::*;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub const DEFAULT_RENEW_IF_DAYS_LEFT: i64 = 30;

#[derive(Debug, PartialEq, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub acme: AcmeConfig,
    #[serde(default)]
    pub system: SystemConfig,
}

#[derive(Debug, Default, PartialEq, Deserialize)]
pub struct AcmeConfig {
    pub acme_email: Option<String>,
    pub acme_url: Option<String>,
    pub renew_if_days_left: Option<i64>,
}

#[derive(Debug, Default, PartialEq, Deserialize)]
pub struct SystemConfig {
    pub group: Option<String>,
    #[serde(default)]
    pub exec: Vec<String>,
    #[serde(default)]
    pub exec_extra: Vec<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct CertConfigFile {
    cert: CertConfig,
}

fn load_str<T: DeserializeOwned>(s: &str) -> Result<T> {
    let conf = toml::from_str(&s).context("Failed to load config")?;
    Ok(conf)
}

fn load_file<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T> {
    let buf = fs::read_to_string(path.as_ref()).context("Failed to read file")?;
    load_str(&buf)
}

fn load_from_folder<P: AsRef<Path>>(path: P) -> Result<Vec<CertConfigFile>> {
    let mut configs = Vec::new();
    let iter = fs::read_dir(path.as_ref())
        .with_context(|| anyhow!("Failed to list directory: {:?}", path.as_ref()))?;

    for file in iter {
        let file = file?;
        let path = file.path();

        if path.extension() == Some(OsStr::new("conf")) {
            let c = load_file(&path)
                .with_context(|| anyhow!("Failed to load config file {:?}", path))?;
            configs.push(c);
        } else {
            debug!("skipping non-config file {:?}", path);
        }
    }
    Ok(configs)
}

#[derive(Debug, PartialEq, Clone, Deserialize)]
pub struct CertConfig {
    pub name: String,
    pub dns_names: Vec<String>,
    #[serde(default)]
    pub must_staple: bool,
    #[serde(default)]
    pub exec: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub certs: Vec<CertConfig>,
    pub acme_email: Option<String>,
    pub acme_url: String,
    pub renew_if_days_left: i64,
    pub data_dir: PathBuf,
    pub chall_dir: PathBuf,
    pub group: Option<String>,
    pub exec: Vec<String>,
    pub exec_extra: Vec<String>,
}

impl Config {
    pub fn filter_certs<'a>(
        &'a self,
        filter: &'a HashSet<String>,
    ) -> impl Iterator<Item = &'a CertConfig> {
        self.certs
            .iter()
            .filter(move |cert| filter.is_empty() || filter.contains(&cert.name))
    }
}

pub fn load(args: &Args) -> Result<Config> {
    // TODO: none of this is applied yet, we need to change all the arg parsing code for that
    let path = &args.config;
    let mut config = load_file::<_, ConfigFile>(path)
        .with_context(|| anyhow!("Failed to load config file {:?}", path))?;

    if args.acme_email.is_some() {
        config.acme.acme_email = args.acme_email.clone();
    }

    let certs = load_from_folder(&args.config_dir)?
        .into_iter()
        .map(|c| c.cert)
        .collect();
    Ok(Config {
        acme_email: args.acme_email.clone(),
        acme_url: args.acme_url.to_string(),
        renew_if_days_left: config
            .acme
            .renew_if_days_left
            .unwrap_or(DEFAULT_RENEW_IF_DAYS_LEFT),
        data_dir: PathBuf::from(&args.data_dir),
        chall_dir: PathBuf::from(&args.chall_dir),
        certs,
        group: config.system.group,
        exec: config.system.exec,
        exec_extra: config.system.exec_extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_cert_conf() {
        let conf = load_str::<CertConfigFile>(
            r#"
            [cert]
            name = "example.com"
            dns_names = ["example.com", "www.example.com"]
        "#,
        )
        .unwrap();

        assert_eq!(
            conf,
            CertConfigFile {
                cert: CertConfig {
                    name: "example.com".to_string(),
                    dns_names: vec!["example.com".to_string(), "www.example.com".to_string(),],
                    must_staple: false,
                    exec: vec![],
                },
            }
        );
    }
}
