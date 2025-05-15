// src/main.rs
use anyhow::{Context as AnyhowContext, Result};
use futures::future::join_all;
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::path::PathBuf;
use clap::Parser;

// Структура для хранения конфигурации из YAML
#[derive(Debug, Deserialize)]
struct Config {
    rpc_url: String,
    wallets: Vec<String>,
}

// Структура для парсинга ответа Solana JSON RPC API
#[derive(Debug, Deserialize)]
struct RpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    result: Option<Balance>,
    error: Option<RpcError>,
    #[allow(dead_code)]
    id: u64,
}

#[derive(Debug, Deserialize)]
struct Balance {
    value: u64,
    #[allow(dead_code)]
    context: BalanceContext,
}

#[derive(Debug, Deserialize)]
struct BalanceContext {
    #[allow(dead_code)]
    slot: u64,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

// Структура для хранения результатов балансов
#[derive(Debug)]
struct WalletBalance {
    address: String,
    balance: f64,
}

// Структура для параметров командной строки
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Путь к файлу конфигурации
    #[clap(short, long, default_value = "config.yaml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Парсинг аргументов командной строки
    let args = Args::parse();
    
    // Загрузка конфигурации
    let config = load_config(&args.config)?;
    
    // Получение балансов
    let balances = get_wallet_balances(&config).await?;
    
    // Вывод результатов
    println!("Balances for {} wallets:", balances.len());
    for balance in balances {
        println!("{}: {} SOL", balance.address, balance.balance);
    }
    
    Ok(())
}

// Загрузка конфигурации из YAML файла
fn load_config(path: &PathBuf) -> Result<Config> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open config file: {:?}", path))?;
    
    let config: Config = serde_yaml::from_reader(file)
        .with_context(|| "Failed to parse config file")?;
    
    Ok(config)
}

// Получение баланса для одного кошелька
async fn get_single_balance(rpc_url: &str, wallet: &str) -> Result<WalletBalance> {
    let client = reqwest::Client::new();
    
    let request_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getBalance",
        "params": [wallet]
    });
    
    let response = client.post(rpc_url)
        .json(&request_body)
        .send()
        .await
        .with_context(|| format!("Failed to request balance for wallet: {}", wallet))?;
    
    let rpc_response: RpcResponse = response.json().await
        .with_context(|| format!("Failed to parse response for wallet: {}", wallet))?;
    
    if let Some(error) = rpc_response.error {
        anyhow::bail!("RPC error for wallet {}: {} (code: {})", wallet, error.message, error.code);
    }
    
    let balance = rpc_response.result
        .with_context(|| format!("No balance result for wallet: {}", wallet))?;
    
    // Преобразование в SOL (1 SOL = 1_000_000_000 lamports)
    let sol_balance = balance.value as f64 / 1_000_000_000.0;
    
    Ok(WalletBalance {
        address: wallet.to_string(),
        balance: sol_balance,
    })
}

// Получение балансов для всех кошельков параллельно
async fn get_wallet_balances(config: &Config) -> Result<Vec<WalletBalance>> {
    let mut tasks = Vec::new();
    
    for wallet in &config.wallets {
        let rpc_url = config.rpc_url.clone();
        let wallet_clone = wallet.clone();
        
        // Создаем задачу для каждого кошелька
        let task = tokio::spawn(async move {
            get_single_balance(&rpc_url, &wallet_clone).await
        });
        
        tasks.push(task);
    }
    
    // Ожидаем завершения всех задач
    let results = join_all(tasks).await;
    
    // Обрабатываем результаты
    let mut balances = Vec::new();
    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(Ok(balance)) => balances.push(balance),
            Ok(Err(e)) => println!("Error fetching balance for wallet {}: {}", config.wallets[i], e),
            Err(e) => println!("Task error for wallet {}: {}", config.wallets[i], e),
        }
    }
    
    Ok(balances)
}