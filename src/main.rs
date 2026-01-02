use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const DEFAULT_SKIP_KEYWORDS: &[&str] = &[
    "Proton",
    "Steam Linux Runtime",
    "Steamworks",
    "Common Redistributables",
    "SteamVR",
    "Dedicated Server",
    "Soundtrack",
];

const DEFAULT_IGNORED_APP_IDS: &[&str] = &["480"];

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Run without writing files to disk, only discovering applications
    #[arg(short, long)]
    dry_run: bool,
    /// Path to Steam installation (defaults to ~/.local/share/Steam)
    #[arg(short, long)]
    steam_path: Option<String>,
    /// Path to applications directory (defaults to ~/.local/share/applications)
    #[arg(short, long)]
    app_dir: Option<String>,
    /// Comma separated list of keywords to skip (defaults to Proton,Steam Linux Runtime,Steamworks,Common Redistributables,SteamVR,Dedicated Server,Soundtrack)
    #[arg(short = 'k', long)]
    skip_keywords: Option<String>,
    /// Comma separated list of app IDs to skip (defaults to 480)
    #[arg(short, long)]
    ignored_app_ids: Option<String>,
}

struct GameInfo {
    appid: String,
    name: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let start_time = std::time::Instant::now();

    let ignored_keywords = match args.skip_keywords {
        Some(ref s) => s.split(',').map(|s| s.trim()).collect::<Vec<_>>(),
        None => DEFAULT_SKIP_KEYWORDS.to_vec(),
    };

    let ignored_app_ids = match args.ignored_app_ids {
        Some(ref s) => s.split(',').map(|s| s.trim()).collect::<Vec<_>>(),
        None => DEFAULT_IGNORED_APP_IDS.to_vec(),
    };

    let home = dirs::home_dir().context("Could not find home directory")?;

    let steam_root = match args.steam_path {
        Some(path) => PathBuf::from(path),
        None => {
            let home = dirs::home_dir().context("Could not find home directory")?;
            home.join(".local/share/Steam")
        }
    };

    let library_vdf = steam_root.join("steamapps/libraryfolders.vdf");
    let icon_cache_dir = steam_root.join("appcache/librarycache");

    let desktop_dir = match args.app_dir {
        Some(path) => PathBuf::from(path),
        None => home.join(".local/share/applications"),
    };

    println!("Steam Root Directory: {:?}", steam_root);
    println!("Desktop Entry Directory: {:?}", desktop_dir);
    println!("Icon Cache Directory: {:?}", icon_cache_dir);

    if args.dry_run {
        println!("----------------------------------");
        println!("DRY RUN ENABLED - No files will be written.");
        println!("----------------------------------");
    } else {
        fs::create_dir_all(&desktop_dir)?;

        println!("Cleaning up old Steam desktop entries...");

        for entry in fs::read_dir(&desktop_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                && filename.starts_with("steam-")
                && filename.ends_with(".desktop")
            {
                fs::remove_file(path)?;
            }
        }
    }

    if !library_vdf.exists() {
        eprintln!("Error: libraryfolders.vdf not found at {:?}", library_vdf);
        std::process::exit(1);
    }

    let libraries = parse_library_folders(&library_vdf)?;

    let mut created_count = 0;
    let mut skipped_count = 0;

    for lib_path in libraries {
        let steamapps = lib_path.join("steamapps");
        if !steamapps.exists() {
            continue;
        }

        println!("Checking Library: {:?}", lib_path);

        let entries = fs::read_dir(&steamapps)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // filter for appmanifest_*.acf
            if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                && filename.starts_with("appmanifest_")
                && filename.ends_with(".acf")
                && let Ok(game) = parse_app_manifest(&path)
            {
                if should_skip(&game.name, &game.appid, &ignored_app_ids, &ignored_keywords) {
                    println!("  Found Tool/Runtime, skipping: {}", game.name);
                    skipped_count += 1;
                    continue;
                }

                // idk how steam does the hash soooo this is good enough
                // 40 char hash + .jpg :pray:
                let found_icon = fs::read_dir(icon_cache_dir.join(&game.appid))
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| {
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.len() == 44 && s.ends_with(".jpg"))
                            .unwrap_or(false)
                    });

                let icon_path = match found_icon {
                    Some(path) => path.to_string_lossy().to_string(),
                    None => "steam".to_string(),
                };

                let desktop_filename = format!("steam-{}.desktop", game.appid);
                let desktop_file_path = desktop_dir.join(&desktop_filename);

                if args.dry_run {
                    println!("  Found game: {} (AppID: {})", game.name, game.appid);
                } else {
                    create_desktop_file(&desktop_file_path, &game, &icon_path)?;
                    println!("  Created Launcher for {}", game.name);
                }
                created_count += 1;
            }
        }
    }

    let elapsed = start_time.elapsed().as_millis();

    if args.dry_run {
        println!(
            "Dry run complete. Found {} games, skipped {} tools. Took {:.2?} milliseconds.",
            created_count, skipped_count, elapsed
        );
    } else {
        println!(
            "Done! {} shortcuts created (skipped {} tools) in {:?}. Took {:.2?} milliseconds.",
            created_count, skipped_count, desktop_dir, elapsed
        );
    }

    Ok(())
}

fn parse_library_folders(path: &Path) -> Result<Vec<PathBuf>> {
    let content = fs::read_to_string(path)?;
    let mut paths = Vec::new();

    let re = Regex::new(r#""path"\s+"([^"]+)""#).unwrap();

    for cap in re.captures_iter(&content) {
        if let Some(matched_path) = cap.get(1) {
            paths.push(PathBuf::from(matched_path.as_str()));
        }
    }

    Ok(paths)
}

fn parse_app_manifest(path: &Path) -> Result<GameInfo> {
    let content = fs::read_to_string(path)?;

    let re_id = Regex::new(r#""appid"\s+"(\d+)""#).unwrap();
    let re_name = Regex::new(r#""name"\s+"([^"]+)""#).unwrap();

    let appid = re_id
        .captures(&content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .context("Failed to find appid")?;

    let name = re_name
        .captures(&content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "Unknown Game".to_string());

    Ok(GameInfo { appid, name })
}

fn should_skip(
    name: &str,
    appid: &str,
    ignored_app_ids: &Vec<&str>,
    ignored_key_words: &Vec<&str>,
) -> bool {
    let name_lower = name.to_lowercase();

    if ignored_app_ids.contains(&appid) {
        return true;
    }

    for keyword in ignored_key_words {
        if name_lower.contains(&keyword.to_lowercase()) {
            return true;
        }
    }
    false
}

fn create_desktop_file(path: &Path, game: &GameInfo, icon_path: &str) -> Result<()> {
    let content = format!(
        "[Desktop Entry]\n\
        Name={}\n\
        Exec=steam steam://rungameid/{}\n\
        Icon={}\n\
        Terminal=false\n\
        Type=Application\n\
        Categories=Game;\n",
        game.name, game.appid, icon_path
    );

    let mut file = fs::File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}
