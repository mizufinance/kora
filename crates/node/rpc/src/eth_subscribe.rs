//! WebSocket subscription support for `eth_subscribe` / `eth_unsubscribe`.

use alloy_primitives::{Address, B256};
use jsonrpsee::{PendingSubscriptionSink, SubscriptionMessage, proc_macros::rpc};
use tokio::sync::broadcast;
use tracing::trace;

use crate::types::{AddressFilter, RpcBlock, RpcLog, TopicFilter};

/// Parsed log filter for subscription-time matching.
#[derive(Clone, Debug, Default)]
struct SubscriptionLogFilter {
    /// Addresses to match (empty = match all).
    addresses: Vec<Address>,
    /// Positional topic filters. Each position is OR-matched within, AND across.
    topics: Vec<Vec<B256>>,
}

/// Parameters for `eth_subscribe("logs", ...)`.
#[derive(Clone, Debug, Default, serde::Deserialize)]
struct LogSubscriptionParams {
    /// Contract address or list of addresses to filter by.
    #[serde(default)]
    address: Option<AddressFilter>,
    /// Topics to filter by.
    #[serde(default)]
    topics: Option<Vec<Option<TopicFilter>>>,
}

impl From<LogSubscriptionParams> for SubscriptionLogFilter {
    fn from(params: LogSubscriptionParams) -> Self {
        let addresses = params.address.map(AddressFilter::into_vec).unwrap_or_default();
        let topics = params
            .topics
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.map(TopicFilter::into_vec).unwrap_or_default())
            .collect();
        Self { addresses, topics }
    }
}

/// Returns `true` if the log matches the subscription filter.
fn matches_filter(log: &RpcLog, filter: &SubscriptionLogFilter) -> bool {
    // Address check: if addresses are specified, log must match one.
    if !filter.addresses.is_empty() && !filter.addresses.contains(&log.address) {
        return false;
    }

    // Topic check: positional AND, with OR within each position.
    for (i, position_topics) in filter.topics.iter().enumerate() {
        if position_topics.is_empty() {
            // Wildcard position — matches any topic.
            continue;
        }
        match log.topics.get(i) {
            Some(log_topic) => {
                if !position_topics.contains(log_topic) {
                    return false;
                }
            }
            // Log has fewer topics than the filter requires.
            None => return false,
        }
    }

    true
}

/// Ethereum subscription JSON-RPC API.
#[rpc(server, namespace = "eth")]
pub trait EthSubscriptionApi {
    /// Subscribe to events. Returns a subscription ID.
    #[subscription(name = "subscribe" => "subscription", unsubscribe = "unsubscribe", item = serde_json::Value)]
    async fn subscribe(
        &self,
        kind: String,
        params: Option<serde_json::Value>,
    ) -> jsonrpsee::core::SubscriptionResult;
}

/// Implementation of `eth_subscribe` / `eth_unsubscribe`.
pub struct EthSubscriptionApiImpl {
    heads_tx: broadcast::Sender<RpcBlock>,
    logs_tx: broadcast::Sender<Vec<RpcLog>>,
}

impl std::fmt::Debug for EthSubscriptionApiImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EthSubscriptionApiImpl").finish_non_exhaustive()
    }
}

impl EthSubscriptionApiImpl {
    /// Create a new subscription API implementation.
    pub const fn new(
        heads_tx: broadcast::Sender<RpcBlock>,
        logs_tx: broadcast::Sender<Vec<RpcLog>>,
    ) -> Self {
        Self { heads_tx, logs_tx }
    }
}

#[jsonrpsee::core::async_trait]
impl EthSubscriptionApiServer for EthSubscriptionApiImpl {
    async fn subscribe(
        &self,
        pending: PendingSubscriptionSink,
        kind: String,
        params: Option<serde_json::Value>,
    ) -> jsonrpsee::core::SubscriptionResult {
        match kind.as_str() {
            "newHeads" => {
                let sink = pending.accept().await?;
                let mut rx = self.heads_tx.subscribe();
                tokio::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Ok(block) => {
                                let Ok(value) = serde_json::to_value(&block) else {
                                    break;
                                };
                                let Ok(msg) = SubscriptionMessage::from_json(&value) else {
                                    break;
                                };
                                if sink.send(msg).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                trace!(lagged = n, "newHeads subscriber lagged, dropping");
                                break;
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
            }
            "logs" => {
                let filter: SubscriptionLogFilter = params
                    .map(|v| {
                        serde_json::from_value::<LogSubscriptionParams>(v)
                            .unwrap_or_default()
                            .into()
                    })
                    .unwrap_or_default();

                let sink = pending.accept().await?;
                let mut rx = self.logs_tx.subscribe();
                tokio::spawn(async move {
                    loop {
                        match rx.recv().await {
                            Ok(logs) => {
                                for log in &logs {
                                    if !matches_filter(log, &filter) {
                                        continue;
                                    }
                                    let Ok(value) = serde_json::to_value(log) else {
                                        return;
                                    };
                                    let Ok(msg) = SubscriptionMessage::from_json(&value) else {
                                        return;
                                    };
                                    if sink.send(msg).await.is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                trace!(lagged = n, "logs subscriber lagged, dropping");
                                break;
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
            }
            other => {
                let _ = pending
                    .reject(jsonrpsee::types::ErrorObjectOwned::owned(
                        crate::error::codes::INVALID_PARAMS,
                        format!("unsupported subscription kind: {other}"),
                        None::<()>,
                    ))
                    .await;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, B256, Bytes, U64};

    use super::*;

    fn make_log(address: Address, topics: Vec<B256>) -> RpcLog {
        RpcLog {
            address,
            topics,
            data: Bytes::new(),
            block_number: U64::from(1),
            transaction_hash: B256::ZERO,
            transaction_index: U64::ZERO,
            block_hash: B256::ZERO,
            log_index: U64::ZERO,
            removed: false,
        }
    }

    #[test]
    fn empty_filter_matches_all() {
        let filter = SubscriptionLogFilter::default();
        let log = make_log(Address::repeat_byte(0x01), vec![B256::repeat_byte(0xaa)]);
        assert!(matches_filter(&log, &filter));
    }

    #[test]
    fn address_filter_matches() {
        let addr = Address::repeat_byte(0x42);
        let filter = SubscriptionLogFilter { addresses: vec![addr], ..Default::default() };
        let log = make_log(addr, vec![]);
        assert!(matches_filter(&log, &filter));
    }

    #[test]
    fn address_filter_rejects() {
        let addr = Address::repeat_byte(0x42);
        let other = Address::repeat_byte(0x99);
        let filter = SubscriptionLogFilter { addresses: vec![addr], ..Default::default() };
        let log = make_log(other, vec![]);
        assert!(!matches_filter(&log, &filter));
    }

    #[test]
    fn multiple_addresses_or_match() {
        let a1 = Address::repeat_byte(0x01);
        let a2 = Address::repeat_byte(0x02);
        let filter = SubscriptionLogFilter { addresses: vec![a1, a2], ..Default::default() };
        assert!(matches_filter(&make_log(a1, vec![]), &filter));
        assert!(matches_filter(&make_log(a2, vec![]), &filter));
        assert!(!matches_filter(&make_log(Address::repeat_byte(0x03), vec![]), &filter));
    }

    #[test]
    fn single_topic_filter_matches() {
        let topic = B256::repeat_byte(0xaa);
        let filter = SubscriptionLogFilter { topics: vec![vec![topic]], ..Default::default() };
        let log = make_log(Address::ZERO, vec![topic]);
        assert!(matches_filter(&log, &filter));
    }

    #[test]
    fn single_topic_filter_rejects() {
        let topic = B256::repeat_byte(0xaa);
        let other = B256::repeat_byte(0xbb);
        let filter = SubscriptionLogFilter { topics: vec![vec![topic]], ..Default::default() };
        let log = make_log(Address::ZERO, vec![other]);
        assert!(!matches_filter(&log, &filter));
    }

    #[test]
    fn wildcard_topic_position() {
        let topic1 = B256::repeat_byte(0xbb);
        // Position 0 = wildcard (empty), position 1 = specific topic
        let filter =
            SubscriptionLogFilter { topics: vec![vec![], vec![topic1]], ..Default::default() };
        let log = make_log(Address::ZERO, vec![B256::repeat_byte(0xff), topic1]);
        assert!(matches_filter(&log, &filter));
    }

    #[test]
    fn topic_or_within_position() {
        let t1 = B256::repeat_byte(0x01);
        let t2 = B256::repeat_byte(0x02);
        let filter = SubscriptionLogFilter { topics: vec![vec![t1, t2]], ..Default::default() };
        assert!(matches_filter(&make_log(Address::ZERO, vec![t1]), &filter));
        assert!(matches_filter(&make_log(Address::ZERO, vec![t2]), &filter));
        assert!(!matches_filter(&make_log(Address::ZERO, vec![B256::repeat_byte(0x03)]), &filter));
    }

    #[test]
    fn log_fewer_topics_than_filter_rejects() {
        let topic = B256::repeat_byte(0xaa);
        let filter =
            SubscriptionLogFilter { topics: vec![vec![], vec![topic]], ..Default::default() };
        // Log only has 1 topic, filter needs 2 positions.
        let log = make_log(Address::ZERO, vec![B256::ZERO]);
        assert!(!matches_filter(&log, &filter));
    }

    #[test]
    fn combined_address_and_topic_filter() {
        let addr = Address::repeat_byte(0x42);
        let topic = B256::repeat_byte(0xaa);
        let filter = SubscriptionLogFilter { addresses: vec![addr], topics: vec![vec![topic]] };
        // Matches both.
        assert!(matches_filter(&make_log(addr, vec![topic]), &filter));
        // Wrong address.
        assert!(!matches_filter(&make_log(Address::repeat_byte(0x99), vec![topic]), &filter));
        // Wrong topic.
        assert!(!matches_filter(&make_log(addr, vec![B256::repeat_byte(0xbb)]), &filter));
    }

    #[test]
    fn log_subscription_params_deserialize() {
        let json = serde_json::json!({
            "address": "0x4242424242424242424242424242424242424242",
            "topics": [
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                null
            ]
        });
        let params: LogSubscriptionParams = serde_json::from_value(json).unwrap();
        assert!(params.address.is_some());
        let topics = params.topics.unwrap();
        assert_eq!(topics.len(), 2);
        assert!(topics[0].is_some());
        assert!(topics[1].is_none());
    }

    #[test]
    fn log_subscription_params_to_filter() {
        let params = LogSubscriptionParams {
            address: Some(AddressFilter::Single(Address::repeat_byte(0x42))),
            topics: Some(vec![Some(TopicFilter::Single(B256::repeat_byte(0xaa))), None]),
        };
        let filter: SubscriptionLogFilter = params.into();
        assert_eq!(filter.addresses.len(), 1);
        assert_eq!(filter.topics.len(), 2);
        assert_eq!(filter.topics[0].len(), 1);
        assert!(filter.topics[1].is_empty()); // None -> wildcard
    }
}
