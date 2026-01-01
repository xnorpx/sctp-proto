use crate::chunk::{chunk_init::ChunkInit, Chunk};
use crate::util::{AssociationIdGenerator, RandomAssociationIdGenerator};

use bytes::Bytes;
use std::fmt;
use std::sync::{Arc, OnceLock};

/// MTU for inbound packet (from DTLS)
pub(crate) const RECEIVE_MTU: usize = 8192;
/// initial MTU for outgoing packets (to DTLS)
pub(crate) const INITIAL_MTU: u32 = 1228;
pub(crate) const INITIAL_RECV_BUF_SIZE: u32 = 1024 * 1024;
pub(crate) const COMMON_HEADER_SIZE: u32 = 12;
pub(crate) const DATA_CHUNK_HEADER_SIZE: u32 = 16;
pub(crate) const DEFAULT_MAX_MESSAGE_SIZE: u32 = 65536;

// Default RTO values in milliseconds (RFC 4960)
pub(crate) const RTO_INITIAL: u64 = 3000;
pub(crate) const RTO_MIN: u64 = 1000;
pub(crate) const RTO_MAX: u64 = 60000;

// Default max retransmit value (RFC 4960 Section 15)
const DEFAULT_MAX_INIT_RETRANS: usize = 8;

/// Config collects the arguments to create_association construction into
/// a single structure
pub struct TransportConfig {
    max_receive_buffer_size: u32,
    max_message_size: u32,
    max_num_outbound_streams: u16,
    max_num_inbound_streams: u16,

    /// Maximum number of retransmissions for INIT chunks during handshake.
    /// Set to `None` for unlimited retries (recommended for WebRTC).
    /// Default: Some(8)
    max_init_retransmits: Option<usize>,

    /// Maximum number of retransmissions for DATA chunks.
    /// Set to `None` for unlimited retries (recommended for WebRTC).
    /// Default: None (unlimited)
    max_data_retransmits: Option<usize>,

    /// Initial retransmission timeout in milliseconds.
    /// Default: 3000
    rto_initial_ms: u64,

    /// Minimum retransmission timeout in milliseconds.
    /// Default: 1000
    rto_min_ms: u64,

    /// Maximum retransmission timeout in milliseconds.
    /// Default: 60000
    rto_max_ms: u64,

    /// Cached INIT chunk - generated once at construction time.
    /// This ensures that `marshalled_chunk_init()` and `chunk_init()` return
    /// the same INIT chunk with consistent initiate_tag and initial_tsn values,
    /// which is critical for out-of-band signaling flows.
    cached_chunk_init: OnceLock<ChunkInit>,
}

impl fmt::Debug for TransportConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransportConfig")
            .field("max_receive_buffer_size", &self.max_receive_buffer_size)
            .field("max_message_size", &self.max_message_size)
            .field("max_num_outbound_streams", &self.max_num_outbound_streams)
            .field("max_num_inbound_streams", &self.max_num_inbound_streams)
            .field("max_init_retransmits", &self.max_init_retransmits)
            .field("max_data_retransmits", &self.max_data_retransmits)
            .field("rto_initial_ms", &self.rto_initial_ms)
            .field("rto_min_ms", &self.rto_min_ms)
            .field("rto_max_ms", &self.rto_max_ms)
            .field("cached_chunk_init", &self.cached_chunk_init.get())
            .finish()
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        TransportConfig {
            max_receive_buffer_size: INITIAL_RECV_BUF_SIZE,
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
            max_num_outbound_streams: u16::MAX,
            max_num_inbound_streams: u16::MAX,
            max_init_retransmits: Some(DEFAULT_MAX_INIT_RETRANS),
            max_data_retransmits: None,
            rto_initial_ms: RTO_INITIAL,
            rto_min_ms: RTO_MIN,
            rto_max_ms: RTO_MAX,
            cached_chunk_init: OnceLock::new(),
        }
    }
}

impl TransportConfig {
    pub fn with_max_receive_buffer_size(mut self, value: u32) -> Self {
        self.max_receive_buffer_size = value;
        self
    }

    pub fn with_max_message_size(mut self, value: u32) -> Self {
        self.max_message_size = value;
        self
    }

    pub fn with_max_num_outbound_streams(mut self, value: u16) -> Self {
        self.max_num_outbound_streams = value;
        self
    }

    pub fn with_max_num_inbound_streams(mut self, value: u16) -> Self {
        self.max_num_inbound_streams = value;
        self
    }

    /// Returns the marshalled INIT chunk bytes for out-of-band signaling.
    ///
    /// This method returns the cached INIT chunk, ensuring that the same
    /// initiate_tag and initial_tsn values are used consistently. This is
    /// critical for out-of-band signaling where the INIT bytes exchanged
    /// via signaling must match what the association actually uses.
    pub fn marshalled_chunk_init(&self) -> Result<Bytes, crate::error::Error> {
        let chunk = self.chunk_init();
        chunk.marshal()
    }

    /// Returns a clone of the cached INIT chunk.
    ///
    /// The INIT chunk is generated once (lazily on first access) and cached,
    /// ensuring consistent initiate_tag and initial_tsn values across all
    /// calls. This is essential for out-of-band signaling flows where the
    /// same INIT must be used both for signaling and association creation.
    pub(crate) fn chunk_init(&self) -> ChunkInit {
        self.cached_chunk_init
            .get_or_init(|| {
                let mut chunk_init = ChunkInit {
                    num_outbound_streams: self.max_num_outbound_streams,
                    num_inbound_streams: self.max_num_inbound_streams,
                    advertised_receiver_window_credit: self.max_receive_buffer_size,
                    ..Default::default()
                };
                chunk_init.set_supported_extensions();
                chunk_init
            })
            .clone()
    }

    pub(crate) fn max_receive_buffer_size(&self) -> u32 {
        self.max_receive_buffer_size
    }

    pub(crate) fn max_message_size(&self) -> u32 {
        self.max_message_size
    }

    pub(crate) fn max_num_outbound_streams(&self) -> u16 {
        self.max_num_outbound_streams
    }

    pub(crate) fn max_num_inbound_streams(&self) -> u16 {
        self.max_num_inbound_streams
    }

    /// Set maximum INIT retransmissions. `None` means unlimited.
    pub fn with_max_init_retransmits(mut self, value: Option<usize>) -> Self {
        self.max_init_retransmits = value;
        self
    }

    /// Set maximum DATA retransmissions. `None` means unlimited.
    pub fn with_max_data_retransmits(mut self, value: Option<usize>) -> Self {
        self.max_data_retransmits = value;
        self
    }

    /// Set initial RTO in milliseconds.
    pub fn with_rto_initial_ms(mut self, value: u64) -> Self {
        self.rto_initial_ms = value;
        self
    }

    /// Set minimum RTO in milliseconds.
    pub fn with_rto_min_ms(mut self, value: u64) -> Self {
        self.rto_min_ms = value;
        self
    }

    /// Set maximum RTO in milliseconds.
    pub fn with_rto_max_ms(mut self, value: u64) -> Self {
        self.rto_max_ms = value;
        self
    }

    pub(crate) fn max_init_retransmits(&self) -> Option<usize> {
        self.max_init_retransmits
    }

    pub(crate) fn max_data_retransmits(&self) -> Option<usize> {
        self.max_data_retransmits
    }

    pub(crate) fn rto_initial_ms(&self) -> u64 {
        self.rto_initial_ms
    }

    pub(crate) fn rto_min_ms(&self) -> u64 {
        self.rto_min_ms
    }

    pub(crate) fn rto_max_ms(&self) -> u64 {
        self.rto_max_ms
    }
}

/// Global configuration for the endpoint, affecting all associations
///
/// Default values should be suitable for most internet applications.
#[derive(Clone)]
pub struct EndpointConfig {
    pub(crate) max_payload_size: u32,

    /// AID generator factory
    ///
    /// Create a aid generator for local aid in Endpoint struct
    pub(crate) aid_generator_factory:
        Arc<dyn Fn() -> Box<dyn AssociationIdGenerator> + Send + Sync>,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl EndpointConfig {
    /// Create a default config
    pub fn new() -> Self {
        let aid_factory: fn() -> Box<dyn AssociationIdGenerator> =
            || Box::<RandomAssociationIdGenerator>::default();
        Self {
            max_payload_size: INITIAL_MTU - (COMMON_HEADER_SIZE + DATA_CHUNK_HEADER_SIZE),
            aid_generator_factory: Arc::new(aid_factory),
        }
    }

    /// Supply a custom Association ID generator factory
    ///
    /// Called once by each `Endpoint` constructed from this configuration to obtain the AID
    /// generator which will be used to generate the AIDs used for incoming packets on all
    /// associations involving that  `Endpoint`. A custom AID generator allows applications to embed
    /// information in local association IDs, e.g. to support stateless packet-level load balancers.
    ///
    /// `EndpointConfig::new()` applies a default random AID generator factory. This functions
    /// accepts any customized AID generator to reset AID generator factory that implements
    /// the `AssociationIdGenerator` trait.
    pub fn aid_generator<F: Fn() -> Box<dyn AssociationIdGenerator> + Send + Sync + 'static>(
        &mut self,
        factory: F,
    ) -> &mut Self {
        self.aid_generator_factory = Arc::new(factory);
        self
    }

    /// Maximum payload size accepted from peers.
    ///
    /// The default is suitable for typical internet applications. Applications which expect to run
    /// on networks supporting Ethernet jumbo frames or similar should set this appropriately.
    pub fn max_payload_size(&mut self, value: u32) -> &mut Self {
        self.max_payload_size = value;
        self
    }

    /// Get the current value of `max_payload_size`
    ///
    /// While most parameters don't need to be readable, this must be exposed to allow higher-level
    /// layers to determine how large a receive buffer to allocate to
    /// support an externally-defined `EndpointConfig`.
    ///
    /// While `get_` accessors are typically unidiomatic in Rust, we favor concision for setters,
    /// which will be used far more heavily.
    #[doc(hidden)]
    pub fn get_max_payload_size(&self) -> u32 {
        self.max_payload_size
    }
}

impl fmt::Debug for EndpointConfig {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("EndpointConfig")
            .field("max_payload_size", &self.max_payload_size)
            .field("aid_generator_factory", &"[ elided ]")
            .finish()
    }
}

/// Parameters governing incoming associations
///
/// Default values should be suitable for most internet applications.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Transport configuration to use for incoming associations
    pub transport: Arc<TransportConfig>,

    /// Maximum number of concurrent associations
    pub(crate) concurrent_associations: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            transport: Arc::new(TransportConfig::default()),
            concurrent_associations: 100_000,
        }
    }
}

impl ServerConfig {
    /// Create a default config with a particular handshake token key
    pub fn new() -> Self {
        ServerConfig::default()
    }
}

/// Configuration for outgoing associations
///
/// Default values should be suitable for most internet applications.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Transport configuration to use
    pub transport: Arc<TransportConfig>,
    /// Optional remote chunk init.
    ///
    /// When provided, the association can skip the usual SCTP 4-way handshake and
    /// immediately transition to the ESTABLISHED state.
    ///
    pub remote_chunk_init: Option<Bytes>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            transport: Arc::new(TransportConfig::default()),
            remote_chunk_init: None,
        }
    }
}

impl ClientConfig {
    /// Create a default config with a particular cryptographic config
    pub fn new() -> Self {
        ClientConfig::default()
    }

    /// Configure remote chunk init parameters.
    ///
    /// When enabled, both the local and remote INIT chunks must be provided
    /// (typically exchanged via a signaling channel). The association can skip
    /// the SCTP 4-way handshake and immediately transition to the ESTABLISHED state.
    pub fn with_remote_chunk_init(mut self, remote_chunk_init: Bytes) -> Self {
        self.remote_chunk_init = Some(remote_chunk_init);
        self
    }
}
