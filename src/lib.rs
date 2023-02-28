// include_str! is not supported in attributes yet
#![doc = r###"
Rust implementation of LND RPC client using async GRPC library `tonic`.

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
// This program accepts three arguments: address, cert file, macaroon file
// The address must start with `https://`!

#[tokio::main]
async fn main() {
    let mut args = std::env::args_os();
    args.next().expect("not even zeroth arg given");
    let address = args.next().expect("missing arguments: address, cert file, macaroon file");
    let cert_file = args.next().expect("missing arguments: cert file, macaroon file");
    let macaroon_file = args.next().expect("missing argument: macaroon file");
    let address = address.into_string().expect("address is not UTF-8");

    // Connecting to LND requires only address, cert file, and macaroon file
    let mut client = tonic_lnd::connect(address, cert_file, macaroon_file)
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

1.48.0

However some dependencies may need to be downgraded using `cargo update -p <package> --precise <version>`.
`Cargo-msrv.lock` is included for reference - it is the lock file used to test the crate and contains known working versions of dependencies.

The MSRV won't be bumped sooner than Debian Bookworm release.

## License

MITNFA
"###]

/// This is part of public interface so it's re-exported.
pub extern crate tonic;

use std::path::{Path, PathBuf};
use std::convert::TryInto;
pub use error::ConnectError;
use error::InternalConnectError;
use tonic::codegen::InterceptedService;
#[allow(unused_imports)]
use tonic::transport::Channel;

#[cfg(feature = "tracing")]
use tracing;

/// Convenience type alias for lightning client.
#[cfg(feature = "lightningrpc")]
pub type LightningClient = lnrpc::lightning_client::LightningClient<InterceptedService<Channel, MacaroonInterceptor>>;


/// Convenience type alias for wallet client.
#[cfg(feature = "walletrpc")]
pub type WalletKitClient = walletrpc::wallet_kit_client::WalletKitClient<InterceptedService<Channel, MacaroonInterceptor>>;

/// Convenience type alias for peers service client.
pub type PeersClient =
    peersrpc::peers_client::PeersClient<InterceptedService<Channel, MacaroonInterceptor>>;

// Convenience type alias for signer client.
#[cfg(feature = "signrpc")]
pub type SignerClient = signrpc::signer_client::SignerClient<InterceptedService<Channel, MacaroonInterceptor>>;

/// The client returned by `connect` function
///
/// This is a convenience type which you most likely want to use instead of raw client.
pub struct Client {
    #[cfg(feature = "lightningrpc")]
    lightning: LightningClient,
    #[cfg(feature = "walletrpc")]
    wallet: WalletKitClient,
    #[cfg(feature = "signrpc")]
    signer: SignerClient,
    peers: PeersClient,
}

impl Client {
    /// Returns the lightning client.
    #[cfg(feature = "lightningrpc")]
    pub fn lightning(&mut self) -> &mut LightningClient {
        &mut self.lightning
    }

    /// Returns the wallet client.
    #[cfg(feature = "walletrpc")]
    pub fn wallet(&mut self) -> &mut WalletKitClient {
        &mut self.wallet
    }

    /// Returns the signer client.
    #[cfg(feature = "signrpc")]
    pub fn signer(&mut self) -> &mut SignerClient {
        &mut self.signer
    }

    /// Returns the peers client.
    pub fn peers(&mut self) -> &mut PeersClient {
        &mut self.peers
    }
}

/// [`tonic::Status`] is re-exported as `Error` for convenience.
pub type Error = tonic::Status;

mod error;

macro_rules! try_map_err {
    ($result:expr, $mapfn:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => return Err($mapfn(error).into()),
        }
    }
}

/// Messages and other types generated by `tonic`/`prost`
///
/// This is the go-to module you will need to look in to find documentation on various message
/// types. However it may be better to start from methods on the [`LightningClient`](lnrpc::lightning_client::LightningClient) type.
#[cfg(feature = "lightningrpc")]
pub mod lnrpc {
    tonic::include_proto!("lnrpc");
}

#[cfg(feature = "walletrpc")]
pub mod walletrpc {
    tonic::include_proto!("walletrpc");
}

#[cfg(feature = "signrpc")]
pub mod signrpc {
    tonic::include_proto!("signrpc");
}

pub mod peersrpc {
    tonic::include_proto!("peersrpc");
}

/// Supplies requests with macaroon
#[derive(Clone)]
pub struct MacaroonInterceptor {
    macaroon: String,
}

impl tonic::service::Interceptor for MacaroonInterceptor {
    fn call(&mut self, mut request: tonic::Request<()>) -> Result<tonic::Request<()>, Error> {
        request
            .metadata_mut()
            .insert("macaroon", tonic::metadata::MetadataValue::from_str(&self.macaroon).expect("hex produced non-ascii"));
        Ok(request)
    }
}

async fn load_macaroon(path: impl AsRef<Path> + Into<PathBuf>) -> Result<String, InternalConnectError> {
    let macaroon = tokio::fs::read(&path)
        .await
        .map_err(|error| InternalConnectError::ReadFile { file: path.into(), error, })?;
    Ok(hex::encode(&macaroon))
}

/// Connects to LND using given address and credentials
///
/// This function does all required processing of the cert file and macaroon file, so that you
/// don't have to. The address must begin with "https://", though.
///
/// This is considered the recommended way to connect to LND. An alternative function to use
/// already-read certificate or macaroon data is currently **not** provided to discourage such use.
/// LND occasionally changes that data which would lead to errors and in turn in worse application.
///
/// If you have a motivating use case for use of direct data feel free to open an issue and
/// explain.
#[cfg_attr(feature = "tracing", tracing::instrument(name = "Connecting to LND"))]
pub async fn connect<A, CP, MP>(address: A, cert_file: CP, macaroon_file: MP) -> Result<Client, ConnectError> where A: TryInto<tonic::transport::Endpoint> + std::fmt::Debug + ToString, <A as TryInto<tonic::transport::Endpoint>>::Error: std::error::Error + Send + Sync + 'static, CP: AsRef<Path> + Into<PathBuf> + std::fmt::Debug, MP: AsRef<Path> + Into<PathBuf> + std::fmt::Debug {
    let address_str = address.to_string();
    #[allow(unused_variables)]
    let conn = try_map_err!(address
        .try_into(), |error| InternalConnectError::InvalidAddress { address: address_str.clone(), error: Box::new(error), })
        .tls_config(tls::config(cert_file).await?)
        .map_err(InternalConnectError::TlsConfig)?
        .connect()
        .await
        .map_err(|error| InternalConnectError::Connect { address: address_str, error, })?;

    let macaroon = load_macaroon(macaroon_file).await?;

    #[allow(unused_variables)]
    let interceptor = MacaroonInterceptor { macaroon, };

    let client = Client {
        #[cfg(feature = "lightningrpc")]
        lightning: lnrpc::lightning_client::LightningClient::with_interceptor(conn.clone(), interceptor.clone()),
        #[cfg(feature = "walletrpc")]
        wallet: walletrpc::wallet_kit_client::WalletKitClient::with_interceptor(conn, interceptor),
        peers: peersrpc::peers_client::PeersClient::with_interceptor(
            conn.clone(),
            interceptor.clone(),
        ),
        #[cfg(feature = "signrpc")]
        signer: signrpc::signer_client::SignerClient::with_interceptor(conn, interceptor),
    };
    Ok(client)
}

mod tls {
    use std::path::{Path, PathBuf};
    use rustls::{RootCertStore, Certificate, TLSError, ServerCertVerified};
    use webpki::DNSNameRef;
    use crate::error::{ConnectError, InternalConnectError};

    pub(crate) async fn config(path: impl AsRef<Path> + Into<PathBuf>) -> Result<tonic::transport::ClientTlsConfig, ConnectError> {
        let mut tls_config = rustls::ClientConfig::new();
        tls_config.dangerous().set_certificate_verifier(std::sync::Arc::new(CertVerifier::load(path).await?));
        tls_config.set_protocols(&["h2".into()]);
        Ok(tonic::transport::ClientTlsConfig::new()
            .rustls_client_config(tls_config))
    }

    pub(crate) struct CertVerifier {
        certs: Vec<Vec<u8>>
    }

    impl CertVerifier {
        pub(crate) async fn load(path: impl AsRef<Path> + Into<PathBuf>) -> Result<Self, InternalConnectError> {
            let contents = try_map_err!(tokio::fs::read(&path).await,
                |error| InternalConnectError::ReadFile { file: path.into(), error });
            let mut reader = &*contents;

            let certs = try_map_err!(rustls_pemfile::certs(&mut reader),
                |error| InternalConnectError::ParseCert { file: path.into(), error });

            #[cfg(feature = "tracing")] {
                tracing::debug!("Certificates loaded (Count: {})", certs.len());
            }

            Ok(CertVerifier {
                certs: certs,
            })
        }
    }

    impl rustls::ServerCertVerifier for CertVerifier {
        fn verify_server_cert(&self, _roots: &RootCertStore, presented_certs: &[Certificate], _dns_name: DNSNameRef<'_>, _ocsp_response: &[u8]) -> Result<ServerCertVerified, TLSError> {
            
            if self.certs.len() != presented_certs.len() {
                return Err(TLSError::General(format!("Mismatched number of certificates (Expected: {}, Presented: {})", self.certs.len(), presented_certs.len())));
            }
            
            for (c, p) in self.certs.iter().zip(presented_certs.iter()) {
                if *p.0 != **c {
                    return Err(TLSError::General(format!("Server certificates do not match ours")));
                } else {
                    #[cfg(feature = "tracing")] {
                        tracing::trace!("Confirmed certificate match");
                    }
                }
            }

            Ok(ServerCertVerified::assertion())
        }
    }
}
