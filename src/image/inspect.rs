use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use kapy_exif::{ExtractRawExif, exif::Metadata, heic, jpeg};

use super::ImageFormat;

const META_DATETIME: &str = "Exif.Image.DateTime";
const META_GPS_LAT: &str = "Exif.GPSInfo.GPSLatitude";
const META_GPS_LON: &str = "Exif.GPSInfo.GPSLongitude";

#[derive(Debug)]
pub struct Inspection {
    pub path: PathBuf,
    #[allow(unused)]
    pub format: ImageFormat,
    pub gps_recorded: bool,
    pub taken_at: DateTime<Local>,
}

pub async fn inspect_image_from_path(path: impl AsRef<Path>) -> Result<Inspection> {
    // determine format
    let format = determine_format(path.as_ref())?;

    // extract exif blob
    let metadata_blob = {
        match format {
            ImageFormat::JPEG => jpeg(path.as_ref()).await?.extract().await?,
            ImageFormat::HEIC => heic(path.as_ref()).await?.extract().await?,
        }
    };

    let metadata_blob = metadata_blob.ok_or(anyhow!("No EXIF data found"))?;

    // make exif from blob
    let metadata = Metadata::new_from_exif_blob(&metadata_blob)?;

    // get 'taken at'
    let taken_at = {
        match metadata.get_tag(META_DATETIME) {
            Some(dt) if dt.len() > 0 => {
                let naive_date = NaiveDateTime::parse_from_str(&dt, "%Y:%m:%d %H:%M:%S")?;
                Local.from_local_datetime(&naive_date).unwrap() // never failed
            }
            _ => {
                let created_at = path.as_ref().metadata()?.created()?;
                DateTime::from(created_at)
            }
        }
    };

    // get gps recorded
    let gps_recorded = {
        let lat_recorded = metadata.get_tag(META_GPS_LAT).is_some();
        let lon_recorded = metadata.get_tag(META_GPS_LON).is_some();
        lat_recorded && lon_recorded
    };

    Ok(Inspection {
        path: path.as_ref().to_path_buf(),
        format,
        gps_recorded,
        taken_at,
    })
}

pub fn determine_format(path: &Path) -> Result<ImageFormat> {
    let format = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| anyhow!("Invalid file extension"))?;

    match format.to_lowercase().as_str() {
        "jpg" | "jpeg" => Ok(ImageFormat::JPEG),
        "heic" | "heif" => Ok(ImageFormat::HEIC),
        _ => Err(anyhow!("Unsupported image format '{}'", format)),
    }
}

#[cfg(test)]
mod tests {
    use crate::image::inspect::inspect_image_from_path;

    const SAMPLES: &[&str] = &[
        "sample/sample_by_pentax-k1.jpg",
        "sample/sample_by_hasselblad-x2d.heic",
        "sample/sample_by_iphone15-pro-max.heic",
    ];

    #[tokio::test]
    async fn inspect_images() {
        for sample in SAMPLES {
            let inspection = inspect_image_from_path(sample)
                .await
                .expect(format!("Failed to inspect image '{}'", sample).as_str());

            println!("{:?}", inspection);
        }
    }
}
