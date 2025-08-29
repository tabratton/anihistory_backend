use crate::get_ext;
use aws_config::Region;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use tracing::error;

static BUCKET_NAME: &str = "anihistory-images";

pub enum ImageTypes {
    Anime,
    User,
}

impl Display for ImageTypes {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageTypes::Anime => write!(f, "anime"),
            ImageTypes::User => write!(f, "user"),
        }
    }
}

fn naive_mime(ext: &String) -> String {
    if ext.contains("jp") {
        "image/jpeg".to_owned()
    } else {
        format!("image/{}", ext)
    }
}

#[derive(Clone)]
pub struct S3Client {
    client: Arc<Client>,
}

impl S3Client {
    pub async fn new() -> Self {
        let region_provider = RegionProviderChain::first_try(Region::new("us-east-1"));
        let shared_config = aws_config::from_env().region(region_provider).load().await;
        Self {
            client: Arc::new(Client::new(&shared_config)),
        }
    }

    pub async fn upload_to_s3(
        &self,
        prefix: ImageTypes,
        id: i32,
        url: &String,
    ) -> Result<(), anyhow::Error> {
        let content = download_image(url).await?;
        let ext = get_ext(url);
        let key = format!("assets/images/{prefix}_{id}.{ext}");

        let body = ByteStream::from(content);
        match self
            .client
            .put_object()
            .bucket(BUCKET_NAME)
            .key(key)
            .content_type(naive_mime(&ext))
            .body(body)
            .send()
            .await
        {
            Ok(_) => Ok(()),
            Err(error) => {
                error!("error uploading assets/images/{prefix}_{id}.{ext} to S3. Error: {error}",);
                Err(error)?
            }
        }
    }
}

async fn download_image(url: &String) -> Result<Vec<u8>, anyhow::Error> {
    Ok(reqwest::get(url).await?.bytes().await?.into())
}
