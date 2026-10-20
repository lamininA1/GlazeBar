#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(
    unreachable_patterns,
    unused_variables,
    dangling_pointers_from_temporaries
)]
mod go_sort;
mod go_upper_table;
mod graphics;
mod hooks;
mod ipc;
mod online_preferences;
mod online_ui;
mod palette_base;
mod palette_extra;
mod palette_mono;
mod palette_styles;
mod pixel_sampler;
mod quantize;
mod style_compare_ui;
mod style_compare_worker;
mod theme;
mod theme_cache;
mod theme_extract;
mod theme_preview_ui;
mod theme_settings;
mod theme_text;
mod theme_transaction;
mod theme_windows;
mod ui_cache;
mod wallhaven;
mod wallpaper;
mod wallpaper_actions;
mod wallpaper_ui;
mod wallpaper_worker;
mod windows_options_ui;
fn root() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap();
    let mut dir = exe.parent().unwrap();
    loop {
        if dir.join("config.yaml").exists() && dir.join("icons").is_dir() {
            return dir.to_owned();
        }
        if let Some(parent) = dir.parent() {
            dir = parent
        } else {
            panic!("GlazeBar project directory not found")
        }
    }
}
fn main() {
    if let Err(e) = run() {
        let theme_alias = std::env::current_exe().ok().is_some_and(|p| {
            p.file_stem()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("GlazeTheme"))
        });
        if theme_alias || std::env::args().any(|a| a == "--theme-json") {
            let _ = serde_json::to_writer(
                std::io::stdout(),
                &serde_json::json!({"error":format!("{e:#}")}),
            );
        }
        eprintln!("{e:#}");
        let _ = std::fs::write(root().join("rust-error.log"), format!("{e:#}"));
        std::process::exit(1)
    }
}
fn run() -> anyhow::Result<()> {
    let root = root();
    let mut args: Vec<String> = std::env::args().collect();
    if std::env::current_exe()?
        .file_stem()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("GlazeTheme"))
    {
        args.insert(1, "--theme-json".into());
    }
    match args.get(1).map(String::as_str) {
        Some("--probe-wallhaven") => {
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let results = wallhaven::search(
                args.get(2).map(String::as_str).unwrap_or("landscape"),
                1,
                &cancel,
            )?;
            let item = results
                .wallpapers
                .first()
                .ok_or_else(|| anyhow::anyhow!("No search results"))?;
            let thumbnail = wallhaven::thumbnail(&item.thumbs.small, &cancel)?;
            thumbnail.save(root.join("rust-wallhaven-thumbnail.png"))?;
            let report = serde_json::json!({"results":results.wallpapers.len(),"page":results.page,"lastPage":results.last_page,"first":item,"thumbnailWidth":thumbnail.width(),"thumbnailHeight":thumbnail.height()});
            std::fs::write(
                root.join("rust-wallhaven-probe.json"),
                serde_json::to_vec_pretty(&report)?,
            )?;
            serde_json::to_writer_pretty(std::io::stdout(), &report)?;
        }
        Some("--probe-wallpaper") => {
            let home = std::path::PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .ok_or_else(|| anyhow::anyhow!("USERPROFILE missing"))?,
            );
            let folder = wallpaper::folder(&root, &home);
            let current = wallpaper::current()?;
            let fallback = std::path::PathBuf::from(
                std::env::var_os("SystemRoot")
                    .ok_or_else(|| anyhow::anyhow!("SystemRoot missing"))?,
            )
            .join("Web/Wallpaper");
            serde_json::to_writer_pretty(
                std::io::stdout(),
                &serde_json::json!({"folder":folder,"current":current,"files":wallpaper::files(&folder,&fallback,&current)}),
            )?;
        }
        Some("--theme-extract") => {
            let request: theme_transaction::Request = serde_json::from_reader(std::io::stdin())?;
            let palette = theme_extract::extract(
                std::path::Path::new(&request.image),
                request.light,
                &request.mode,
            )?;
            serde_json::to_writer_pretty(
                std::io::stdout(),
                &serde_json::json!({"palette":palette}),
            )?;
        }
        Some("--probe-palette-colors") => {
            #[derive(serde::Deserialize)]
            struct Input {
                colors: Vec<String>,
                weights: Option<Vec<f64>>,
                #[serde(default)]
                light: bool,
                mode: Option<String>,
            }
            let input: Input = serde_json::from_reader(std::io::stdin())?;
            serde_json::to_writer_pretty(
                std::io::stdout(),
                &palette_mono::generate(
                    &input.colors,
                    input.weights.as_deref(),
                    input.light,
                    input.mode.as_deref().unwrap_or("normal"),
                )?,
            )?;
        }
        Some("--probe-dominant") => {
            let path = args
                .get(2)
                .ok_or_else(|| anyhow::anyhow!("Image path required"))?;
            let pixels = pixel_sampler::load(std::path::Path::new(path))?;
            serde_json::to_writer_pretty(
                std::io::stdout(),
                &serde_json::json!({"sampleCount":pixels.len(),"colors":quantize::dominant(&pixels,48)?}),
            )?;
        }
        Some("--quantize-pixels") => {
            let pixels: Vec<[f64; 3]> = serde_json::from_reader(std::io::stdin())?;
            serde_json::to_writer_pretty(std::io::stdout(), &quantize::dominant(&pixels, 48)?)?;
        }
        Some("--theme-files" | "--theme-json") => {
            let request: theme_transaction::Request = serde_json::from_reader(std::io::stdin())?;
            let home = args
                .windows(2)
                .find(|w| w[0] == "--home")
                .map(|w| std::path::PathBuf::from(&w[1]))
                .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from))
                .ok_or_else(|| anyhow::anyhow!("Home directory not found"))?;
            serde_json::to_writer_pretty(
                std::io::stdout(),
                &theme_transaction::run(&home, &request)?,
            )?;
        }
        Some("--theme-export-preview") => {
            let request: serde_json::Value = serde_json::from_reader(std::io::stdin())?;
            let colors: Vec<String> = serde_json::from_value(request["palette"].clone())?;
            let mut result =
                theme::export_preview(&colors, request["light"].as_bool().unwrap_or(false))?;
            let palette = theme::readable(&colors)?;
            if let Some(source) = request["codexSettings"].as_str() {
                result["codexSettings"] = serde_json::json!(theme_text::codex_settings(source));
            }
            if let Some(source) = request["glazewmConfig"].as_str() {
                result["glazewmConfig"] = serde_json::json!(theme_text::focused_border(
                    source,
                    &theme::accent(&palette, request["light"].as_bool().unwrap_or(false))
                )?);
            }
            if let Some(source) = request["terminalSettings"].as_str() {
                result["terminalSettings"] =
                    serde_json::json!(theme_settings::terminal_settings(source, &palette)?);
            }
            if let Some(source) = request["vscodeSettings"].as_str() {
                result["vscodeSettings"] =
                    serde_json::json!(theme_settings::vscode_settings(source, &palette)?);
            }
            serde_json::to_writer_pretty(std::io::stdout(), &result)?;
        }
        Some("--validate-hooks") => {
            hooks::validate()?;
            std::fs::write(
                root.join("rust-hooks-validation.json"),
                r#"{"observeOnly":true,"installed":true,"released":true}"#,
            )?;
        }
        Some("--shadow-sheet") => {
            runtime::run_with_popup(root.clone(), Some(5), Some(views::Action::Sheet))?
        }
        Some("--shadow-wallpaper") => runtime::run_with_popup(
            root.clone(),
            Some(args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10)),
            Some(views::Action::Wallpaper),
        )?,
        Some(
            arg @ ("--stop" | "--toggle" | "--bar" | "--sheet" | "--wallpaper" | "--resync"
            | "--snapshot" | "--close-popups"),
        ) => runtime::signal(&root, arg)?,
        Some("--probe-weather") => {
            let value = weather::fetch()?;
            println!("{value:?}");
        }
        Some("--validate-brightness-worker") => {
            let names = ipc::Client::new()
                .model()?
                .monitors
                .into_iter()
                .map(|m| m.name)
                .collect();
            let result = brightness::validate_worker(names)?;
            std::fs::write(
                root.join("rust-brightness-worker-validation.json"),
                serde_json::to_vec_pretty(&result)?,
            )?;
            println!("{result}");
        }
        Some("--validate-native") => native::validate(root.clone())?,
        Some("--render-wallhaven") => reference::render_wallhaven(&root)?,
        Some("--probe-brightness") => {
            let names = ipc::Client::new()
                .model()?
                .monitors
                .into_iter()
                .map(|m| m.name)
                .collect::<Vec<_>>();
            let result = brightness::probe(&names, args.iter().any(|v| v == "--same-value"))?;
            std::fs::write(
                root.join("rust-brightness-probe.json"),
                serde_json::to_vec_pretty(&result)?,
            )?;
            println!("{result}");
        }
        Some("--probe") => {
            println!("availableWiFiNetworks={}", wifi::scan(false)?.len());
            let m = ipc::Client::new().model()?;
            let mut s = sensors::Sensors::default();
            sensors::Reader::new().read(&mut s, true);
            println!(
                "monitors={} sensors={}",
                m.monitors.len(),
                serde_json::to_string(&s)?
            );
            for mon in &m.monitors {
                let scale = display::read(&mon.name)?;
                println!("{} scale={}", mon.name, views::SCALES[scale.current]);
            }
        }
        Some("--shadow-wifi") => {
            runtime::run_with_popup(root.clone(), Some(8), Some(views::Action::Wifi))?
        }
        Some("--render") => {
            reference::render(&root)?;
            reference::compare(&root)?;
        }
        Some("--preview") => native::preview(
            root.clone(),
            args.get(2).and_then(|v| v.parse().ok()).unwrap_or(0),
        )?,
        Some("--shadow") => runtime::run(
            root.clone(),
            Some(args.get(2).and_then(|v| v.parse().ok()).unwrap_or(10)),
        )?,
        None | Some("--resident") => runtime::run(root.clone(), None)?,
        Some(argument) => anyhow::bail!("Unknown argument: {argument}"),
    };
    Ok(())
}

mod sensors;
mod views;

mod display;
mod reference;

mod native;

mod brightness;
mod power;
mod precache;
mod runtime;
mod sheet;
mod weather;
mod wifi;
mod wifi_ui;
