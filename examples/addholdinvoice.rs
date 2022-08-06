// This program accepts three arguments: address, cert file, macaroon file
// The address must start with `https://`!

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

    // Connecting to LND requires only address, cert file, macaroon file
    let mut invoices_client = tonic_lnd::connect_invoices(address, cert_file, macaroon_file)
        .await
        .expect("failed to connect");

    let add_hold_invoice_resp = invoices_client
        .add_hold_invoice(tonic_lnd::invoicesrpc::AddHoldInvoiceRequest {
            hash: vec![0; 32],
            value: 5555,
            ..Default::default()
        })
        .await
        .expect("failed to add hold invoice");

    // We only print it here, note that in real-life code you may want to call `.into_inner()` on
    // the response to get the message.
    println!("{:#?}", add_hold_invoice_resp);
}
