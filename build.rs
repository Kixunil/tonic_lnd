use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-env-changed=LND_REPO_DIR");
    let lnd_rpc_dir_owned;
    let dir = match std::env::var_os("LND_REPO_DIR") {
        Some(lnd_repo_path) => {
            let mut lnd_rpc_dir = PathBuf::from(lnd_repo_path);
            lnd_rpc_dir.push("lnrpc");
            lnd_rpc_dir_owned = lnd_rpc_dir;
            lnd_rpc_dir_owned.display().to_string()
        }
        None => "vendor".to_string(),
    };

    let protos = vec![
        "autopilotrpc/autopilot.proto",
        "chainrpc/chainnotifier.proto",
        "devrpc/dev.proto",
        "invoicesrpc/invoices.proto",
        "lightning.proto",
        "lnclipb/lncli.proto",
        "neutrinorpc/neutrino.proto",
        "peersrpc/peers.proto",
        "routerrpc/router.proto",
        "signrpc/signer.proto",
        "verrpc/verrpc.proto",
        "walletrpc/walletkit.proto",
        "watchtowerrpc/watchtower.proto",
        "wtclientrpc/wtclient.proto",
    ];

    let proto_paths: Vec<_> = protos
        .iter()
        .map(|proto| {
            let mut path = PathBuf::from(&dir);
            path.push(proto);
            path.display().to_string()
        })
        .collect();

    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .compile(&proto_paths, &[dir])?;
    Ok(())
}
