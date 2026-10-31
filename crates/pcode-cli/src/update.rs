//! `pcode actualizar`: descarga la última release de GitHub, verifica su SHA-256 y sustituye el
//! binario en marcha.
use anyhow::{Context, bail};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Repositorio de las releases (`PCODE_REPO` para usar un fork).
fn repo() -> String {
    std::env::var("PCODE_REPO").unwrap_or_else(|_| "jlasherasTracasa/PublicCode".into())
}

/// Sufijo del archivo de la release para esta plataforma.
pub fn asset_suffix(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("-x86_64-pc-windows-msvc.zip"),
        ("linux", "x86_64") => Some("-x86_64-unknown-linux-gnu.tar.gz"),
        _ => None,
    }
}

/// Busca la suma de `name` en un `SHA256SUMS` (`<hex>  <archivo>`).
pub fn checksum_for<'a>(sums: &'a str, name: &str) -> Option<&'a str> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, f) = (it.next()?, it.next()?);
        (f.trim_start_matches('*') == name).then_some(h)
    })
}

/// `true` si `latest` (p. ej. `v0.4.0`) es más nueva que `current` (`0.3.0`).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .map(|x| x.parse().unwrap_or(0))
            .collect()
    };
    parse(latest) > parse(current)
}

async fn get(client: &reqwest::Client, url: &str) -> anyhow::Result<reqwest::Response> {
    let r = client
        .get(url)
        .header("User-Agent", "pcode")
        .send()
        .await
        .with_context(|| format!("no se pudo descargar {url}"))?;
    if !r.status().is_success() {
        bail!("{url}: HTTP {}", r.status());
    }
    Ok(r)
}

fn extract(archive: &Path, into: &Path) -> anyhow::Result<()> {
    // `tar` viene con Linux y con Windows 10/11 (bsdtar, que también abre .zip).
    let st = std::process::Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .status()
        .context("no se pudo ejecutar tar")?;
    if !st.success() {
        bail!("tar no pudo descomprimir {}", archive.display());
    }
    Ok(())
}

fn find_binary(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_binary(&p, name) {
                return Some(f);
            }
        } else if p.file_name().is_some_and(|n| n == name) {
            return Some(p);
        }
    }
    None
}

/// Actualiza `pcode`. Con `check` solo dice si hay versión nueva.
pub async fn update(check: bool) -> anyhow::Result<i32> {
    let current = env!("CARGO_PKG_VERSION");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    let api = format!("https://api.github.com/repos/{}/releases/latest", repo());
    let rel: serde_json::Value = get(&client, &api).await?.json().await?;
    let tag = rel["tag_name"]
        .as_str()
        .context("la release no tiene tag_name")?;
    if !is_newer(tag, current) {
        println!("pcode {current} ya es la última versión.");
        return Ok(0);
    }
    if check {
        println!("Hay una versión nueva: {tag} (tienes {current}). Ejecuta `pcode actualizar`.");
        return Ok(0);
    }
    let suffix = asset_suffix(std::env::consts::OS, std::env::consts::ARCH).with_context(|| {
        format!(
            "no hay binarios para {} {}: compila con `cargo install --path crates/pcode-cli`",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    let name = format!("pcode-{tag}{suffix}");
    let base = format!("https://github.com/{}/releases/download/{tag}", repo());
    println!("Descargando {name}…");
    let sums = get(&client, &format!("{base}/SHA256SUMS"))
        .await?
        .text()
        .await?;
    let expected = checksum_for(&sums, &name)
        .with_context(|| format!("SHA256SUMS no incluye {name}"))?
        .to_lowercase();
    let bytes = get(&client, &format!("{base}/{name}"))
        .await?
        .bytes()
        .await?;
    let got = hex::encode(Sha256::digest(&bytes));
    if got != expected {
        bail!("la suma SHA-256 de {name} no coincide: se esperaba {expected} y es {got}");
    }
    let tmp = std::env::temp_dir().join(format!("pcode-update-{}", std::process::id()));
    std::fs::create_dir_all(&tmp)?;
    let archive = tmp.join(&name);
    std::fs::write(&archive, &bytes)?;
    extract(&archive, &tmp)?;
    let exe_name = if cfg!(windows) { "pcode.exe" } else { "pcode" };
    let new = find_binary(&tmp, exe_name).context("el archivo no contiene pcode")?;
    let me = std::env::current_exe()?;
    let dir = me.parent().context("ruta del ejecutable sin carpeta")?;
    // Se copia junto al actual y se renombra: el binario en marcha sigue funcionando.
    let staged = dir.join(format!("{exe_name}.nuevo"));
    std::fs::copy(&new, &staged)
        .with_context(|| format!("no se puede escribir en {} (¿permisos?)", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
    }
    if cfg!(windows) {
        // Windows no deja sobrescribir un .exe en marcha, pero sí renombrarlo.
        let old = dir.join("pcode.antiguo.exe");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&me, &old)?;
    }
    std::fs::rename(&staged, &me)?;
    let _ = std::fs::remove_dir_all(&tmp);
    println!("Actualizado: pcode {current} → {tag}");
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elige_el_archivo_de_la_plataforma() {
        assert_eq!(
            asset_suffix("windows", "x86_64"),
            Some("-x86_64-pc-windows-msvc.zip")
        );
        assert_eq!(
            asset_suffix("linux", "x86_64"),
            Some("-x86_64-unknown-linux-gnu.tar.gz")
        );
        assert_eq!(asset_suffix("macos", "aarch64"), None);
    }

    #[test]
    fn lee_sha256sums_y_compara_versiones() {
        let s = "abc  pcode-v1.0.0-x86_64-unknown-linux-gnu.tar.gz\ndef *pcode-v1.0.0-x86_64-pc-windows-msvc.zip\n";
        assert_eq!(
            checksum_for(s, "pcode-v1.0.0-x86_64-pc-windows-msvc.zip"),
            Some("def")
        );
        assert_eq!(checksum_for(s, "otro"), None);
        assert!(is_newer("v0.10.0", "0.9.3"));
        assert!(!is_newer("v0.3.0", "0.3.0"));
        assert!(!is_newer("v0.2.9", "0.3.0"));
    }
}
