use binstalk_downloader::{download::Download, remote::Client};
use std::num::NonZeroU16;
use tracing::trace;
use url::Url;

pub async fn create_client() -> Client {
    trace!("create_client");
    Client::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        None,
        NonZeroU16::new(10).unwrap(),
        1.try_into().unwrap(),
        [],
    )
    .unwrap()
}

pub async fn download(url: &str) -> Download<'static> {
    trace!("download {}", url);
    let client = Client::new(
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        None,
        NonZeroU16::new(10).unwrap(),
        1.try_into().unwrap(),
        [],
    )
    .unwrap();
    Download::new(client, Url::parse(url).unwrap())
}

#[cfg(test)]
mod test {
    use crate::download::download;
    use binstalk_downloader::download::PkgFmt;
    use std::path::Path;
    use tempfile::tempdir;
    #[tokio::test]
    async fn test_download() {
        let url = "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.1/mujs-x86_64-unknown-linux-gnu.tar.gz";
        let files = download(url).await;
        let fmt = PkgFmt::guess_pkg_format(url).unwrap();
        let out_dir = tempdir().unwrap();
        let files = files.and_extract(fmt, out_dir.path()).await.unwrap();
        assert!(files.has_file(Path::new("mujs")));
        assert!(files.has_file(Path::new("mujs-pp")));
        assert!(files.has_file(Path::new("libmujs.a")));
    }
}
