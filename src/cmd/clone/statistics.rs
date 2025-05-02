use std::ops::Add;

use anyhow::Error;
use console::style;

use crate::image::inspect::Inspection;

pub struct CopyStatistics {
    pub skipped: usize,
    pub copied: usize,
    pub copied_with_adding_gps_info: usize,
    pub error: usize,
}

impl CopyStatistics {
    pub fn new() -> Self {
        Self {
            skipped: 0,
            copied: 0,
            copied_with_adding_gps_info: 0,
            error: 0,
        }
    }

    fn total(&self) -> usize {
        self.skipped + self.copied + self.copied_with_adding_gps_info + self.error
    }

    /*
    123 total images (120 succeed / 3 failed)
    ---
      0 skipped
     10 copied
    100 copied with adding gps info
      1 errors
    ---
    Errors:
    - some_image.heic: Failed to read metadata
    */
    pub fn print_with_error(&self, errors: &Vec<(&Inspection, Error)>) {
        let error_len = errors.len();
        let width = max_width(vec![self.total(), error_len]);
        print!("{:>5} total images", style(self.total()).cyan().bold());
        println!(
            " ({} succeed / {} failed)",
            style(self.total() - error_len).green(),
            if error_len > 0 {
                style(error_len).red()
            } else {
                style(error_len).dim()
            }
        );

        println!("{}", style("---").dim());
        println!("{:>width$} skipped", self.skipped);
        println!("{:>width$} copied", self.copied);
        println!(
            "{:>width$} copied with adding gps info",
            self.copied_with_adding_gps_info,
        );
        println!("{:>width$} errors", self.error);

        // print errors
        if errors.len() > 0 {
            println!("{}", style("---").dim());
            println!("Errors:");
            for (inspection, e) in errors.iter() {
                println!(
                    "{} {}: {}",
                    style("-").red(),
                    style(inspection.path.to_str().unwrap()).red().bold(),
                    e
                );
            }
        }
    }
}

impl Add for CopyStatistics {
    type Output = CopyStatistics;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            skipped: self.skipped + rhs.skipped,
            copied: self.copied + rhs.copied,
            copied_with_adding_gps_info: self.copied_with_adding_gps_info
                + rhs.copied_with_adding_gps_info,
            error: self.error + rhs.error,
        }
    }
}

fn max_width(nums: Vec<usize>) -> usize {
    let widths: Vec<usize> = nums
        .iter()
        .map(|n| (*n as f64).log10().floor() as usize + 1)
        .collect();

    *widths.iter().max().unwrap()
}
