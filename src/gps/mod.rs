use crate::drive::GoogleDrive;
use anyhow::Result;
use bytes::Bytes;
use chrono::{DateTime, FixedOffset, Utc};
use gpx::{Gpx, Waypoint};
use std::collections::BTreeMap;
use std::io::BufReader;
use std::time::{Duration, SystemTime};

pub trait GpsSearch {
    fn search(&self, t: &DateTime<FixedOffset>) -> Option<Waypoint>;
}

pub struct NoopGpsSearch;

impl GpsSearch for NoopGpsSearch {
    fn search(&self, _t: &DateTime<FixedOffset>) -> Option<Waypoint> {
        None
    }
}

trait UnixTime {
    fn unix_at(&self) -> Option<i64>;
}

impl UnixTime for Waypoint {
    fn unix_at(&self) -> Option<i64> {
        match self.time {
            Some(t) => {
                let dt = DateTime::parse_from_rfc3339(&t.format().unwrap()).unwrap();
                Some(dt.timestamp())
            }
            None => None,
        }
    }
}

trait TimestampKey {
    fn make_key(&self, t: i64) -> i64;
    fn make_prev_key(&self, t: i64) -> i64;
    fn make_next_key(&self, t: i64) -> i64;
}

impl TimestampKey for Duration {
    fn make_key(&self, t: i64) -> i64 {
        t - (t % self.as_secs() as i64)
    }

    fn make_prev_key(&self, t: i64) -> i64 {
        let curr = self.make_key(t);
        curr - self.as_secs() as i64
    }

    fn make_next_key(&self, t: i64) -> i64 {
        let curr = self.make_key(t);
        curr + self.as_secs() as i64
    }
}

trait SearchWaypoint {
    fn closest(&self, t: i64) -> Option<Waypoint>;
}

impl SearchWaypoint for Vec<Waypoint> {
    fn closest(&self, t: i64) -> Option<Waypoint> {
        // find nearest waypoint
        match self.into_iter().min_by(|a, b| {
            let t_a = a.unix_at().unwrap(); // never failed, we already examined it
            let t_b = b.unix_at().unwrap();

            let diff_a = (t_a - t).abs();
            let diff_b = (t_b - t).abs();

            diff_a.cmp(&diff_b)
        }) {
            Some(found) => Some(found.clone()),
            None => None,
        }
    }
}

trait Pour<T> {
    fn pour_into(&mut self, data: T) -> Result<i32>;
}

#[derive(Debug)]
pub struct GpxStorage {
    cache: BTreeMap<i64, Vec<Waypoint>>,
    match_within: Duration,
}

impl GpxStorage {
    pub fn new(match_within: Duration) -> Self {
        Self {
            cache: BTreeMap::new(),
            match_within,
        }
    }

    pub async fn from_google_drive<F>(
        drive: &GoogleDrive,
        start: SystemTime,
        end: SystemTime,
        max_gpx_files: usize,
        match_within: Duration,
        mut when_update: F,
    ) -> Result<Self>
    where
        F: FnMut(String),
    {
        // make new storage
        let mut storage = GpxStorage::new(match_within);

        // make query to find gpx files on google drive
        let start: DateTime<Utc> = DateTime::from(start);
        let end: DateTime<Utc> = DateTime::from(end);

        let start = start.format("%Y-%m-%dT%H:%M:%S");
        let end = end.format("%Y-%m-%dT%H:%M:%S");

        let q = format!(
            "modifiedTime >= '{}' and createdTime <= '{}' and mimeType='application/gpx+xml'",
            start, end
        );

        // query to google drive
        let list = drive.list(&q, max_gpx_files, None).await?;

        for gpx in list.files.iter() {
            when_update(gpx.name.clone());

            // download content
            let blob = drive.download_blob(&gpx.id).await?;
            storage.pour_into(blob)?;
        }

        Ok(storage)
    }
}

impl GpsSearch for GpxStorage {
    fn search(&self, t: &DateTime<FixedOffset>) -> Option<Waypoint> {
        let t = t.timestamp();

        let keys = vec![
            self.match_within.make_prev_key(t),
            self.match_within.make_key(t),
            self.match_within.make_next_key(t),
        ];

        let mut target = Vec::new();

        for key in keys {
            match self.cache.get(&key) {
                Some(l) => {
                    target.extend(l.to_vec());
                }
                None => (),
            }
        }

        target.closest(t)
    }
}

impl Pour<Gpx> for GpxStorage {
    fn pour_into(&mut self, data: Gpx) -> Result<i32> {
        let mut counts = 0;

        for track in data.tracks.iter() {
            for segment in track.segments.iter() {
                for waypoint in segment.points.iter() {
                    match waypoint.unix_at() {
                        Some(t) => {
                            let key = self.match_within.make_key(t);
                            match self.cache.get_mut(&key) {
                                Some(l) => {
                                    // insert into existed list
                                    l.push(waypoint.clone());
                                    counts += 1;
                                }
                                None => {
                                    // make new list
                                    let l = vec![waypoint.clone()];
                                    self.cache.insert(key, l);
                                    counts += 1;
                                }
                            }
                        }
                        None => continue,
                    }
                }
            }
        }

        Ok(counts)
    }
}

impl Pour<Bytes> for GpxStorage {
    fn pour_into(&mut self, data: Bytes) -> Result<i32> {
        // parse gpx from bytes
        let reader = BufReader::new(data.as_ref());
        let g = gpx::read(reader)?;

        self.pour_into(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn parse_gpx() {
        let gpx_content = String::from(TEST_GPX_CONTENT);
        let reader = BufReader::new(gpx_content.as_bytes());

        let g = gpx::read(reader).unwrap();

        // pouring
        let mut storage = GpxStorage::new(Duration::from_secs(300));
        let counts = storage.pour_into(g).unwrap();
        println!("{} waypoints were poured", counts);

        // search
        let qs = "2023-02-03T05:29:36Z";
        let qdt = DateTime::parse_from_rfc3339(qs).unwrap();
        let qts = qdt.timestamp();

        let waypoint = storage.search(&qdt).unwrap();

        assert_eq!(qts, waypoint.unix_at().unwrap());

        let qdt = qdt + chrono::Duration::seconds(1);
        let waypoint = storage.search(&qdt).unwrap();
        assert_eq!(qts, waypoint.unix_at().unwrap());
    }

    const TEST_GPX_CONTENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx creator="Geotag Photos http://www.geotagphotos.net/" version="1.0" xmlns="http://www.topografix.com/GPX/1/0" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.topografix.com/GPX/1/0 http://www.topografix.com/GPX/1/0/gpx.xsd">
<trk>
  <name><![CDATA[2023/02/03]]></name>
  <trkseg><!-- TZ: 32400 -->

      <trkpt lat="37.311695" lon="126.606705"><ele>9.332291</ele><time>2023-02-03T04:54:49Z</time></trkpt>
      <trkpt lat="37.312168" lon="126.606476"><ele>16.431431</ele><time>2023-02-03T04:56:54Z</time></trkpt>
      <trkpt lat="37.312778" lon="126.606773"><ele>-30.164284</ele><time>2023-02-03T04:58:56Z</time></trkpt>
      <trkpt lat="37.312778" lon="126.606773"><ele>-30.164284</ele><time>2023-02-03T05:01:00Z</time></trkpt>
      <trkpt lat="37.313000" lon="126.608322"><ele>42.361786</ele><time>2023-02-03T05:03:03Z</time></trkpt>
      <trkpt lat="37.313000" lon="126.608322"><ele>42.361786</ele><time>2023-02-03T05:07:13Z</time></trkpt>
      <trkpt lat="37.312695" lon="126.609932"><ele>88.593208</ele><time>2023-02-03T05:09:14Z</time></trkpt>
      <trkpt lat="37.312695" lon="126.609932"><ele>88.593208</ele><time>2023-02-03T05:13:24Z</time></trkpt>
      <trkpt lat="37.312370" lon="126.608231"><ele>17.515579</ele><time>2023-02-03T05:15:27Z</time></trkpt>
      <trkpt lat="37.311485" lon="126.607597"><ele>17.735046</ele><time>2023-02-03T05:17:28Z</time></trkpt>
      <trkpt lat="37.309200" lon="126.605545"><ele>-8.730284</ele><time>2023-02-03T05:19:34Z</time></trkpt>
      <trkpt lat="37.297585" lon="126.585625"><ele>11.262866</ele><time>2023-02-03T05:21:34Z</time></trkpt>
      <trkpt lat="37.288368" lon="126.576439"><ele>13.387933</ele><time>2023-02-03T05:23:34Z</time></trkpt>
      <trkpt lat="37.286354" lon="126.575218"><ele>4.405506</ele><time>2023-02-03T05:25:34Z</time></trkpt>
      <trkpt lat="37.286354" lon="126.575218"><ele>4.405506</ele><time>2023-02-03T05:27:34Z</time></trkpt>
      <trkpt lat="37.287075" lon="126.574463"><ele>7.853204</ele><time>2023-02-03T05:29:36Z</time></trkpt>
      <trkpt lat="37.287075" lon="126.574463"><ele>7.853204</ele><time>2023-02-03T05:33:37Z</time></trkpt>
      <trkpt lat="37.286427" lon="126.573517"><ele>8.081021</ele><time>2023-02-03T05:35:37Z</time></trkpt>
      <trkpt lat="37.285812" lon="126.572586"><ele>13.726681</ele><time>2023-02-03T05:37:39Z</time></trkpt>
      <trkpt lat="37.284847" lon="126.571144"><ele>13.745453</ele><time>2023-02-03T05:39:40Z</time></trkpt>
      <trkpt lat="37.284847" lon="126.571144"><ele>13.745453</ele><time>2023-02-03T05:41:40Z</time></trkpt>
      <trkpt lat="37.283970" lon="126.569855"><ele>13.212789</ele><time>2023-02-03T05:43:42Z</time></trkpt>
      <trkpt lat="37.283970" lon="126.569855"><ele>13.212789</ele><time>2023-02-03T05:45:42Z</time></trkpt>
      <trkpt lat="37.282070" lon="126.569252"><ele>8.852501</ele><time>2023-02-03T05:47:42Z</time></trkpt>
      <trkpt lat="37.282166" lon="126.568489"><ele>3.532457</ele><time>2023-02-03T05:49:42Z</time></trkpt>
      <trkpt lat="37.281124" lon="126.569641"><ele>40.537270</ele><time>2023-02-03T05:51:46Z</time></trkpt>
      <trkpt lat="37.280655" lon="126.569992"><ele>-10.731391</ele><time>2023-02-03T05:53:46Z</time></trkpt>
      <trkpt lat="37.280125" lon="126.569801"><ele>13.398923</ele><time>2023-02-03T05:55:47Z</time></trkpt>
      <trkpt lat="37.280125" lon="126.569801"><ele>13.398923</ele><time>2023-02-03T05:57:49Z</time></trkpt>
      <trkpt lat="37.279167" lon="126.569756"><ele>28.237352</ele><time>2023-02-03T05:59:54Z</time></trkpt>
      <trkpt lat="37.278355" lon="126.569382"><ele>93.430725</ele><time>2023-02-03T06:01:57Z</time></trkpt>
      <trkpt lat="37.276512" lon="126.569267"><ele>13.396681</ele><time>2023-02-03T06:03:58Z</time></trkpt>
      <trkpt lat="37.275486" lon="126.569328"><ele>6.852501</ele><time>2023-02-03T06:05:58Z</time></trkpt>
      <trkpt lat="37.275486" lon="126.569328"><ele>6.852501</ele><time>2023-02-03T06:08:51Z</time></trkpt>  </trkseg>
  <trkseg><!-- TZ: 32400 -->

      <trkpt lat="37.277668" lon="126.566864"><ele>12.139791</ele><time>2023-02-03T06:41:30Z</time></trkpt>
      <trkpt lat="37.277561" lon="126.565765"><ele>28.865511</ele><time>2023-02-03T06:43:35Z</time></trkpt>
      <trkpt lat="37.277561" lon="126.565765"><ele>28.865511</ele><time>2023-02-03T06:45:41Z</time></trkpt>
      <trkpt lat="37.277142" lon="126.568710"><ele>29.056765</ele><time>2023-02-03T06:47:41Z</time></trkpt>
      <trkpt lat="37.275784" lon="126.567070"><ele>22.426682</ele><time>2023-02-03T06:49:44Z</time></trkpt>
      <trkpt lat="37.275223" lon="126.567032"><ele>9.382112</ele><time>2023-02-03T06:51:44Z</time></trkpt>
      <trkpt lat="37.276474" lon="126.566795"><ele>23.298229</ele><time>2023-02-03T06:53:44Z</time></trkpt>
      <trkpt lat="37.276474" lon="126.566795"><ele>23.298229</ele><time>2023-02-03T06:55:44Z</time></trkpt>
      <trkpt lat="37.276093" lon="126.566307"><ele>43.871181</ele><time>2023-02-03T06:57:44Z</time></trkpt>
      <trkpt lat="37.275848" lon="126.564995"><ele>65.424187</ele><time>2023-02-03T06:59:44Z</time></trkpt>
      <trkpt lat="37.275852" lon="126.564056"><ele>83.609528</ele><time>2023-02-03T07:01:44Z</time></trkpt>
      <trkpt lat="37.275852" lon="126.564056"><ele>83.609528</ele><time>2023-02-03T07:03:44Z</time></trkpt>
      <trkpt lat="37.275845" lon="126.563400"><ele>98.494881</ele><time>2023-02-03T07:05:44Z</time></trkpt>
      <trkpt lat="37.275845" lon="126.563400"><ele>98.494881</ele><time>2023-02-03T07:15:56Z</time></trkpt>
      <trkpt lat="37.276539" lon="126.562027"><ele>84.804840</ele><time>2023-02-03T07:18:01Z</time></trkpt>
      <trkpt lat="37.276539" lon="126.562027"><ele>84.804840</ele><time>2023-02-03T07:20:05Z</time></trkpt>
      <trkpt lat="37.276901" lon="126.560997"><ele>26.889185</ele><time>2023-02-03T07:22:09Z</time></trkpt>
      <trkpt lat="37.276730" lon="126.559967"><ele>9.046349</ele><time>2023-02-03T07:24:09Z</time></trkpt>
      <trkpt lat="37.276711" lon="126.558968"><ele>45.285339</ele><time>2023-02-03T07:26:13Z</time></trkpt>
      <trkpt lat="37.276711" lon="126.558968"><ele>45.285339</ele><time>2023-02-03T07:36:37Z</time></trkpt>
      <trkpt lat="37.276886" lon="126.558029"><ele>-6.747444</ele><time>2023-02-03T07:38:37Z</time></trkpt>
      <trkpt lat="37.276886" lon="126.558029"><ele>-6.747444</ele><time>2023-02-03T07:40:42Z</time></trkpt>
      <trkpt lat="37.276424" lon="126.556480"><ele>-1.034570</ele><time>2023-02-03T07:42:47Z</time></trkpt>
      <trkpt lat="37.276424" lon="126.556480"><ele>-1.034570</ele><time>2023-02-03T07:48:54Z</time></trkpt>
      <trkpt lat="37.276093" lon="126.555458"><ele>14.758107</ele><time>2023-02-03T07:50:54Z</time></trkpt>
      <trkpt lat="37.276299" lon="126.554825"><ele>15.179617</ele><time>2023-02-03T07:52:54Z</time></trkpt>
      <trkpt lat="37.276299" lon="126.554825"><ele>15.179617</ele><time>2023-02-03T07:54:54Z</time></trkpt>
      <trkpt lat="37.276367" lon="126.552574"><ele>15.296681</ele><time>2023-02-03T07:56:55Z</time></trkpt>
      <trkpt lat="37.276367" lon="126.552574"><ele>15.296681</ele><time>2023-02-03T07:58:55Z</time></trkpt>
      <trkpt lat="37.277004" lon="126.551727"><ele>16.457047</ele><time>2023-02-03T08:00:58Z</time></trkpt>
      <trkpt lat="37.277954" lon="126.551338"><ele>36.106075</ele><time>2023-02-03T08:02:58Z</time></trkpt>
      <trkpt lat="37.279346" lon="126.551231"><ele>-6.002488</ele><time>2023-02-03T08:05:04Z</time></trkpt>
      <trkpt lat="37.280251" lon="126.551544"><ele>-0.810675</ele><time>2023-02-03T08:07:06Z</time></trkpt>
      <trkpt lat="37.280262" lon="126.550797"><ele>-40.909760</ele><time>2023-02-03T08:09:11Z</time></trkpt>
      <trkpt lat="37.281311" lon="126.550407"><ele>17.696043</ele><time>2023-02-03T08:11:14Z</time></trkpt>
      <trkpt lat="37.282177" lon="126.549820"><ele>28.539169</ele><time>2023-02-03T08:13:14Z</time></trkpt>
      <trkpt lat="37.282619" lon="126.548882"><ele>58.759583</ele><time>2023-02-03T08:15:14Z</time></trkpt>
      <trkpt lat="37.283176" lon="126.548317"><ele>43.822224</ele><time>2023-02-03T08:17:14Z</time></trkpt>
      <trkpt lat="37.283546" lon="126.546860"><ele>23.355228</ele><time>2023-02-03T08:19:14Z</time></trkpt>
      <trkpt lat="37.283627" lon="126.544907"><ele>6.708738</ele><time>2023-02-03T08:21:18Z</time></trkpt>
      <trkpt lat="37.283627" lon="126.544907"><ele>6.708738</ele><time>2023-02-03T08:23:18Z</time></trkpt>
      <trkpt lat="37.283043" lon="126.543121"><ele>33.271416</ele><time>2023-02-03T08:25:24Z</time></trkpt>
      <trkpt lat="37.283646" lon="126.541405"><ele>4.883728</ele><time>2023-02-03T08:27:27Z</time></trkpt>
      <trkpt lat="37.284714" lon="126.540802"><ele>-0.605018</ele><time>2023-02-03T08:29:31Z</time></trkpt>
      <trkpt lat="37.285706" lon="126.540352"><ele>21.218918</ele><time>2023-02-03T08:31:31Z</time></trkpt>
      <trkpt lat="37.286144" lon="126.539436"><ele>6.216610</ele><time>2023-02-03T08:33:31Z</time></trkpt>
      <trkpt lat="37.286526" lon="126.538795"><ele>-0.387028</ele><time>2023-02-03T08:35:32Z</time></trkpt>
      <trkpt lat="37.286526" lon="126.538795"><ele>-0.387028</ele><time>2023-02-03T08:41:44Z</time></trkpt>
      <trkpt lat="37.286724" lon="126.537918"><ele>9.427399</ele><time>2023-02-03T08:43:48Z</time></trkpt>
      <trkpt lat="37.286938" lon="126.537331"><ele>8.909561</ele><time>2023-02-03T08:45:52Z</time></trkpt>
      <trkpt lat="37.287643" lon="126.535866"><ele>22.734629</ele><time>2023-02-03T08:47:56Z</time></trkpt>
      <trkpt lat="37.288521" lon="126.534546"><ele>34.329601</ele><time>2023-02-03T08:50:01Z</time></trkpt>
      <trkpt lat="37.288521" lon="126.534546"><ele>34.329601</ele><time>2023-02-03T08:56:09Z</time></trkpt>
      <trkpt lat="37.288418" lon="126.533020"><ele>-23.381374</ele><time>2023-02-03T08:58:13Z</time></trkpt>
      <trkpt lat="37.288418" lon="126.533020"><ele>-23.381374</ele><time>2023-02-03T09:02:13Z</time></trkpt>
      <trkpt lat="37.287697" lon="126.534218"><ele>15.002955</ele><time>2023-02-03T09:04:18Z</time></trkpt>
      <trkpt lat="37.287697" lon="126.534218"><ele>15.002955</ele><time>2023-02-03T09:08:23Z</time></trkpt>
      <trkpt lat="37.287971" lon="126.535622"><ele>1.074199</ele><time>2023-02-03T09:10:23Z</time></trkpt>
      <trkpt lat="37.287052" lon="126.536758"><ele>16.292402</ele><time>2023-02-03T09:12:27Z</time></trkpt>
      <trkpt lat="37.287052" lon="126.536758"><ele>16.292402</ele><time>2023-02-03T09:14:30Z</time></trkpt>
      <trkpt lat="37.287033" lon="126.538353"><ele>-93.274529</ele><time>2023-02-03T09:16:34Z</time></trkpt>
      <trkpt lat="37.287033" lon="126.538353"><ele>-93.274529</ele><time>2023-02-03T09:18:42Z</time></trkpt>
      <trkpt lat="37.286125" lon="126.538933"><ele>-2.402106</ele><time>2023-02-03T09:20:42Z</time></trkpt>
      <trkpt lat="37.285492" lon="126.539413"><ele>-14.383680</ele><time>2023-02-03T09:22:42Z</time></trkpt>
      <trkpt lat="37.285030" lon="126.539688"><ele>13.220819</ele><time>2023-02-03T09:24:42Z</time></trkpt>
      <trkpt lat="37.285030" lon="126.539688"><ele>13.220819</ele><time>2023-02-03T09:26:46Z</time></trkpt>
      <trkpt lat="37.284069" lon="126.539429"><ele>7.137990</ele><time>2023-02-03T09:28:46Z</time></trkpt>
      <trkpt lat="37.282623" lon="126.539551"><ele>5.758447</ele><time>2023-02-03T09:30:46Z</time></trkpt>
      <trkpt lat="37.282314" lon="126.540535"><ele>13.974158</ele><time>2023-02-03T09:32:49Z</time></trkpt>
      <trkpt lat="37.282314" lon="126.540535"><ele>13.974158</ele><time>2023-02-03T09:37:01Z</time></trkpt>
      <trkpt lat="37.281570" lon="126.542160"><ele>15.767071</ele><time>2023-02-03T09:39:01Z</time></trkpt>
      <trkpt lat="37.281197" lon="126.543167"><ele>-17.234629</ele><time>2023-02-03T09:41:04Z</time></trkpt>
      <trkpt lat="37.281071" lon="126.544098"><ele>-0.259251</ele><time>2023-02-03T09:43:09Z</time></trkpt>
      <trkpt lat="37.280800" lon="126.545677"><ele>8.436171</ele><time>2023-02-03T09:45:09Z</time></trkpt>
      <trkpt lat="37.280552" lon="126.546631"><ele>16.896444</ele><time>2023-02-03T09:47:13Z</time></trkpt>
      <trkpt lat="37.280552" lon="126.546631"><ele>16.896444</ele><time>2023-02-03T09:49:22Z</time></trkpt>
      <trkpt lat="37.279892" lon="126.548447"><ele>7.706508</ele><time>2023-02-03T09:51:25Z</time></trkpt>
      <trkpt lat="37.279892" lon="126.548447"><ele>7.706508</ele><time>2023-02-03T09:55:29Z</time></trkpt>
      <trkpt lat="37.279259" lon="126.549828"><ele>7.978574</ele><time>2023-02-03T09:57:30Z</time></trkpt>
      <trkpt lat="37.279259" lon="126.549828"><ele>7.978574</ele><time>2023-02-03T09:58:58Z</time></trkpt>  </trkseg>
</trk>
</gpx>
"#;
}

/*
$ exiv2 -pa IMAGE.JPG | grep -i gps
Exif.Image.GPSTag                            Long        1  6406
Exif.GPSInfo.GPSVersionID                    Byte        4  2.3.0.0
Exif.GPSInfo.GPSLatitudeRef                  Ascii       2  North
Exif.GPSInfo.GPSLatitude                     Rational    3  37deg 17' 13"
Exif.GPSInfo.GPSLongitudeRef                 Ascii       2  East
Exif.GPSInfo.GPSLongitude                    Rational    3  126deg 32' 12"
Exif.GPSInfo.GPSAltitudeRef                  Byte        1  Above sea level
Exif.GPSInfo.GPSAltitude                     Rational    1  16.3 m

$ identify -verbose IMAGE.JPG | grep -i gps
    exif:GPSInfo: 6406
    exif:GPSVersionID: ....
    exif:GPSLatitudeRef: N
    exif:GPSLatitude: 37/1, 17/1, 8367/625
    exif:GPSLongitudeRef: E
    exif:GPSLongitude: 126/1, 32/1, 15411/1250
    exif:GPSAltitudeRef: .
    exif:GPSAltitude: 21229/1303
 */
