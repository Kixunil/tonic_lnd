// This program connects to LND and prints out all incoming invoices as they settle.
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

    let mut invoice_stream = client
        .subscribe_invoices(tonic_lnd::lnrpc::InvoiceSubscription {
            add_index: 0,
            settle_index: 0,
        })
        .await
        .expect("Failed to call subscribe_invoices")
        .into_inner();

    while let Some(invoice) = invoice_stream
        .message()
        .await
        .expect("Failed to receive invoices")
    {
        if let Some(state) = tonic_lnd::lnrpc::invoice::InvoiceState::from_i32(invoice.state) {
            // If this invoice was Settled we can do something with it
            if state == tonic_lnd::lnrpc::invoice::InvoiceState::Settled {
                println!("{:?}", invoice);
            }
        }
    }
}
