// include_str! is not supported in attributes yet
#![doc = r###"
Rust implementation of LND RPC client using async GRPC library `tonic-openssl`.
## About
**Warning: this crate is in early development and may have unknown problems!
Review it before using with mainnet funds!**
This crate implements LND GRPC using [`tonic`](https://docs.rs/tonic/) and [`prost`](https://docs.rs/prost/).
Apart from being up-to-date at the time of writing (:D) it also allows `aync` usage.
It contains vendored `rpc.proto` file so LND source code is not *required*
but accepts an environment variable `LND_REPO_DIR` which overrides the vendored `rpc.proto` file.
This can be used to test new features in non-released `lnd`.
(Actually, the motivating project using this library is that case. :))
## Usage
There's no setup needed beyond adding the crate to your `Cargo.toml`.
If you need to change the `rpc.proto` input set the environment variable `LND_REPO_DIR` to the directory with cloned `lnd` during build.
Here's an example of retrieving information from LND (`getinfo` call).
You can find the same example in crate root for your convenience.

```no_run
// This program accepts three arguments: host, port, cert file, macaroon file
// The address must start with `https://`!
#[tokio::main]
async fn main() {
    let mut args = std::env::args_os();
    args.next().expect("not even zeroth arg given");
    let host = args
        .next()
        .expect("missing arguments: host, port, cert file, macaroon file");
    let port = args
        .next()
        .expect("missing arguments: port, cert file, macaroon file");
    let cert_file = args
        .next()
        .expect("missing arguments: cert file, macaroon file");
    let macaroon_file = args.next().expect("missing argument: macaroon file");
    let host: String = host.into_string().expect("host is not UTF-8");
    let port: u32 = port
        .into_string()
        .expect("port is not UTF-8")
        .parse()
        .expect("port is not u32");
    let cert_file: String = cert_file.into_string().expect("cert_file is not UTF-8");
    let macaroon_file: String = macaroon_file
        .into_string()
        .expect("macaroon_file is not UTF-8");

    // Connecting to LND requires only address, cert file, and macaroon file
    let mut client = tonic_lnd::connect(host, port, cert_file, macaroon_file)
        .await
        .expect("failed to connect");

    let info = client
        .lightning()
        // All calls require at least empty parameter
        .get_info(tonic_lnd::lnrpc::GetInfoRequest {})
        .await
        .expect("failed to get info");

    // We only print it here, note that in real-life code you may want to call `.into_inner()` on
    // the response to get the message.
    println!("{:#?}", info);
}
```
## MSRV
Undetermined yet, please make suggestions.
## License
MITNFA
"###]

use error::InternalConnectError;
use hyper::client::connect::HttpConnector;
use hyper::{client::ResponseFuture, Body, Client, Request, Response, Uri};
use hyper_openssl::HttpsConnector;
use openssl::{
    ssl::{SslConnector, SslMethod},
    x509::X509,
};
use std::path::{Path, PathBuf};
use std::{error::Error, task::Poll};
use tonic::body::BoxBody;
use tonic::codegen::InterceptedService;
use tonic_openssl::ALPN_H2_WIRE;
use tower::Service;

#[cfg(feature = "tracing")]
use tracing;

/// Convenience type alias for lightning client.
pub type LightningClient =
    lnrpc::lightning_client::LightningClient<InterceptedService<SslChannel, MacaroonInterceptor>>;

/// Convenience type alias for wallet client.
pub type WalletKitClient = walletrpc::wallet_kit_client::WalletKitClient<
    InterceptedService<SslChannel, MacaroonInterceptor>,
>;

/// The client returned by `connect` function
///
/// This is a convenience type which you most likely want to use instead of raw client.
pub struct LndClient {
    lightning: LightningClient,
    wallet: WalletKitClient,
}

impl LndClient {
    /// Returns the lightning client.
    pub fn lightning(&mut self) -> &mut LightningClient {
        &mut self.lightning
    }

    /// Returns the wallet client.
    pub fn wallet(&mut self) -> &mut WalletKitClient {
        &mut self.wallet
    }
}

mod error;

/// [`tonic::Status`] is re-exported as `Error` for convenience.
pub type LndClientError = tonic::Status;

pub mod lnrpc {
    tonic::include_proto!("lnrpc");
}

pub mod walletrpc {
    tonic::include_proto!("walletrpc");
}

pub mod signrpc {
    tonic::include_proto!("signrpc");
}

/// Supplies requests with macaroon
#[derive(Clone)]
pub struct MacaroonInterceptor {
    macaroon: String,
}

impl tonic::service::Interceptor for MacaroonInterceptor {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, LndClientError> {
        request.metadata_mut().insert(
            "macaroon",
            #[allow(deprecated)]
            tonic::metadata::MetadataValue::from_str(&self.macaroon)
                .expect("hex produced non-ascii"),
        );
        Ok(request)
    }
}

async fn load_macaroon(
    path: impl AsRef<Path> + Into<PathBuf>,
) -> Result<String, InternalConnectError> {
    let macaroon =
        tokio::fs::read(&path)
            .await
            .map_err(|error| InternalConnectError::ReadFile {
                file: path.into(),
                error,
            })?;
    Ok(hex::encode(&macaroon))
}

pub async fn connect(
    lnd_host: String,
    lnd_port: u32,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndClient, Box<dyn std::error::Error>> {
    let lnd_address = format!("https://{}:{}", lnd_host, lnd_port).to_string();

    let pem = tokio::fs::read(lnd_tls_cert_path).await.ok();
    let uri = lnd_address.parse::<Uri>().unwrap();
    let channel = SslChannel::new(pem, uri).await?;

    let macaroon = load_macaroon(lnd_macaroon_path).await.unwrap();
    let interceptor = MacaroonInterceptor { macaroon };

    let client = LndClient {
        lightning: lnrpc::lightning_client::LightningClient::with_interceptor(
            channel.clone(),
            interceptor.clone(),
        ),
        wallet: walletrpc::wallet_kit_client::WalletKitClient::with_interceptor(
            channel.clone(),
            interceptor,
        ),
    };

    Ok(client)
}

#[derive(Clone)]
pub struct SslChannel {
    uri: Uri,
    client: SslClient,
}

#[derive(Clone)]
enum SslClient {
    ClearText(Client<HttpConnector, BoxBody>),
    Tls(Client<HttpsConnector<HttpConnector>, BoxBody>),
}

impl SslChannel {
    pub async fn new(certificate: Option<Vec<u8>>, uri: Uri) -> Result<Self, Box<dyn Error>> {
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        let client = match certificate {
            None => SslClient::ClearText(Client::builder().http2_only(true).build(http)),
            Some(pem) => {
                let ca = X509::from_pem(&pem[..])?;
                let mut connector = SslConnector::builder(SslMethod::tls())?;
                connector.cert_store_mut().add_cert(ca)?;
                connector.set_alpn_protos(ALPN_H2_WIRE)?;
                let mut https = HttpsConnector::with_connector(http, connector)?;
                https.set_callback(|c, _| {
                    c.set_verify_hostname(false);
                    Ok(())
                });
                SslClient::Tls(Client::builder().http2_only(true).build(https))
            }
        };

        Ok(Self { client, uri })
    }
}

impl Service<Request<BoxBody>> for SslChannel {
    type Response = Response<Body>;
    type Error = hyper::Error;
    type Future = ResponseFuture;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }

    fn call(&mut self, mut req: Request<BoxBody>) -> Self::Future {
        let uri = Uri::builder()
            .scheme(self.uri.scheme().unwrap().clone())
            .authority(self.uri.authority().unwrap().clone())
            .path_and_query(req.uri().path_and_query().unwrap().clone())
            .build()
            .unwrap();
        *req.uri_mut() = uri;
        match &self.client {
            SslClient::ClearText(client) => client.request(req),
            SslClient::Tls(client) => client.request(req),
        }
    }
}
