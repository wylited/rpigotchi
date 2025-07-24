use dotenv::dotenv;
use std::env;

pub struct Client {
    client_id: String,
    client_secret: String
}

pub struct Token {
    access_token: String,
    refresh_token: String,
    time: u64
}

pub fn get_client_data() -> Client {
    dotenv().ok();

    let client_id = env::var("CLIENT_ID").unwrap();
    let client_secret = env::var("CLIENT_SECRET").unwrap();

    Client {
        client_id,
        client_secret
    }
}