use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    time::Duration,
};

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;

use crate::models::RemoteModelFile;

const PAGE_SIZE: usize = 24;

#[derive(Deserialize)]
struct HubModel {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    siblings: Vec<HubFile>,
}

#[derive(Deserialize)]
struct HubFile {
    rfilename: String,
}

pub(crate) struct HubPage {
    pub(crate) models: Vec<RemoteModelFile>,
    pub(crate) next: Option<String>,
}

pub(crate) fn search_url(query: &str) -> String {
    let query = if query.trim().is_empty() {
        "upscal"
    } else {
        query.trim()
    };
    format!(
        "https://huggingface.co/api/models?search={}&sort=downloads&direction=-1&limit={PAGE_SIZE}&full=true",
        utf8_percent_encode(query, NON_ALPHANUMERIC)
    )
}

pub(crate) fn fetch_page(url: &str) -> Result<HubPage, String> {
    let mut current_url = url.to_string();
    let mut models = Vec::new();
    let mut next = None;

    for _ in 0..5 {
        let page = fetch_page_once(&current_url)?;
        models.extend(page.models);
        next = page.next;
        if !models.is_empty() || next.is_none() {
            break;
        }
        current_url = next.clone().expect("next URL was checked above");
    }

    Ok(HubPage { models, next })
}

fn fetch_page_once(url: &str) -> Result<HubPage, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(18)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(url)
        .header("User-Agent", "super-puper-image-scaler/0.1")
        .call()
        .map_err(|error| error.to_string())?;
    let next = response
        .headers()
        .get("link")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_next_link);
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let repositories: Vec<HubModel> =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let models = repositories
        .into_iter()
        .filter_map(|repository| {
            choose_model_file(&repository.siblings).map(|file_name| RemoteModelFile {
                repo_id: repository.id,
                file_name,
                downloads: repository.downloads,
            })
        })
        .collect();
    Ok(HubPage { models, next })
}

fn choose_model_file(files: &[HubFile]) -> Option<String> {
    files
        .iter()
        .filter(|file| is_model_path(Path::new(&file.rfilename)))
        .min_by_key(|file| file.rfilename.matches('/').count())
        .map(|file| file.rfilename.clone())
}

fn is_model_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("onnx"))
}

fn parse_next_link(header: &str) -> Option<String> {
    header.split(',').find_map(|link| {
        let (url, relation) = link.trim().split_once(';')?;
        if relation.trim() != "rel=\"next\"" {
            return None;
        }
        url.trim()
            .strip_prefix('<')
            .and_then(|url| url.strip_suffix('>'))
            .map(str::to_string)
    })
}

pub(crate) fn download(model: &RemoteModelFile) -> Result<PathBuf, String> {
    let download_root = directories::UserDirs::new()
        .and_then(|directories| directories.download_dir().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| "Downloads directory is unavailable".to_string())?
        .join("Super Puper Image Scaler")
        .join("Models");
    fs::create_dir_all(&download_root).map_err(|error| error.to_string())?;

    let file_name = Path::new(&model.file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Invalid model file name".to_string())?;
    let destination = unique_download_path(&download_root, file_name);
    let partial = destination.with_extension(format!(
        "{}.part",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("model")
    ));
    let url = format!(
        "https://huggingface.co/{}/resolve/main/{}?download=true",
        encode_path(&model.repo_id),
        encode_path(&model.file_name)
    );
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60 * 30)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(&url)
        .header("User-Agent", "super-puper-image-scaler/0.1")
        .call()
        .map_err(|error| error.to_string())?;
    let mut output = File::create(&partial).map_err(|error| error.to_string())?;
    if let Err(error) = io::copy(&mut response.body_mut().as_reader(), &mut output) {
        let _ = fs::remove_file(&partial);
        return Err(error.to_string());
    }
    fs::rename(&partial, &destination).map_err(|error| {
        let _ = fs::remove_file(&partial);
        error.to_string()
    })?;
    Ok(destination)
}

fn unique_download_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("model");
    let extension = path.extension().and_then(|extension| extension.to_str());
    for suffix in 2..10_000 {
        let name = match extension {
            Some(extension) => format!("{stem} ({suffix}).{extension}"),
            None => format!("{stem} ({suffix})"),
        };
        let candidate = directory.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("downloaded-{file_name}"))
}

fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn format_downloads(downloads: u64) -> String {
    if downloads >= 1_000_000 {
        format!("{:.1}M", downloads as f64 / 1_000_000.)
    } else if downloads >= 1_000 {
        format!("{:.1}K", downloads as f64 / 1_000.)
    } else {
        downloads.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hugging_face_next_link() {
        let header = "<https://huggingface.co/api/models?cursor=abc>; rel=\"next\"";
        assert_eq!(
            parse_next_link(header).as_deref(),
            Some("https://huggingface.co/api/models?cursor=abc")
        );
    }

    #[test]
    fn selects_only_onnx_files() {
        let files = vec![
            HubFile {
                rfilename: "weights.safetensors".into(),
            },
            HubFile {
                rfilename: "weights.onnx".into(),
            },
        ];
        assert_eq!(choose_model_file(&files).as_deref(), Some("weights.onnx"));
    }

    #[test]
    #[ignore]
    fn live_hugging_face_response_is_compatible() {
        let page = fetch_page(&search_url("upscal")).unwrap();
        assert!(!page.models.is_empty());
        assert!(page.next.is_some());
    }
}
