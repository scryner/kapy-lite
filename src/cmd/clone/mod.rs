use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, offset::LocalResult};
use console::style;
use regex::Regex;
use statistics::CopyStatistics;
use tokio::fs;
use walkdir::{DirEntry, WalkDir};

use crate::{
    config::Config,
    drive::{
        GoogleDrive,
        auth::{GoogleAuthenticator, ListenPort},
    },
    gps::{GpsSearch, GpxStorage, NoopGpsSearch},
    image::{
        copy::{CopyResult, copy_with_inspection},
        inspect::{Inspection, inspect_image_from_path},
    },
    progress::{PanelType, Progress, Update},
};

mod statistics;

pub async fn do_clone(
    conf: Config,
    cred_path: &Path,
    ignore_geotag: bool,
    dry_run: bool,
    after: Option<String>,
) -> Result<()> {
    // print info
    let import_from = conf.import_from().to_str().unwrap();
    let import_to = conf.import_to().to_str().unwrap();
    println!(
        "Cloning from {} to {}...\n",
        style(import_from).bold().cyan(),
        style(import_to).bold().green()
    );

    // check path existence
    check_directory(conf.import_from()).map_err(|e| anyhow!("Invalid 'from' directory: {}", e))?;
    check_directory(conf.import_to()).map_err(|e| anyhow!("Invalid 'to' directory: {}", e))?;

    // inspect images
    let inspections = do_inspect_import_from(conf.import_from(), conf.import_to(), after).await?;

    // calculate first date and end date among import files
    let (oldest_created_at, most_recent_created_at) =
        match oldest_and_most_recent_taken_at(&inspections) {
            Ok((oldest, most_recent)) => (oldest, most_recent),
            Err(e) => {
                return Err(anyhow!(
                    "Failed to find oldest and most recent files: {}",
                    e
                ));
            }
        };

    // make gps search trait object
    let gpx = make_gps_search(
        ignore_geotag,
        oldest_created_at,
        most_recent_created_at,
        cred_path,
    )
    .await?;

    // process clone
    do_copy_with_inspections(&inspections, conf.import_to(), gpx, dry_run).await?;

    Ok(())
}

async fn calculate_to_be_import_after(
    after: Option<String>,
    out_dir: &Path,
) -> Result<Option<SystemTime>> {
    match after {
        Some(after) => {
            // valid: YYYY or YYYY-MM-DD or YYYY-MM
            match system_time_from_str(&after) {
                Ok(t) => Ok(Some(t)),
                Err(_) => Err(anyhow!(
                    "Invalid time format: YYYY-MM-DD or YYYY-MM or YYYY are valid"
                )),
            }
        }
        None => match to_be_imported_after(out_dir).await {
            Ok(t) => Ok(t),
            Err(e) => Err(anyhow!(
                "Failed to determine date and time to be imported after: {}",
                e
            )),
        },
    }
}

async fn do_inspect_import_from(
    import_from: &Path,
    import_to: &Path,
    after: Option<String>,
) -> Result<Vec<Inspection>> {
    // calculate when to copy started (since the last save to 'conf.to_path')
    let to_be_import_after = calculate_to_be_import_after(after, import_to).await?;

    // get to import files
    let import_entries = import_entries(import_from);

    // filter import files to retrieve
    let import_entries = match to_be_import_after {
        Some(t) => import_entries
            .into_iter()
            .filter(|entry| {
                let entry_created_at = entry.metadata().unwrap().created().unwrap();
                entry_created_at > t
            })
            .collect(),
        None => import_entries,
    };

    // print inspecting message
    println!(
        "{} {}",
        style("Inspecting").green().bold(),
        import_from.to_str().unwrap()
    );
    let progress = Progress::new(vec![
        PanelType::Bar("files_bar".to_string(), import_entries.len() as u64),
        PanelType::Message("state".to_string()),
    ]);

    let mut inspections = Vec::new();
    let mut inspection_failed = 0;

    for entry in import_entries.iter() {
        progress.update("files_bar", Update::Incr(None));

        let path = entry.path();
        let path_str = path.to_str().unwrap(); // never failed
        progress.update(
            "state",
            Update::Incr(Some(format!("{}: inspecting...", style(path_str).bold()))),
        );

        match inspect_image_from_path(path).await {
            Ok(inspection) => {
                inspections.push(inspection);
            }
            Err(e) => {
                eprintln!("Failed to inspection image '{}': {}", path_str, e);
                inspection_failed += 1;
            }
        };
    }

    // print inspecting summary
    progress.finish_all();
    progress.println(format!(
        "{:>5} files are inspected ({} total / {} succeed / {} failed)",
        style(inspections.len()).cyan().bold(),
        style(import_entries.len()).green(),
        style(inspections.len()).cyan(),
        if inspection_failed > 0 {
            style(inspection_failed).red()
        } else {
            style(0).dim()
        }
    ));
    progress.clear();

    Ok(inspections)
}

async fn do_copy_with_inspections(
    inspections: &Vec<Inspection>,
    out_dir: &Path,
    gpx: Arc<dyn GpsSearch>,
    dry_run: bool,
) -> Result<()> {
    let mut statistics = CopyStatistics::new();
    let mut errors = Vec::new();

    // print progress info
    println!("{} {}", style("Cloning").green().bold(), out_dir.display());
    let progress = Progress::new(vec![
        PanelType::Bar("files_bar".to_string(), inspections.len() as u64),
        PanelType::Message("state".to_string()),
    ]);

    // actual do copy with inspections
    for inspection in inspections.iter() {
        progress.update("files_bar", Update::Incr(None));

        let copy_res = match copy_with_inspection(
            &inspection.path,
            out_dir,
            inspection,
            Arc::clone(&gpx),
            dry_run,
        )
        .await
        {
            Ok(res) => res,
            Err(e) => {
                statistics.error += 1;
                errors.push((inspection, e));
                continue;
            }
        };

        match copy_res {
            CopyResult::Copied => statistics.copied += 1,
            CopyResult::CopiedWithAddingGpsInfo => statistics.copied_with_adding_gps_info += 1,
            CopyResult::Skipped => statistics.skipped += 1,
        }
    }

    progress.finish_all();
    progress.clear();

    // print statistics
    statistics.print_with_error(&errors);

    Ok(())
}

async fn make_gps_search(
    ignore_geotag: bool,
    oldest_created_at: SystemTime,
    most_recent_created_at: SystemTime,
    cred_path: &Path,
) -> Result<Arc<dyn GpsSearch>> {
    if ignore_geotag {
        return Ok(Arc::new(NoopGpsSearch));
    }

    // adjust time to more flexibility (+ 1 hour)
    let start = oldest_created_at - Duration::from_secs(3600);
    let end = most_recent_created_at + Duration::from_secs(3600);

    // make a progress
    println!(
        "{} from google drive: {} ~ {}",
        style("Preparing GPX").green().bold(),
        style(start.to_string()).cyan(),
        style(end.to_string()).cyan()
    );
    let progress = Progress::new(vec![PanelType::Message("gpx_filename".to_string())]);

    // initialize google drive
    let mut count = 0;

    let auth = GoogleAuthenticator::new(ListenPort::DefaultPort, cred_path)?;
    let drive = GoogleDrive::new(auth);

    const DEFAULT_MAX_SEARCH_FILES_ON_GOOGLE_DRIVE: usize = 100;
    const DEFAULT_GPS_MATCH_WITHIN: Duration = Duration::from_secs(5 * 60); // match within 5 min

    match GpxStorage::from_google_drive(
        &drive,
        start,
        end,
        DEFAULT_MAX_SEARCH_FILES_ON_GOOGLE_DRIVE,
        DEFAULT_GPS_MATCH_WITHIN,
        |filename| {
            progress.update(
                "gpx_filename",
                Update::Incr(Some(format!(
                    "{} is downloading and pouring...",
                    style(filename).bold()
                ))),
            );
            count += 1;
        },
    )
    .await
    {
        Ok(search) => {
            progress.finish_all();
            progress.println(format!(
                "{:>5} gpx files are retrieved",
                style(count).cyan().bold()
            ));
            progress.clear();

            Ok(Arc::new(search))
        }
        Err(e) => Err(anyhow!(
            "Failed to initialize geotag search on your google drive: {}",
            e
        )),
    }
}

fn check_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("Directory '{}' does not exist", path.display()));
    } else if !path.is_dir() {
        return Err(anyhow!("Path '{}' is not a directory", path.display()));
    }
    Ok(())
}

fn system_time_from_str(s: &str) -> Result<SystemTime> {
    let re_only_year = Regex::new(RE_ONLY_YEAR)?;
    let re_year_month = Regex::new(RE_YEAR_MONTH)?;
    let re_year_month_day = Regex::new(RE_YEAR_MONTH_DAY)?;

    let naive_str;

    if re_only_year.is_match(s) {
        naive_str = format!("{}-01-01 00:00:00", s);
    } else if re_year_month.is_match(s) {
        let captures = re_year_month.captures(s).unwrap();

        let year = captures.name("year").unwrap().as_str();
        let month = captures.name("month").unwrap().as_str();

        naive_str = format!("{}-{}-01 00:00:00", year, month);
    } else if re_year_month_day.is_match(s) {
        let captures = re_year_month_day.captures(s).unwrap();

        let year = captures.name("year").unwrap().as_str();
        let month = captures.name("month").unwrap().as_str();
        let day = captures.name("day").unwrap().as_str();

        naive_str = format!("{}-{}-{} 00:00:00", year, month, day);
    } else {
        return Err(anyhow!("Invalid str to convert to system time '{}'", s));
    }

    let naive_dt = NaiveDateTime::parse_from_str(&naive_str, "%Y-%m-%d %H:%M:%S")?;
    let local_dt = match Local.from_local_datetime(&naive_dt) {
        LocalResult::Single(dt) => dt,
        _ => {
            // never reached
            return Err(anyhow!("Failed to local datetime"));
        }
    };

    Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(local_dt.timestamp() as u64))
}

trait ToSystemTime {
    fn to_system_time(&self) -> SystemTime;
}

impl ToSystemTime for i64 {
    fn to_system_time(&self) -> SystemTime {
        let duration_since_epoch = Duration::from_secs(*self as u64);
        UNIX_EPOCH + duration_since_epoch
    }
}

const RE_ONLY_YEAR: &str = "^[0-9]{4}$";
const RE_YEAR_MONTH: &str = r"(?P<year>[0-9]{4})-(?P<month>[0-9]{2})$";
const RE_YEAR_MONTH_DAY: &str = r"(?P<year>[0-9]{4})-(?P<month>[0-9]{2})-(?P<day>[0-9]{2})$";

async fn to_be_imported_after(out_dir: &Path) -> Result<Option<SystemTime>> {
    // find first-level: e.g., 2023
    let first_depth_dir = get_last_modified_dir(out_dir, Some(RE_ONLY_YEAR)).await?;
    if let Some(first_depth_dir) = first_depth_dir {
        // find second-level: e.g., 2023-02-16
        return if let Some(second_depth_dir) =
            get_last_modified_dir(&first_depth_dir, Some(RE_YEAR_MONTH_DAY)).await?
        {
            let t = system_time_from_str(second_depth_dir.file_name().unwrap().to_str().unwrap())?;
            Ok(Some(t))
        } else {
            // get first day of given year
            let first_day_of_year =
                system_time_from_str(first_depth_dir.file_name().unwrap().to_str().unwrap())?;
            Ok(Some(first_day_of_year))
        };
    }

    Ok(None)
}

async fn get_last_modified_dir(dir: &Path, re_pattern: Option<&str>) -> Result<Option<PathBuf>> {
    let mut last_modified: Option<PathBuf> = None;

    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            if let Some(pattern) = re_pattern {
                if let Some(filename) = entry.file_name().to_str() {
                    let re = Regex::new(pattern)?;
                    if !re.is_match(filename) {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            if let Some(ref prev_entry) = last_modified {
                let prev_modified_time = prev_entry.metadata()?.modified()?;
                let modified_time = entry.metadata().await?.modified()?;
                if modified_time > prev_modified_time {
                    last_modified = Some(entry.path());
                }
            } else {
                last_modified = Some(entry.path());
            }
        }
    }

    Ok(last_modified)
}

fn import_entries(dir: &Path) -> Vec<DirEntry> {
    walk_and_filter_only_supported_images(dir)
}

const MAX_DEPTH: usize = 10;

fn walk_and_filter_only_supported_images(dir: &Path) -> Vec<DirEntry> {
    let mut entries = Vec::new();

    for entry in WalkDir::new(dir)
        .max_depth(MAX_DEPTH)
        .into_iter()
        .filter_map(|entry| {
            let entry = match entry {
                Ok(v) => v,
                Err(_) => return None,
            };

            let path = entry.path();
            if !path.is_file() {
                return None;
            }

            if let Some(filename) = path.file_stem() {
                match filename.to_str() {
                    Some(s) if String::from(s).starts_with(".") => return None, // filter if file is hidden
                    _ => (),
                }
            } else {
                return None;
            }

            if let Some(ext) = path.extension()?.to_str() {
                return match ext.to_lowercase().as_str() {
                    "jpeg" | "jpg" | "heic" | "heif" => Some(entry),
                    _ => None,
                };
            }

            None
        })
    {
        entries.push(entry);
    }

    entries
}

fn oldest_and_most_recent_taken_at(entries: &Vec<Inspection>) -> Result<(SystemTime, SystemTime)> {
    let created_at_list = entries
        .iter()
        .map(|entry| entry.taken_at.timestamp())
        .collect::<Vec<i64>>();

    let oldest = created_at_list.iter().min();
    let most_recent = created_at_list.iter().max();

    if let (Some(oldest), Some(most_recent)) = (oldest, most_recent) {
        Ok((oldest.to_system_time(), most_recent.to_system_time()))
    } else {
        Err(anyhow!("Failed to find oldest and most recent file"))
    }
}

trait FormattedLocalTime {
    fn to_string(&self) -> String;
}

impl FormattedLocalTime for SystemTime {
    fn to_string(&self) -> String {
        let dt: DateTime<Local> = DateTime::from(*self);
        dt.format("%Y:%m:%d %H:%M:%S%:::z").to_string()
    }
}
