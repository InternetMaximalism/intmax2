use intmax2_interfaces::api::withdrawal_server::interface::{ClaimStatus, WithdrawalStatus};

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "withdrawal_status")]
#[sqlx(rename_all = "snake_case")]
pub enum SqlWithdrawalStatus {
    Requested,
    Relayed,
    Success,
    NeedClaim,
    Failed,
}

impl From<WithdrawalStatus> for SqlWithdrawalStatus {
    fn from(status: WithdrawalStatus) -> Self {
        match status {
            WithdrawalStatus::Requested => SqlWithdrawalStatus::Requested,
            WithdrawalStatus::Relayed => SqlWithdrawalStatus::Relayed,
            WithdrawalStatus::Success => SqlWithdrawalStatus::Success,
            WithdrawalStatus::NeedClaim => SqlWithdrawalStatus::NeedClaim,
            WithdrawalStatus::Failed => SqlWithdrawalStatus::Failed,
        }
    }
}

impl From<SqlWithdrawalStatus> for WithdrawalStatus {
    fn from(val: SqlWithdrawalStatus) -> Self {
        match val {
            SqlWithdrawalStatus::Requested => WithdrawalStatus::Requested,
            SqlWithdrawalStatus::Relayed => WithdrawalStatus::Relayed,
            SqlWithdrawalStatus::Success => WithdrawalStatus::Success,
            SqlWithdrawalStatus::NeedClaim => WithdrawalStatus::NeedClaim,
            SqlWithdrawalStatus::Failed => WithdrawalStatus::Failed,
        }
    }
}

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "claim_status")]
#[sqlx(rename_all = "snake_case")]
pub enum SqlClaimStatus {
    Requested,
    Verified,
    Relayed,
    Success,
    Failed,
}

impl From<ClaimStatus> for SqlClaimStatus {
    fn from(status: ClaimStatus) -> Self {
        match status {
            ClaimStatus::Requested => SqlClaimStatus::Requested,
            ClaimStatus::Verified => SqlClaimStatus::Verified,
            ClaimStatus::Relayed => SqlClaimStatus::Relayed,
            ClaimStatus::Success => SqlClaimStatus::Success,
            ClaimStatus::Failed => SqlClaimStatus::Failed,
        }
    }
}

impl From<SqlClaimStatus> for ClaimStatus {
    fn from(val: SqlClaimStatus) -> Self {
        match val {
            SqlClaimStatus::Requested => ClaimStatus::Requested,
            SqlClaimStatus::Verified => ClaimStatus::Verified,
            SqlClaimStatus::Relayed => ClaimStatus::Relayed,
            SqlClaimStatus::Success => ClaimStatus::Success,
            SqlClaimStatus::Failed => ClaimStatus::Failed,
        }
    }
}
