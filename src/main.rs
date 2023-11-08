use std::{collections::HashSet, env};
use std::time::Duration;

use futures::future::join_all;
use reqwest::{Certificate, Response};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

const STEAM_CA: &[u8] = include_bytes!("../DigiCertHighAssuranceEVRootCA.crt.pem");

#[derive(Serialize, Deserialize, Debug)]
struct IpListItem {
    address: String,
    list: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mikrotik_address = env::var("MIKROTIK_ADDRESS").unwrap_or("192.168.88.1".to_string());
    let mikrotik_user = env::var("MIKROTIK_USER").unwrap_or("admin".to_string());
    let mikrotik_pass = env::var("MIKROTIK_PASS").unwrap_or("".to_string());
    let server_port = env::var("MIKROTIK_FETCH_PORT").unwrap_or("".to_string());
    let list_name = env::var("MIKROTIK_ADDRESS_LIST_NAME").unwrap_or("steam_cm".to_string());
    if server_port.is_empty() {
        let ip_ports = get_cm_servers().await?;
        let distinct_ips = remove_ports_and_distinct_ips(&ip_ports);
        update_ip_list(
            &mikrotik_address,
            &mikrotik_user,
            &mikrotik_pass,
            &list_name,
            distinct_ips,
        )
            .await?;
        Ok(())
    } else {
        let listen_addr = format!("0.0.0.0:{}", server_port);
        let listener = TcpListener::bind(&listen_addr).await?;
        println!("Config Server listening on: {}", listen_addr);
        loop {
            let (stream, _) = listener.accept().await?;
            let ip_ports = get_cm_servers().await?;
            let distinct_ips = remove_ports_and_distinct_ips(&ip_ports);
            let body = generate_mikrotik_rsc(distinct_ips, &list_name);
            handle_connection(stream, &body).await;
        }
    }
}

async fn update_ip_list(
    mikrotik_address: &str,
    mikrotik_user: &str,
    mikrotik_pass: &str,
    list_name: &str,
    cm_ips: Vec<String>,
) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)// Because most of users is self signed
        .connect_timeout(Duration::from_secs(5)) // 5sec is very long for LAN gateways
        .build()
        .unwrap_or_default();
    let password: Option<&str> = Some(mikrotik_pass);
    let old_list_query: serde_json::Value = client
        .get(format!(
            "https://{mikrotikAddress}/rest/ip/firewall/address-list?list={listName}",
            mikrotikAddress = mikrotik_address,
            listName = list_name
        ))
        .basic_auth(mikrotik_user, password)
        .send()
        .await?
        .json()
        .await?;
    if let Some(old_list) = old_list_query.as_array() {
        let old_list_ids: Vec<String> = old_list
            .iter()
            .map(|entry| entry[".id"].as_str().unwrap_or_default().to_owned())
            .collect();
        let mut delete_requests = vec![];
        for id in old_list_ids {
            let req = client
                .delete(format!(
                    "https://{mikrotikAddress}/rest/ip/firewall/address-list/{oldId}",
                    mikrotikAddress = mikrotik_address,
                    oldId = id
                ))
                .basic_auth(mikrotik_user, password)
                .send();
            delete_requests.push(req);
        }
        let delete_old_results = join_all(delete_requests).await;
        test_mikrotik_results(delete_old_results).await;

        let mut add_requests = vec![];
        for ip in cm_ips {
            let item = IpListItem {
                address: ip,
                list: list_name.to_string(),
            };
            let req = client
                .put(format!(
                    "https://{mikrotikAddress}/rest/ip/firewall/address-list",
                    mikrotikAddress = mikrotik_address,
                ))
                .basic_auth(mikrotik_user, password)
                .json(&item)
                .send();
            add_requests.push(req);
        }
        let add_new_results = join_all(add_requests).await;
        test_mikrotik_results(add_new_results).await;
    }
    Ok(())
}

async fn get_cm_servers() -> Result<Vec<String>, reqwest::Error> {
    let client = reqwest::Client::builder()
        .add_root_certificate(Certificate::from_pem(&STEAM_CA)?)
        .connect_timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();
    let response = client
        .get("https://api.steampowered.com/ISteamDirectory/GetCMList/v1/?format=json&cellid=0")
        .send()
        .await?;

    if response.status().is_success() {
        let data: serde_json::Value = response.json().await?;
        if let Some(serverlist) = data["response"]["serverlist"].as_array() {
            let ip_ports: Vec<String> = serverlist
                .iter()
                .map(|entry| entry.as_str().unwrap_or_default().to_owned())
                .collect();
            Ok(ip_ports)
        } else {
            eprintln!("API Error: Missing serverlist field in response JSON.");
            Ok(Vec::new())
        }
    } else {
        eprintln!("HTTP Error: HTTP CODE {}", response.status());
        Ok(Vec::new())
    }
}

async fn test_mikrotik_results(results: Vec<Result<Response, reqwest::Error>>) {
    for res in results {
        if let Ok(response) = res {
            if !response.status().is_success() {
                eprintln!(
                    "MIKROTIK ERROR: {} {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                );
            }
        } else if let Err(err) = res {
            eprintln!("Request failed: {}", err);
        }
    }
}

fn generate_mikrotik_rsc(distinct_ips: Vec<String>, list_name: &str) -> String {
    let mut ros_ip_list_actions = Vec::new();
    for ip in &distinct_ips {
        ros_ip_list_actions.push(format!(
            ":do {{add address={} list={}}} on-error={{}}",
            ip, list_name
        ));
    }

    let ros_ip_list_header = format!("/log info \"Import steam ipv4 cm server list...\"\n/ip firewall address-list remove [/ip firewall address-list find list={}]\n/ip firewall address-list", list_name);
    let ros_ip_list_actions_joined = ros_ip_list_actions.join("\n");

    return format!("{}\n{}", ros_ip_list_header, ros_ip_list_actions_joined);
}

async fn handle_connection(mut stream: TcpStream, response_body: &str) {
    let status_line = "HTTP/1.1 200 OK";
    let contents = response_body;
    let length = contents.len();
    let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
    stream
        .write_all(response.as_bytes())
        .await
        .unwrap_or_default();
}

fn remove_ports_and_distinct_ips(ip_ports: &[String]) -> Vec<String> {
    let ips: Vec<String> = ip_ports
        .iter()
        .map(|ip_port| ip_port.split(":").next().unwrap_or_default().to_owned())
        .collect();

    let distinct_ips: Vec<String> = ips
        .into_iter()
        .collect::<HashSet<String>>()
        .into_iter()
        .collect();
    distinct_ips
}
