use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{json::JsonString, serde_as};

use crate::IdBuf;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoDetails {
    pub allow_ratings: bool,
    pub author: String,
    pub average_rating: f64,
    // todo: add Type ChannelId
    pub channel_id: String,
    pub is_crawlable: bool,
    pub is_live_content: bool,
    #[serde(default)]
    pub is_live_default_broadcast: bool,
    #[serde(default)]
    pub is_low_latency_live_stream: bool,
    pub is_owner_viewing: bool,
    pub is_private: bool,
    pub is_unplugged_corpus: bool,
    #[serde(default)]
    pub key_words: Vec<String>,
    #[serde_as(as = "JsonString")]
    pub length_seconds: u64,
    pub short_description: String,
    #[serde(rename = "thumbnail")]
    #[serde(serialize_with = "Thumbnail::serialize_vec")]
    #[serde(deserialize_with = "Thumbnail::deserialize_vec")]
    pub thumbnails: Vec<Thumbnail>,
    pub title: String,
    pub video_id: IdBuf,
    #[serde_as(as = "JsonString")]
    pub view_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct Thumbnail {
    pub width: u64,
    pub height: u64,
    /// a absolute or relative url
    pub url: String,
}


impl Thumbnail {
    pub(crate) fn deserialize_vec<'de, D>(deserializer: D) -> Result<Vec<Self>, <D as Deserializer<'de>>::Error> where
        D: Deserializer<'de> {
        #[derive(Deserialize)]
        struct Thumbnails { thumbnails: Vec<Thumbnail> }

        Ok(
            Thumbnails::deserialize(deserializer)?
                .thumbnails
        )
    }
    pub(crate) fn serialize_vec<S>(thumbnails: &Vec<Thumbnail>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer {
        #[derive(Serialize)]
        struct Thumbnails<'a> { thumbnails: &'a Vec<Thumbnail> }

        Thumbnails { thumbnails }.serialize(serializer)
    }
}
