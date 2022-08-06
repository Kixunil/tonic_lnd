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
```rust
// This program accepts three arguments: address, cert file, macaroon file
#[tokio::main]
async fn main() {
    let mut args = std::env::args_os();
    args.next().expect("not even zeroth arg given");
    let address = args
        .next()
        .expect("missing arguments: address, cert file, macaroon file");
    let cert_file = args
        .next()
        .expect("missing arguments: cert file, macaroon file")
        .into_string()
        .expect("cert_file is not UTF-8");
    let macaroon_file = args
        .next()
        .expect("missing argument: macaroon file")
        .into_string()
        .expect("cert_file is not UTF-8");
    let address = address.into_string().expect("address is not UTF-8");

    // Connecting to LND requires only address, cert file, and macaroon file
    let mut client = tonic_lnd::connect_lightning(address, cert_file, macaroon_file)
        .await
        .expect("failed to connect");

    let info = client
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
use tonic_openssl::ALPN_H2_WIRE;
use tower::Service;

pub mod autopilotrpc {
    tonic::include_proto!("autopilotrpc");
}

pub mod chainrpc {
    tonic::include_proto!("chainrpc");
}

pub mod devrpc {
    tonic::include_proto!("devrpc");
}

pub mod invoicesrpc {
    tonic::include_proto!("invoicesrpc");
}

pub mod lnrpc {
    tonic::include_proto!("lnrpc");
}

pub mod lnclipb {
    tonic::include_proto!("lnclipb");
}

pub mod neutrinorpc {
    tonic::include_proto!("neutrinorpc");
}

pub mod peersrpc {
    tonic::include_proto!("peersrpc");
}

pub mod routerrpc {
    tonic::include_proto!("routerrpc");
}

pub mod signrpc {
    tonic::include_proto!("signrpc");
}

pub mod verrpc {
    tonic::include_proto!("verrpc");
}

pub mod walletrpc {
    tonic::include_proto!("walletrpc");
}

pub mod watchtowerrpc {
    tonic::include_proto!("watchtowerrpc");
}

pub mod wtclientrpc {
    tonic::include_proto!("wtclientrpc");
}

/// [`tonic::Status`] is re-exported as `LndClientError` for convenience.
pub type LndClientError = tonic::Status;

pub type LndAutopilotClient = crate::autopilotrpc::autopilot_client::AutopilotClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndChainClient = crate::chainrpc::chain_notifier_client::ChainNotifierClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndDevClient = crate::devrpc::dev_client::DevClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndInvoicesClient = crate::invoicesrpc::invoices_client::InvoicesClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndLightningClient = crate::lnrpc::lightning_client::LightningClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndNeutrinoClient = crate::neutrinorpc::neutrino_kit_client::NeutrinoKitClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndPeersClient = crate::peersrpc::peers_client::PeersClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndRouterClient = crate::routerrpc::router_client::RouterClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndSignerClient = crate::signrpc::signer_client::SignerClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndVersionerClient = crate::verrpc::versioner_client::VersionerClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndWalletClient = crate::walletrpc::wallet_kit_client::WalletKitClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndWatchtowerClient = crate::watchtowerrpc::watchtower_client::WatchtowerClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

pub type LndWtcClient = crate::wtclientrpc::watchtower_client_client::WatchtowerClientClient<
    tonic::codegen::InterceptedService<MyChannel, MacaroonInterceptor>,
>;

mod error;

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

async fn get_channel(
    lnd_address: String,
    lnd_tls_cert_path: String,
) -> Result<MyChannel, Box<dyn std::error::Error>> {
    let pem = tokio::fs::read(lnd_tls_cert_path).await.ok();
    let uri = lnd_address.parse::<Uri>().unwrap();
    let channel = MyChannel::new(pem, uri).await?;
    Ok(channel)
}

async fn get_macaroon_interceptor(
    lnd_macaroon_path: String,
) -> Result<MacaroonInterceptor, Box<dyn std::error::Error>> {
    // TODO: don't use unwrap.
    let macaroon = load_macaroon(lnd_macaroon_path).await.unwrap();
    Ok(MacaroonInterceptor { macaroon })
}

pub async fn connect_autopilot(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndAutopilotClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client = crate::autopilotrpc::autopilot_client::AutopilotClient::with_interceptor(
        channel,
        interceptor,
    );
    Ok(client)
}

pub async fn connect_chain_notifier(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndChainClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client = crate::chainrpc::chain_notifier_client::ChainNotifierClient::with_interceptor(
        channel,
        interceptor,
    );
    Ok(client)
}

pub async fn connect_dev(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndDevClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client = crate::devrpc::dev_client::DevClient::with_interceptor(channel, interceptor);
    Ok(client)
}

pub async fn connect_invoices(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndInvoicesClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client =
        crate::invoicesrpc::invoices_client::InvoicesClient::with_interceptor(channel, interceptor);
    Ok(client)
}

pub async fn connect_lightning(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndLightningClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client =
        crate::lnrpc::lightning_client::LightningClient::with_interceptor(channel, interceptor);
    Ok(client)
}

pub async fn connect_neutrino(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndNeutrinoClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client = crate::neutrinorpc::neutrino_kit_client::NeutrinoKitClient::with_interceptor(
        channel,
        interceptor,
    );
    Ok(client)
}

pub async fn connect_peers(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndPeersClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client = crate::peersrpc::peers_client::PeersClient::with_interceptor(channel, interceptor);
    Ok(client)
}

pub async fn connect_router(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndRouterClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client =
        crate::routerrpc::router_client::RouterClient::with_interceptor(channel, interceptor);
    Ok(client)
}

pub async fn connect_signer(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndSignerClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client =
        crate::signrpc::signer_client::SignerClient::with_interceptor(channel, interceptor);
    Ok(client)
}

pub async fn connect_versioner(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndVersionerClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client =
        crate::verrpc::versioner_client::VersionerClient::with_interceptor(channel, interceptor);
    Ok(client)
}

pub async fn connect_wallet(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndWalletClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client = crate::walletrpc::wallet_kit_client::WalletKitClient::with_interceptor(
        channel,
        interceptor,
    );
    Ok(client)
}

pub async fn connect_watchtower(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndWatchtowerClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client = crate::watchtowerrpc::watchtower_client::WatchtowerClient::with_interceptor(
        channel,
        interceptor,
    );
    Ok(client)
}

pub async fn connect_wtc(
    lnd_address: String,
    lnd_tls_cert_path: String,
    lnd_macaroon_path: String,
) -> Result<LndWtcClient, Box<dyn std::error::Error>> {
    let channel = get_channel(lnd_address, lnd_tls_cert_path).await?;
    let interceptor = get_macaroon_interceptor(lnd_macaroon_path).await?;
    let client =
        crate::wtclientrpc::watchtower_client_client::WatchtowerClientClient::with_interceptor(
            channel,
            interceptor,
        );
    Ok(client)
}

#[derive(Clone)]
pub struct MyChannel {
    uri: Uri,
    client: MyClient,
}

#[derive(Clone)]
enum MyClient {
    ClearText(Client<HttpConnector, BoxBody>),
    Tls(Client<HttpsConnector<HttpConnector>, BoxBody>),
}

impl MyChannel {
    pub async fn new(certificate: Option<Vec<u8>>, uri: Uri) -> Result<Self, Box<dyn Error>> {
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        let client = match certificate {
            None => MyClient::ClearText(Client::builder().http2_only(true).build(http)),
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
                MyClient::Tls(Client::builder().http2_only(true).build(https))
            }
        };

        Ok(Self { client, uri })
    }
}

impl Service<Request<BoxBody>> for MyChannel {
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
            MyClient::ClearText(client) => client.request(req),
            MyClient::Tls(client) => client.request(req),
        }
    }
}
