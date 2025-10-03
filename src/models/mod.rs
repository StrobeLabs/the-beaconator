pub mod app_state;
pub mod requests;
pub mod responses;

pub use app_state::{ApiEndpoints, ApiSummary, AppState, EndpointInfo, EndpointStatus, PerpConfig};
pub use requests::{
    BatchCreatePerpcityBeaconRequest, BatchDeployPerpsForBeaconsRequest,
    BatchDepositLiquidityForPerpsRequest, BatchUpdateBeaconRequest, BeaconUpdateData,
    CreateBeaconRequest, CreateVerifiableBeaconRequest, DeployPerpForBeaconRequest,
    DepositLiquidityForPerpRequest, FundGuestWalletRequest, RegisterBeaconRequest,
    UpdateBeaconRequest,
};
pub use responses::{
    ApiResponse, BatchCreatePerpcityBeaconResponse, BatchDeployPerpsForBeaconsResponse,
    BatchDepositLiquidityForPerpsResponse, BatchUpdateBeaconResponse, BeaconUpdateResult,
    DeployPerpForBeaconResponse, DepositLiquidityForPerpResponse,
};
