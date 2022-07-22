// This example only fetches and prints the node info to the standard output similarly to
// `lncli getinfo`.
//
// This program accepts four arguments: host, port, cert file, macaroon file

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

    // Connecting to LND requires only host, port, cert file, macaroon file
    let mut client = tonic_lnd::connect_lightning(host, port, cert_file, macaroon_file)
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
