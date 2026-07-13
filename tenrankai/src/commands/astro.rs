use crate::Config;
use crate::config::ConfigStorageLoader;
use crate::gallery::Gallery;
use crate::storage;
use seiza::Wcs;
use std::sync::Arc;
use tenrankai_config_storage::{ConfigStorageUrl, create_config_storage};

/// Handle `astro regen`: reproject every persisted overlay in a gallery
/// through its stored WCS against the currently configured object catalog.
pub async fn handle_regen_command(
    config: Config,
    site_name: String,
    gallery_name: String,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let astro_config = config
        .astro
        .as_ref()
        .ok_or("astro is not configured. Add an [astro] section to config.toml.")?;
    let astro = crate::astro::AstroContext::load(astro_config)
        .ok_or("failed to load astro catalogs (see log)")?;

    let config_storage_url = config.app.config_storage.clone().ok_or(
        "config_storage is required. Add 'config_storage = \"config.d\"' to [app] section of config.toml.",
    )?;
    let storage_url = ConfigStorageUrl::parse(&config_storage_url)?;
    let config_storage = create_config_storage(&storage_url).await?;
    let loader = ConfigStorageLoader::new(config_storage, config.app.cookie_secret.clone());

    let site_config = loader
        .load_site(&site_name)
        .await?
        .ok_or_else(|| format!("Site '{}' not found", site_name))?;
    let gallery_config = site_config
        .galleries
        .as_ref()
        .and_then(|galleries| galleries.iter().find(|g| g.name == gallery_name).cloned())
        .ok_or_else(|| {
            format!(
                "Gallery '{}' not found in site '{}'",
                gallery_name, site_name
            )
        })?;

    let source_storage = storage::create_storage_from_url(&gallery_config.source_directory).await?;
    let cache_storage = storage::create_storage_from_url(&gallery_config.cache_directory).await?;
    let gallery = Arc::new(Gallery::new(gallery_config, source_storage, cache_storage));

    let paths = gallery.user_metadata_storage.list_all().await?;
    let mut refreshed = 0usize;
    let mut current = 0usize;
    for path in paths {
        let Ok(Some(metadata)) = gallery.user_metadata_storage.load(&path).await else {
            continue;
        };
        let Some(solution) = metadata.astro else {
            continue;
        };
        if solution.objects_version == astro.objects_version() {
            current += 1;
            continue;
        }
        let wcs = Wcs {
            crval: (solution.crval[0], solution.crval[1]),
            crpix: (solution.crpix[0], solution.crpix[1]),
            cd: solution.cd,
        };
        let mut updated = solution.clone();
        updated.objects =
            crate::astro::placed_objects(&astro, &wcs, (solution.width, solution.height));
        updated.objects_version = astro.objects_version().to_string();
        println!(
            "{path}: {} -> {} objects (version {} -> {}){}",
            solution.objects.len(),
            updated.objects.len(),
            solution.objects_version,
            updated.objects_version,
            if dry_run { " [dry run]" } else { "" },
        );
        if !dry_run {
            gallery
                .user_metadata_storage
                .save_astro(&path, Some(&updated))
                .await?;
        }
        refreshed += 1;
    }
    println!(
        "{refreshed} solution(s) {}, {current} already current",
        if dry_run {
            "would refresh"
        } else {
            "refreshed"
        },
    );
    Ok(())
}
