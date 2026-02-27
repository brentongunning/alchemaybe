use mpl_core::instructions::{BurnV1Builder, CreateV1Builder};
use mpl_core::types::{Attribute, Attributes, Plugin, PluginAuthorityPair};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::Transaction;
use std::str::FromStr;
use std::sync::Arc;

pub struct SolanaConfig {
    pub rpc_client: RpcClient,
    pub server_keypair: Arc<Keypair>,
    pub collection_pubkey: Pubkey,
    pub public_base_url: String,
    pub helius_api_key: String,
    pub http_client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedCard {
    pub mint_address: String,
    pub card_id: String,
    pub name: String,
    pub image: String,
}

/// Extract card_id from a DAS item's plugins.attributes.data.attribute_list
fn extract_card_id(item: &serde_json::Value) -> Option<String> {
    item.get("plugins")?
        .get("attributes")?
        .get("data")?
        .get("attribute_list")?
        .as_array()?
        .iter()
        .find(|a| a.get("key").and_then(|k| k.as_str()) == Some("card_id"))
        .and_then(|a| a.get("value")?.as_str().map(|s| s.to_string()))
}

fn extract_name(item: &serde_json::Value) -> String {
    item.get("content")
        .and_then(|c| c.get("metadata"))
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string()
}

fn is_in_collection(item: &serde_json::Value, collection: &str) -> bool {
    item.get("grouping")
        .and_then(|g| g.as_array())
        .map(|groups| {
            groups.iter().any(|g| {
                g.get("group_key").and_then(|k| k.as_str()) == Some("collection")
                    && g.get("group_value").and_then(|v| v.as_str()) == Some(collection)
            })
        })
        .unwrap_or(false)
}

impl SolanaConfig {
    /// Load Solana config from environment variables. Returns None if not configured.
    pub fn from_env() -> Option<Self> {
        let keypair_path = std::env::var("SOLANA_KEYPAIR_PATH").ok()?;
        let rpc_url = std::env::var("SOLANA_RPC_URL").ok()?;
        let helius_api_key = std::env::var("HELIUS_API_KEY").ok()?;
        let collection_address = std::env::var("COLLECTION_ADDRESS").ok()?;
        let public_base_url =
            std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://localhost:3001".into());

        let keypair_data = std::fs::read_to_string(&keypair_path)
            .unwrap_or_else(|e| panic!("Failed to read keypair at {keypair_path}: {e}"));
        let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_data)
            .unwrap_or_else(|e| panic!("Failed to parse keypair JSON: {e}"));
        let server_keypair =
            Keypair::try_from(keypair_bytes.as_slice()).expect("Invalid keypair bytes");

        let collection_pubkey = Pubkey::from_str(&collection_address)
            .unwrap_or_else(|e| panic!("Invalid collection address {collection_address}: {e}"));

        let rpc_client = RpcClient::new_with_commitment(&rpc_url, CommitmentConfig::confirmed());
        let http_client = reqwest::Client::new();

        log::info!("Solana config loaded: collection={collection_address}");

        Some(SolanaConfig {
            rpc_client,
            server_keypair: Arc::new(server_keypair),
            collection_pubkey,
            public_base_url,
            helius_api_key,
            http_client,
        })
    }

    /// Query owned NFT cards for a wallet using Helius DAS API.
    pub async fn query_owned_cards(&self, wallet: &str) -> Result<Vec<OwnedCard>, String> {
        let wallet_pubkey =
            Pubkey::from_str(wallet).map_err(|e| format!("Invalid wallet address: {e}"))?;

        let rpc_url = format!(
            "https://devnet.helius-rpc.com/?api-key={}",
            self.helius_api_key
        );

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "alchemaybe",
            "method": "getAssetsByOwner",
            "params": {
                "ownerAddress": wallet_pubkey.to_string(),
                "page": 1,
                "limit": 1000
            }
        });

        let resp = self
            .http_client
            .post(&rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("DAS request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("DAS returned {status}: {body}"));
        }

        let das_resp: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("DAS parse error: {e}"))?;

        let items = das_resp
            .get("result")
            .and_then(|r| r.get("items"))
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default();

        let collection_str = self.collection_pubkey.to_string();
        let mut cards = Vec::new();

        for item in &items {
            if !is_in_collection(item, &collection_str) {
                continue;
            }

            let card_id = match extract_card_id(item) {
                Some(id) if !id.is_empty() => id,
                _ => continue,
            };

            let id = item.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            let name = extract_name(item);

            cards.push(OwnedCard {
                mint_address: id.to_string(),
                card_id,
                name,
                image: String::new(),
            });
        }

        Ok(cards)
    }

    /// Build a mint transaction for a single card. Server partial-signs.
    /// Returns (base64 serialized transaction, new asset pubkey string).
    pub fn build_mint_tx(
        &self,
        card_id: &str,
        name: &str,
        metadata_uri: &str,
        recipient: &Pubkey,
    ) -> Result<(String, String), String> {
        let asset_keypair = Keypair::new();
        let asset_pubkey = asset_keypair.pubkey();

        let create_ix = CreateV1Builder::new()
            .asset(asset_pubkey)
            .collection(Some(self.collection_pubkey))
            .authority(Some(self.server_keypair.pubkey()))
            .payer(recipient.clone())
            .owner(Some(recipient.clone()))
            .name(name.to_string())
            .uri(metadata_uri.to_string())
            .plugins(vec![PluginAuthorityPair {
                plugin: Plugin::Attributes(Attributes {
                    attribute_list: vec![Attribute {
                        key: "card_id".to_string(),
                        value: card_id.to_string(),
                    }],
                }),
                authority: None,
            }])
            .instruction();

        let recent_blockhash = self
            .rpc_client
            .get_latest_blockhash()
            .map_err(|e| format!("Failed to get blockhash: {e}"))?;

        let mut tx = Transaction::new_with_payer(&[create_ix], Some(recipient));
        tx.partial_sign(&[&*self.server_keypair, &asset_keypair], recent_blockhash);

        let serialized = bincode::serialize(&tx)
            .map_err(|e| format!("Failed to serialize tx: {e}"))?;
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &serialized);

        Ok((b64, asset_pubkey.to_string()))
    }

    /// Build an atomic burn+mint transaction: burns N input NFTs, mints 1 new one.
    /// Server partial-signs. Returns (base64 tx, new asset pubkey string).
    pub fn build_burn_and_mint_tx(
        &self,
        burn_mints: &[Pubkey],
        new_card_id: &str,
        new_name: &str,
        new_metadata_uri: &str,
        owner: &Pubkey,
    ) -> Result<(String, String), String> {
        let mut instructions = Vec::new();

        // Burn instructions for each input NFT
        for mint in burn_mints {
            let burn_ix = BurnV1Builder::new()
                .asset(*mint)
                .collection(Some(self.collection_pubkey))
                .payer(*owner)
                .authority(Some(*owner))
                .instruction();
            instructions.push(burn_ix);
        }

        // Create instruction for the new NFT
        let asset_keypair = Keypair::new();
        let asset_pubkey = asset_keypair.pubkey();

        let create_ix = CreateV1Builder::new()
            .asset(asset_pubkey)
            .collection(Some(self.collection_pubkey))
            .authority(Some(self.server_keypair.pubkey()))
            .payer(owner.clone())
            .owner(Some(owner.clone()))
            .name(new_name.to_string())
            .uri(new_metadata_uri.to_string())
            .plugins(vec![PluginAuthorityPair {
                plugin: Plugin::Attributes(Attributes {
                    attribute_list: vec![Attribute {
                        key: "card_id".to_string(),
                        value: new_card_id.to_string(),
                    }],
                }),
                authority: None,
            }])
            .instruction();
        instructions.push(create_ix);

        let recent_blockhash = self
            .rpc_client
            .get_latest_blockhash()
            .map_err(|e| format!("Failed to get blockhash: {e}"))?;

        let mut tx = Transaction::new_with_payer(&instructions, Some(owner));
        tx.partial_sign(&[&*self.server_keypair, &asset_keypair], recent_blockhash);

        let serialized = bincode::serialize(&tx)
            .map_err(|e| format!("Failed to serialize tx: {e}"))?;
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &serialized);

        Ok((b64, asset_pubkey.to_string()))
    }

    /// Build a SOL payment transaction from buyer to server. Buyer signs.
    pub fn build_payment_tx(
        &self,
        price_lamports: u64,
        buyer: &Pubkey,
    ) -> Result<String, String> {
        let transfer_ix = solana_sdk::system_instruction::transfer(
            buyer,
            &self.server_keypair.pubkey(),
            price_lamports,
        );

        let recent_blockhash = self
            .rpc_client
            .get_latest_blockhash()
            .map_err(|e| format!("Failed to get blockhash: {e}"))?;

        let mut tx = Transaction::new_with_payer(&[transfer_ix], Some(buyer));
        // Only buyer signs — no server signature needed for a simple transfer
        tx.partial_sign(&[] as &[&Keypair], recent_blockhash);

        let serialized = bincode::serialize(&tx)
            .map_err(|e| format!("Failed to serialize tx: {e}"))?;
        Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &serialized))
    }

    /// Mint a card fully server-side (server pays). Returns tx signature and asset pubkey.
    pub fn server_mint(
        &self,
        card_id: &str,
        name: &str,
        metadata_uri: &str,
        recipient: &Pubkey,
    ) -> Result<(String, String), String> {
        let asset_keypair = Keypair::new();
        let asset_pubkey = asset_keypair.pubkey();

        let create_ix = CreateV1Builder::new()
            .asset(asset_pubkey)
            .collection(Some(self.collection_pubkey))
            .authority(Some(self.server_keypair.pubkey()))
            .payer(self.server_keypair.pubkey())
            .owner(Some(*recipient))
            .name(name.to_string())
            .uri(metadata_uri.to_string())
            .plugins(vec![PluginAuthorityPair {
                plugin: Plugin::Attributes(Attributes {
                    attribute_list: vec![Attribute {
                        key: "card_id".to_string(),
                        value: card_id.to_string(),
                    }],
                }),
                authority: None,
            }])
            .instruction();

        let recent_blockhash = self
            .rpc_client
            .get_latest_blockhash()
            .map_err(|e| format!("Failed to get blockhash: {e}"))?;

        let tx = Transaction::new_signed_with_payer(
            &[create_ix],
            Some(&self.server_keypair.pubkey()),
            &[&*self.server_keypair, &asset_keypair],
            recent_blockhash,
        );

        let sig = self
            .rpc_client
            .send_and_confirm_transaction(&tx)
            .map_err(|e| format!("Mint failed: {e}"))?;

        Ok((sig.to_string(), asset_pubkey.to_string()))
    }

    /// Ensure metadata JSON file exists for a card. Returns the public URI.
    pub fn ensure_metadata_json(
        &self,
        card_id: &str,
        name: &str,
        description: &str,
        image_path: &str,
    ) -> Result<String, String> {
        let dir = "cards/metadata";
        let _ = std::fs::create_dir_all(dir);

        let filename = format!("{card_id}.json");
        let disk_path = format!("{dir}/{filename}");
        let public_uri = format!("{}/cards/metadata/{filename}", self.public_base_url);

        // Build image URL from the serve path
        let image_url = if image_path.starts_with("http") {
            image_path.to_string()
        } else {
            format!("{}{image_path}", self.public_base_url)
        };

        let metadata = serde_json::json!({
            "name": name,
            "description": description,
            "image": image_url,
            "attributes": [
                { "trait_type": "card_id", "value": card_id }
            ]
        });

        let data = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("JSON serialize error: {e}"))?;
        std::fs::write(&disk_path, data)
            .map_err(|e| format!("Failed to write metadata: {e}"))?;

        Ok(public_uri)
    }

    /// Submit a fully-signed transaction to the network.
    pub fn submit_transaction(&self, signed_tx_base64: &str) -> Result<String, String> {
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            signed_tx_base64,
        )
        .map_err(|e| format!("Base64 decode error: {e}"))?;

        let tx: Transaction = bincode::deserialize(&bytes)
            .map_err(|e| format!("Transaction deserialize error: {e}"))?;

        let sig = self
            .rpc_client
            .send_and_confirm_transaction(&tx)
            .map_err(|e| format!("Transaction failed: {e}"))?;

        Ok(sig.to_string())
    }
}
