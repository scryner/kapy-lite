use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Result, anyhow};
use chrono::{DateTime, FixedOffset, Local};
use kapy_exif::{CopyWithRawExif, ExtractRawExif, exif::Metadata, heic, jpeg};
use tokio::io::AsyncWrite;

use crate::gps::GpsSearch;

use super::{
    ImageFormat,
    inspect::{Inspection, determine_format},
};

pub enum CopyResult {
    Copied,
    CopiedWithAddingGpsInfo,
    Skipped,
}

pub async fn copy_with_inspection(
    in_file: impl AsRef<Path>,
    out_dir: impl AsRef<Path>,
    inspection: &Inspection,
    gpx: Arc<dyn GpsSearch>,
    dry_run: bool,
) -> Result<CopyResult> {
    // check arguments
    if !in_file.as_ref().is_file() {
        return Err(anyhow!(
            "Input path '{}' is not file",
            in_file.as_ref().to_str().unwrap()
        ));
    }

    if !out_dir.as_ref().is_dir() {
        return Err(anyhow!(
            "Output path '{}' is not directory",
            out_dir.as_ref().to_str().unwrap()
        ));
    }

    // retrieve gps data if needed
    let gps_info = if !inspection.gps_recorded {
        // try to match gps
        let taken_at = inspection.taken_at.to_fixed_offset();

        if let Some(waypoint) = gpx.search(&taken_at) {
            Some(GpsInfo {
                lat: waypoint.point().y(),
                lon: waypoint.point().x(),
                alt: waypoint.elevation.unwrap_or(0.0),
            })
        } else {
            None
        }
    } else {
        None
    };

    let out_file = match out_file(in_file.as_ref(), out_dir.as_ref())? {
        Some(out_file) => out_file,
        None => return Ok(CopyResult::Skipped),
    };

    if let Some(gps_info) = gps_info {
        if !dry_run {
            copy_with_gps_info(in_file.as_ref(), out_file.as_ref(), &gps_info).await?;
        }
        Ok(CopyResult::CopiedWithAddingGpsInfo)
    } else {
        // just copy
        if !dry_run {
            tokio::fs::copy(in_file.as_ref(), &out_file).await?;
        }
        Ok(CopyResult::Copied)
    }
}

trait ToFixedOffset {
    fn to_fixed_offset(&self) -> DateTime<FixedOffset>;
}

impl ToFixedOffset for DateTime<Local> {
    fn to_fixed_offset(&self) -> DateTime<FixedOffset> {
        self.with_timezone(self.offset())
    }
}

struct GpsInfo {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
}

fn out_file(in_file: &Path, out_dir: &Path) -> Result<Option<PathBuf>> {
    let filename = match in_file.file_stem() {
        Some(stem) => stem.to_str().unwrap(), // never failed
        None => {
            // never reached
            return Err(anyhow!("Failed to find stem of file"));
        }
    };

    let ext = match in_file.extension() {
        Some(ext) => ext.to_str().unwrap(), // never failed
        None => {
            // never reached
            return Err(anyhow!("Failed to find extension of file"));
        }
    };

    let dest_filename = format!("{}.{}", filename, ext);
    let out_file = out_dir.to_path_buf().join(&dest_filename);

    if out_file.exists() {
        // check file size
        let in_file_len = in_file.metadata()?.len();
        let out_file_len = out_file.metadata()?.len();

        // just return if it is same file
        if in_file_len == out_file_len {
            return Ok(None);
        }

        let new_out_file = {
            const MAX_RETRIES: u32 = 10;
            let mut new_out_file = None;

            for i in 1..MAX_RETRIES {
                let new_filename = format!("{}-{:02}.{}", filename, i, ext);
                let new_path = out_dir.to_path_buf().join(&new_filename);

                if !new_path.exists() {
                    new_out_file = Some(new_path);
                    break;
                }
            }

            if let Some(new_out_file) = new_out_file {
                new_out_file
            } else {
                return Err(anyhow!("Reached max retries to find new out file"));
            }
        };

        Ok(Some(new_out_file))
    } else {
        Ok(Some(out_file))
    }
}

async fn copy_with_gps_info(in_file: &Path, out_file: &Path, gps_info: &GpsInfo) -> Result<()> {
    let format = determine_format(in_file)?;

    // open file as writer
    let w = tokio::fs::File::create(out_file).await?;

    // open image
    match format {
        ImageFormat::JPEG => {
            let image = jpeg(in_file).await?;
            copy_with_gps_info_internal(image, gps_info, w).await
        }
        ImageFormat::HEIC => {
            let image = heic(in_file).await?;
            copy_with_gps_info_internal(image, gps_info, w).await
        }
    }
}

async fn copy_with_gps_info_internal(
    image: impl ExtractRawExif + CopyWithRawExif,
    gps_info: &GpsInfo,
    w: impl AsyncWrite + Send + Sync + Unpin,
) -> Result<()> {
    // extract exif
    let metadata_blob = image
        .extract()
        .await?
        .ok_or(anyhow!("No EXIF data found"))?;

    // make exif from blob
    let mut metadata = Metadata::new_from_exif_blob(&metadata_blob)?;

    // update gps info
    metadata.update_gps_info(gps_info.lat, gps_info.lon, gps_info.alt)?;

    // make exif blob from metadata
    let new_metadata_blob = metadata.dump()?;

    // copy with new exif blob
    image.copy_with_raw_exif(&new_metadata_blob, w).await
}
