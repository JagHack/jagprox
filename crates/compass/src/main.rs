//! JagProx broadcasting rendezvous + relay server ("compass"). Thin entry point; the
//! logic lives in the library so it can be integration-tested in-process.

use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let addr = std::env::var("COMPASS_ADDR").unwrap_or_else(|_| "0.0.0.0:7676".to_string());
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("compass: cannot bind {addr}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("compass: listening on {addr}");
    jagprox_compass::serve(listener).await;
}
