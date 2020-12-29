use std::ops::Range;
#[cfg(feature = "download")]
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
#[cfg(feature = "download")]
use futures::Stream as FuturesStream;
use mime::Mime;
use reqwest::Client;
#[cfg(feature = "download")]
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    stream::StreamExt,
};

use crate::{Result, TryCollect, VideoDetails};
use crate::error::Error;
use crate::video_info::player_response::streaming_data::{AudioQuality, ColorInfo, FormatType, MimeType, ProjectionType, Quality, QualityLabel, RawFormat, SignatureCipher};

use self::itags::ItagProfile;

mod itags;

#[derive(Clone, Debug)]
pub struct Stream {
    pub mime: Mime,
    pub codecs: Vec<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub is_dash: bool,
    pub abr: Option<u64>,
    pub resolution: Option<u64>,
    pub is_progressive: bool,
    pub includes_video_track: bool,
    pub includes_audio_track: bool,
    pub is_3d: bool,
    pub is_hdr: bool,
    pub is_live: bool,
    pub format_type: Option<FormatType>,
    pub approx_duration_ms: u64,
    pub audio_channels: Option<u8>,
    pub audio_quality: Option<AudioQuality>,
    pub audio_sample_rate: Option<u64>,
    pub average_bitrate: Option<u64>,
    pub bitrate: Option<u64>,
    pub color_info: Option<ColorInfo>,
    pub content_length: Option<u64>,
    pub fps: u8,
    pub height: Option<u64>,
    pub high_replication: Option<bool>,
    pub index_range: Option<Range<u64>>,
    pub init_range: Option<Range<u64>>,
    pub is_otf: bool,
    pub itag: u64,
    pub last_modified: DateTime<Utc>,
    pub loudness_db: Option<f64>,
    pub projection_type: ProjectionType,
    pub quality: Quality,
    pub quality_label: Option<QualityLabel>,
    pub signature_cipher: SignatureCipher,
    pub width: Option<u64>,
    pub video_details: Arc<VideoDetails>,
    client: Client,
}


impl Stream {
    pub fn from_raw_format(raw_format: RawFormat, client: Client, video_details: Arc<VideoDetails>) -> Result<Self> {
        let (video_codec, audio_codec) = Self::parse_codecs(
            &raw_format.mime_type
        )?;
        let itag_profile = ItagProfile::from_itag(raw_format.itag);

        Ok(Self {
            is_progressive: is_progressive(&raw_format.mime_type.codecs),
            includes_video_track: includes_video_track(&raw_format.mime_type.codecs, &raw_format.mime_type.mime),
            includes_audio_track: includes_audio_track(&raw_format.mime_type.codecs, &raw_format.mime_type.mime),
            mime: raw_format.mime_type.mime,
            codecs: raw_format.mime_type.codecs,
            video_codec,
            audio_codec,
            is_dash: itag_profile.is_dash,
            abr: itag_profile.abr,
            resolution: itag_profile.resolution,
            is_3d: itag_profile.is_3d,
            is_hdr: itag_profile.is_hdr,
            is_live: itag_profile.is_live,
            format_type: raw_format.format_type,
            approx_duration_ms: raw_format.approx_duration_ms,
            audio_channels: raw_format.audio_channels,
            audio_quality: raw_format.audio_quality,
            audio_sample_rate: raw_format.audio_sample_rate,
            average_bitrate: raw_format.average_bitrate,
            bitrate: raw_format.bitrate,
            color_info: raw_format.color_info,
            content_length: raw_format.content_length,
            fps: raw_format.fps,
            height: raw_format.height,
            high_replication: raw_format.high_replication,
            index_range: raw_format.index_range,
            init_range: raw_format.init_range,
            is_otf: raw_format.format_type.contains(&FormatType::Otf),
            itag: raw_format.itag,
            last_modified: raw_format.last_modified,
            loudness_db: raw_format.loudness_db,
            projection_type: raw_format.projection_type,
            quality: raw_format.quality,
            quality_label: raw_format.quality_label,
            signature_cipher: raw_format.signature_cipher,
            width: raw_format.width,
            client,
            video_details,
        })
    }

    // todo: download in ranges
    // todo: blocking download

    /// Attempts to downloads the `Stream`s resource.
    /// This will download the video to <video_id>.mp4 in the current working directory.
    #[inline]
    #[cfg(feature = "download")]
    pub async fn download(&self) -> Result<PathBuf> {
        let path = Path::new(self.video_details.video_id.as_str())
            .with_extension("mp4");
        self.download_to(&path)
            .await
            .map(|_| path)
    }

    /// Attempts to downloads the `Stream`s resource.
    /// This will download the video to <video_id>.mp4 in the provided directory. 
    #[inline]
    #[cfg(feature = "download")]
    pub async fn download_to_dir<P: AsRef<Path>>(&self, dir: P) -> Result<PathBuf> {
        let mut path = dir
            .as_ref()
            .join(self.video_details.video_id.as_str());
        path.set_extension("mp4");
        self.download_to(&path)
            .await
            .map(|_| path)
    }

    /// Attempts to downloads the `Stream`s resource.
    /// This will download the video to the provided file path.
    #[cfg(feature = "download")]
    pub async fn download_to<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        log::trace!("download_to: {:?}", path.as_ref());
        let mut file = File::create(&path).await?;

        match self.download_full(&self.signature_cipher.url, &mut file).await {
            Ok(_) => {
                log::info!(
                    "downloaded {} successfully to {:?}",
                    self.video_details.video_id, path.as_ref()
                );
                Ok(())
            }
            Err(Error::Request(e)) if e.status().contains(&reqwest::StatusCode::NOT_FOUND) => {
                log::error!("failed to download {}: {:?}", self.video_details.video_id, e);
                // Some adaptive streams need to be requested with sequence numbers
                self.download_full_seq(&mut file)
                    .await
            }
            Err(e) => {
                log::error!("failed to download {}: {:?}", self.video_details.video_id, e);
                drop(file);
                tokio::fs::remove_file(path).await?;
                Err(e)
            }
        }
    }

    #[cfg(feature = "download")]
    async fn download_full_seq(&self, file: &mut File) -> Result<()> {
        // fixme: this implementation is **not** tested yet!
        // To test it, I would need an url of a video, which does require sequenced downloading.
        log::warn!(
            "`download_full_seq` is not tested yet and probably broken!\n\
            Please open a GitHub issue and paste the whole warning message plus the videos Id in:\n\
            url: {}", self.signature_cipher.url.as_str()
        );

        let mut url = self.signature_cipher.url.clone();
        let base_query = url
            .query()
            .map(str::to_owned)
            .unwrap_or_else(|| String::new());

        // The 0th sequential request provides the file headers, which tell us
        // information about how the file is segmented.
        Self::set_url_seq_query(&mut url, &base_query, 0);
        let res = self.get(&url).await?;
        let segment_count = Stream::extract_segment_count(&res)?;
        Self::write_stream_to_file(res.bytes_stream(), file).await?;

        for i in 1..segment_count {
            Self::set_url_seq_query(&mut url, &base_query, i);
            self.download_full(&url, file).await?;
        }

        Ok(())
    }

    #[inline]
    #[cfg(feature = "download")]
    async fn download_full(&self, url: &url::Url, file: &mut File) -> Result<()> {
        let res = self.get(url).await?;
        Self::write_stream_to_file(res.bytes_stream(), file).await
    }

    #[inline]
    #[cfg(feature = "download")]
    async fn get(&self, url: &url::Url) -> Result<reqwest::Response> {
        log::trace!("get: {}", url.as_str());
        Ok(
            self.client
                .get(url.as_str())
                .send()
                .await?
                .error_for_status()?
        )
    }

    #[inline]
    #[cfg(feature = "download")]
    async fn write_stream_to_file(mut stream: impl FuturesStream<Item=reqwest::Result<bytes::Bytes>> + Unpin, file: &mut File) -> Result<()> {
        while let Some(chunk) = stream.next().await {
            file
                .write_all(&chunk?)
                .await?;
        }
        Ok(())
    }

    #[inline]
    #[cfg(feature = "download")]
    fn set_url_seq_query(url: &mut url::Url, base_query: &str, sq: u64) {
        url.set_query(Some(&base_query));
        url
            .query_pairs_mut()
            .append_pair("sq", &sq.to_string());
    }

    #[inline]
    #[cfg(feature = "download")]
    fn extract_segment_count(res: &reqwest::Response) -> Result<u64> {
        Ok(
            res
                .headers()
                .get("Segment-Count")
                .ok_or_else(|| Error::UnexpectedResponse(
                    "sequence download request did not contain a Segment-Count".into()
                ))?
                .to_str()
                .map_err(|_| Error::UnexpectedResponse(
                    "Segment-Count is not valid utf-8".into()
                ))?
                .parse::<u64>()
                .map_err(|_| Error::UnexpectedResponse(
                    "Segment-Count could not be parsed into an integer".into()
                ))?
        )
    }

    #[inline]
    #[cfg(feature = "download")]
    pub async fn content_length(&self) -> Result<u64> {
        if let Some(content_length) = self.content_length {
            return Ok(content_length);
        }

        self.client
            .head(self.signature_cipher.url.as_str())
            .send()
            .await?
            .error_for_status()?
            .content_length()
            .ok_or(Error::UnexpectedResponse(
                "the response did not contain a valid content-length field".into()
            ))
    }

    #[inline]
    fn parse_codecs(MimeType { mime, codecs }: &MimeType) -> Result<(Option<String>, Option<String>)> {
        if !is_adaptive(codecs) {
            let (video, audio) = codecs
                .iter()
                .try_collect()
                .ok_or(Error::UnexpectedResponse(format!(
                    "expected codecs to contains 2 elements, got {}, `{:?}`",
                    codecs.len(), codecs
                ).into()))?;
            Ok((Some(video.to_owned()), Some(audio.to_owned())))
        } else if includes_video_track(codecs, mime) {
            Ok((codecs.first().cloned(), None))
        } else if includes_audio_track(codecs, mime) {
            Ok((None, codecs.first().cloned()))
        } else {
            Ok((None, None))
        }
    }
}

#[inline]
fn is_adaptive(codecs: &Vec<String>) -> bool {
    codecs.len() % 2 != 0
}

#[inline]
fn includes_video_track(codecs: &Vec<String>, mime: &Mime) -> bool {
    is_progressive(codecs) || mime.type_() == "video"
}

#[inline]
fn includes_audio_track(codecs: &Vec<String>, mime: &Mime) -> bool {
    is_progressive(codecs) || mime.type_() == "audio"
}

#[inline]
fn is_progressive(codecs: &Vec<String>) -> bool {
    !is_adaptive(codecs)
}