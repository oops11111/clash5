use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde_json::Value;
use std::{collections::HashMap, fs, io::Read, path::Path};

const REPOSITORY: &str = "https://github.com/oops11111/clash5";
const INHERITED_UPDATER_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEQyOEMyRjBCQkVGOUJEREYKUldUZnZmbStDeStNMHU5Mmo1N24xQXZwSVRYbXA2NUpzZE5oVzlqeS9Bc0t6RVV4MmtwVjBZaHgK";
const REQUIRED_PLATFORMS: [(&str, &str); 4] = [
    ("windows-x86_64", "x64-setup.exe"),
    ("windows-aarch64", "arm64-setup.exe"),
    ("darwin-x86_64", "x64.app.tar.gz"),
    ("darwin-aarch64", "aarch64.app.tar.gz"),
];

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key].as_str().with_context(|| format!("missing {key}"))
}

fn json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}

fn decode(value: &str) -> Result<String> {
    Ok(String::from_utf8(
        base64::engine::general_purpose::STANDARD.decode(value)?,
    )?)
}

fn validate_asset<'a>(assets: &'a HashMap<&str, &Value>, directory: &Path, tag: &str, name: &str) -> Result<&'a Value> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)),
        "unsafe asset name: {name}"
    );
    let asset = assets
        .get(name)
        .with_context(|| format!("missing release asset {name}"))?;
    let expected_url = format!("{REPOSITORY}/releases/download/{tag}/{name}");
    ensure!(
        field(asset, "browser_download_url")? == expected_url,
        "wrong download URL for {name}"
    );
    let size = asset["size"]
        .as_u64()
        .with_context(|| format!("missing size for {name}"))?;
    ensure!(size > 0, "empty release asset {name}");
    let actual = fs::metadata(directory.join(name))
        .with_context(|| format!("missing downloaded asset {name}"))?
        .len();
    ensure!(
        actual == size,
        "size mismatch for {name}: release says {size}, file has {actual}"
    );
    Ok(asset)
}

fn verify_release(release_path: &Path, directory: &Path, package_path: &Path, config_path: &Path) -> Result<()> {
    let release = json(release_path)?;
    let package = json(package_path)?;
    let config = json(config_path)?;
    let version = Version::parse(field(&package, "version")?)?;
    ensure!(
        version.build.is_empty(),
        "release version must not contain build metadata"
    );
    let tag = format!("v{version}");
    ensure!(
        field(&release, "tag_name")? == tag,
        "release tag differs from package version"
    );
    ensure!(release["draft"] == true, "release is already published");
    ensure!(
        release["prerelease"] == !version.pre.is_empty(),
        "prerelease channel mismatch"
    );
    ensure!(
        field(&release, "name")? == format!("clash5 {tag}"),
        "wrong release name"
    );
    ensure!(
        field(&release, "html_url")? == format!("{REPOSITORY}/releases/tag/{tag}"),
        "wrong release URL"
    );

    let mut assets = HashMap::new();
    for asset in release["assets"].as_array().context("missing release assets")? {
        let name = field(asset, "name")?;
        ensure!(assets.insert(name, asset).is_none(), "duplicate release asset {name}");
    }

    let manifest_name = if version.pre.is_empty() {
        "latest.json"
    } else {
        "latest-test.json"
    };
    validate_asset(&assets, directory, &tag, manifest_name)?;
    let manifest = json(&directory.join(manifest_name))?;
    ensure!(
        field(&manifest, "version")? == version.to_string(),
        "manifest version mismatch"
    );
    let platforms = manifest["platforms"]
        .as_object()
        .context("missing manifest platforms")?;
    ensure!(
        platforms.len() == REQUIRED_PLATFORMS.len(),
        "manifest contains an unsupported platform"
    );
    let updater_config = &config["plugins"]["updater"];
    ensure!(
        updater_config["requireSignedVersion"] == true,
        "enable requireSignedVersion before release validation"
    );
    let key = field(updater_config, "pubkey")?;
    ensure!(
        key != INHERITED_UPDATER_KEY,
        "replace the inherited updater public key before release validation"
    );
    let public_key = PublicKey::decode(&decode(key)?)?;

    for (platform, suffix) in REQUIRED_PLATFORMS {
        let entry = platforms
            .get(platform)
            .with_context(|| format!("missing {platform} updater"))?;
        let prefix = format!("{REPOSITORY}/releases/download/{tag}/");
        let url = field(entry, "url")?;
        let name = url
            .strip_prefix(&prefix)
            .with_context(|| format!("wrong updater repository or tag for {platform}"))?;
        ensure!(
            name.starts_with("clash5_") && name.contains(&version.to_string()) && name.ends_with(suffix),
            "wrong updater asset for {platform}: {name}"
        );
        validate_asset(&assets, directory, &tag, name)?;
        let sidecar = format!("{name}.sig");
        validate_asset(&assets, directory, &tag, &sidecar)?;
        let encoded_signature = field(entry, "signature")?;
        ensure!(
            fs::read_to_string(directory.join(&sidecar))?.trim() == encoded_signature,
            "signature sidecar mismatch for {platform}"
        );
        let signature = Signature::decode(&decode(encoded_signature)?)?;
        let signed_version = signature
            .trusted_comment()
            .split('\t')
            .find_map(|part| part.strip_prefix("version:"));
        ensure!(
            signed_version == Some(version.to_string().as_str()),
            "signature version mismatch for {platform}"
        );
        let mut verifier = public_key.verify_stream(&signature)?;
        let mut file = fs::File::open(directory.join(name))?;
        let mut buffer = [0u8; 65536];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            verifier.update(&buffer[..count]);
        }
        verifier
            .finalize()
            .with_context(|| format!("invalid updater signature for {platform}"))?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    ensure!(
        args.len() == 5,
        "usage: clash5-release-gate <release.json> <assets-dir> <package.json> <tauri.conf.json>"
    );
    verify_release(
        Path::new(&args[1]),
        Path::new(&args[2]),
        Path::new(&args[3]),
        Path::new(&args[4]),
    )?;
    println!("clash5 draft metadata, assets, and updater signatures verified");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDZDMkFFRTJDOTk4NjAxOUIKUldTYkFZYVpMTzRxYkZZU3ZjVlNqSWp4UlprdmJDN1dnSGF3dkNiZXBvVTNjMWVaNjh1ZG9mSmoK";
    const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTYkFZYVpMTzRxYkVWUCtqVU9uNTdBVFlycFlzK2lHOWJOUGkzV1hCZW9FeTZqRzNtUDc0VU4zcnB0WHI3TGxySjN2bE9qOTRaT1puTEFjT3ljOFpkMXh3enl2T3R6bmdrPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkyMzgwMzQ4CWZpbGU6cGF5bG9hZC5iaW4JdmVyc2lvbjoyLjUuMTAKaXB0QzZXSkYyc2Rid0Z2djdDbXZHazA5UWdTM2R5L2xLTzBQTDB2RTVkNStBUllnaU5wTXpiT1RkMDVPL2d3LzFtUWRuOVpGS0RINXVNOUcvQVdDQVE9PQo=";
    const PAYLOAD: &[u8] = b"clash5 release gate fixture";

    struct Fixture {
        root: std::path::PathBuf,
        release: Value,
        manifest: Value,
        first_asset: String,
    }

    impl Fixture {
        fn new() -> Self {
            let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let root = std::env::temp_dir().join(format!("clash5-release-gate-{}-{nonce}", std::process::id()));
            fs::create_dir(&root).unwrap();
            let tag = "v2.5.10";
            let mut assets = Vec::new();
            let mut platforms = serde_json::Map::new();
            let mut first_asset = String::new();
            for (platform, suffix) in REQUIRED_PLATFORMS {
                let name = format!("clash5_2.5.10_{suffix}");
                if first_asset.is_empty() {
                    first_asset = name.clone();
                }
                let url = format!("{REPOSITORY}/releases/download/{tag}/{name}");
                fs::write(root.join(&name), PAYLOAD).unwrap();
                assets.push(json!({"name": name, "size": PAYLOAD.len(), "browser_download_url": url}));
                let sidecar = format!("{name}.sig");
                fs::write(root.join(&sidecar), SIGNATURE).unwrap();
                assets.push(
                    json!({"name": sidecar, "size": SIGNATURE.len(), "browser_download_url": format!("{url}.sig")}),
                );
                platforms.insert(platform.to_string(), json!({"url": url, "signature": SIGNATURE}));
            }
            let manifest = json!({"version": "2.5.10", "platforms": platforms});
            let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
            fs::write(root.join("latest.json"), &manifest_bytes).unwrap();
            assets.push(json!({"name": "latest.json", "size": manifest_bytes.len(), "browser_download_url": format!("{REPOSITORY}/releases/download/{tag}/latest.json")}));
            fs::write(root.join("package.json"), r#"{"version":"2.5.10"}"#).unwrap();
            fs::write(
                root.join("tauri.conf.json"),
                serde_json::to_vec(&json!({"plugins":{"updater":{"pubkey":PUBKEY,"requireSignedVersion":true}}}))
                    .unwrap(),
            )
            .unwrap();
            let release = json!({
                "tag_name": tag,
                "name": format!("clash5 {tag}"),
                "draft": true,
                "prerelease": false,
                "html_url": format!("{REPOSITORY}/releases/tag/{tag}"),
                "assets": assets,
            });
            Self {
                root,
                release,
                manifest,
                first_asset,
            }
        }

        fn verify(&self) -> Result<()> {
            fs::write(self.root.join("release.json"), serde_json::to_vec(&self.release)?)?;
            fs::write(self.root.join("latest.json"), serde_json::to_vec(&self.manifest)?)?;
            verify_release(
                &self.root.join("release.json"),
                &self.root,
                &self.root.join("package.json"),
                &self.root.join("tauri.conf.json"),
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let (Ok(root), Ok(temp)) = (self.root.canonicalize(), std::env::temp_dir().canonicalize())
                && root.parent() == Some(temp.as_path())
                && root
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("clash5-release-gate-"))
            {
                let _ = fs::remove_dir_all(root);
            }
        }
    }

    #[test]
    fn draft_requires_owned_assets_and_valid_version_bound_signatures() {
        let mut fixture = Fixture::new();
        fixture.verify().unwrap();

        fixture.release["draft"] = false.into();
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fixture.release["tag_name"] = "v2.5.11".into();
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fixture.release["prerelease"] = true.into();
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fixture.release["html_url"] = "https://github.com/other/repo/releases/tag/v2.5.10".into();
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fixture.manifest["version"] = "2.5.11".into();
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fixture.release["assets"]
            .as_array_mut()
            .unwrap()
            .retain(|asset| asset["name"] != fixture.first_asset);
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fs::remove_file(fixture.root.join(format!("{}.sig", fixture.first_asset))).unwrap();
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fs::write(fixture.root.join(&fixture.first_asset), b"clash5 release gate fixturE").unwrap();
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fixture.manifest["platforms"]["windows-x86_64"]["url"] =
            "https://github.com/other/repo/releases/download/v2.5.10/clash5_2.5.10_x64-setup.exe".into();
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fixture.manifest["platforms"]["windows-x86_64"]["signature"] = "wrong-signature".into();
        assert!(fixture.verify().is_err());
        fixture = Fixture::new();

        fixture.manifest["platforms"]["linux-x86_64"] = fixture.manifest["platforms"]["windows-x86_64"].clone();
        assert!(fixture.verify().is_err());
    }
}
