// This example walks through how to initialize an LND wallet 
// AN IMPORTANT NOTE: Be sure to know what you're doing when interacting with this. The first step of initializing LND
// is to create a seed. Don't lose this seed or you'll lose the bitcoin tied to the account.
//
// This program accepts two arguments: an address and cert file
// The address must start with `https://`!

#[tokio::main]
async fn main() {
    let mut args = std::env::args_os();
    args.next().expect("not even zeroth arg given");
    let address = args.next().expect("missing arguments: address and cert file");
    let cert_file = args.next().expect("missing arguments: cert file");
    let address = address.into_string().expect("address is not UTF-8");

    // Connecting to the wallet unlocker requires only an address and cert file.
    let mut unlocker = tonic_lnd::connect_wallet_unlocker(address, cert_file)
        .await
        .expect("failed to connect");

    // Generate wallet seed, which we need to initialize the lnd wallet.
    let request = tonic_lnd::lnrpc::GenSeedRequest {
        ..Default::default()
    };
    let resp = unlocker
        .client()
        .gen_seed(request)
        .await
        .expect("failed to generate seed");

    println!("{:#?}", resp);

    // Attempt to initialize wallet.
    let init_request = tonic_lnd::lnrpc::InitWalletRequest {
        wallet_password: "password".as_bytes().to_vec(),
        cipher_seed_mnemonic: resp.into_inner().cipher_seed_mnemonic,
        ..Default::default()
    };
    let init_resp = unlocker
        .client()
        .init_wallet(init_request)
        .await
        .expect("failed to initialize wallet");

    // We only print it here, note that in real-life code you may want to call `.into_inner()` on
    // the response to get the message.
    println!("{:#?}", init_resp);

    // Now that the wallet is initalized, any time the daemon restarts, the unlock method can be used to unlock it.
}
